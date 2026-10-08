//! Query string extraction with protobuf field paths.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use axum::body::Body;
use axum::extract::FromRequestParts;
use connectrpc::ConnectError;
use http::request::Parts;
use serde::de::value::{Error, MapDeserializer, SeqDeserializer};
use serde::de::{DeserializeOwned, Deserializer, IntoDeserializer, Visitor};

use crate::error_response;

/// Query extractor that accepts dotted protobuf field paths such as
/// `?datasource.symbol.ticker=AAPL`, deserializing them into nested messages.
///
/// Values are passed as strings, except `true`/`false` for fields that reject
/// a string (protobuf `bool` fields). Repeated keys, or a single value for a
/// field that rejects a scalar, bind repeated fields.
#[derive(Clone, Debug, Default)]
pub struct ProtoQuery<T>(pub T);

impl<T, S> FromRequestParts<S> for ProtoQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = http::Response<Body>;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        from_query_str(parts.uri.query().unwrap_or_default())
            .map(ProtoQuery)
            .map_err(|err| {
                error_response(ConnectError::invalid_argument(format!(
                    "invalid query string: {err}"
                )))
            })
    }
}

/// Deserializes a URL query string whose keys may be dotted field paths.
pub fn from_query_str<T: DeserializeOwned>(query: &str) -> Result<T, Error> {
    let pairs: Vec<(String, String)> = form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    // Query values carry no type, and ProtoJSON bool and repeated fields
    // reject strings. A rejected value is retried as a bool (`true`/`false`)
    // or as a one-element list. If retrying fails, the first error is kept.
    let hints = Hints::default();
    let mut first_err = None;
    loop {
        let mut root = BTreeMap::new();
        for (key, value) in &pairs {
            let leaf = Node::Leaf {
                key: key.clone(),
                values: vec![value.clone()],
                element: false,
                hints: &hints,
            };
            insert(&mut root, key, leaf)?;
        }
        let known = hints.len();
        match T::deserialize(Node::Branch(root)) {
            Err(err) if hints.len() > known => {
                first_err.get_or_insert(err);
            }
            Err(err) => return Err(first_err.unwrap_or(err)),
            result => return result,
        }
    }
}

fn insert<'a>(
    map: &mut BTreeMap<String, Node<'a>>,
    path: &str,
    leaf: Node<'a>,
) -> Result<(), Error> {
    let (head, rest) = match path.split_once('.') {
        Some((head, rest)) => (head, Some(rest)),
        None => (path, None),
    };
    if head.is_empty() {
        return Err(serde::de::Error::custom(format!(
            "invalid field path {path:?}"
        )));
    }
    match rest {
        None => match (map.get_mut(head), leaf) {
            (None, leaf) => {
                map.insert(head.to_owned(), leaf);
                Ok(())
            }
            (Some(Node::Leaf { values, .. }), Node::Leaf { values: more, .. }) => {
                values.extend(more);
                Ok(())
            }
            _ => Err(serde::de::Error::custom(format!(
                "field {head:?} is both a value and a message"
            ))),
        },
        Some(rest) => match map
            .entry(head.to_owned())
            .or_insert_with(|| Node::Branch(BTreeMap::new()))
        {
            Node::Branch(child) => insert(child, rest, leaf),
            Node::Leaf { .. } => Err(serde::de::Error::custom(format!(
                "field {head:?} is both a value and a message"
            ))),
        },
    }
}

#[derive(Default)]
struct Hints {
    bools: RefCell<BTreeSet<String>>,
    lists: RefCell<BTreeSet<String>>,
}

impl Hints {
    fn len(&self) -> usize {
        self.bools.borrow().len() + self.lists.borrow().len()
    }
}

enum Node<'a> {
    Leaf {
        key: String,
        values: Vec<String>,
        // List elements are never wrapped in another list.
        element: bool,
        hints: &'a Hints,
    },
    Branch(BTreeMap<String, Node<'a>>),
}

impl<'de> Deserializer<'de> for Node<'_> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Node::Leaf {
                key,
                values,
                element,
                hints,
            } => {
                if !element && (values.len() > 1 || hints.lists.borrow().contains(&key)) {
                    let items = values.into_iter().map(|value| Node::Leaf {
                        key: key.clone(),
                        values: vec![value],
                        element: true,
                        hints,
                    });
                    return visitor.visit_seq(SeqDeserializer::new(items));
                }
                let value = values.concat();
                let is_bool = value == "true" || value == "false";
                let result = if hints.bools.borrow().contains(&key) {
                    visitor.visit_bool(value == "true")
                } else {
                    visitor.visit_string(value)
                };
                if result.is_err() {
                    let retry_as_bool = is_bool && hints.bools.borrow_mut().insert(key.clone());
                    if !retry_as_bool && !element {
                        hints.lists.borrow_mut().insert(key);
                    }
                }
                result
            }
            Node::Branch(map) => visitor.visit_map(MapDeserializer::new(map.into_iter())),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_some(self)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

impl<'de, 'a> IntoDeserializer<'de, Error> for Node<'a> {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::from_query_str;

    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(default)]
    struct Request {
        name: String,
        enabled: bool,
        ids: Vec<String>,
        outer: Outer,
    }

    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(default)]
    struct Outer {
        id: String,
        active: bool,
        inner: Option<Inner>,
    }

    #[derive(Debug, Default, Deserialize, PartialEq)]
    struct Inner {
        value: String,
    }

    #[test]
    fn deserializes_dotted_paths_into_nested_messages() {
        let request: Request = from_query_str("name=a&outer.id=b&outer.inner.value=c%20d").unwrap();

        assert_eq!(
            request,
            Request {
                name: "a".into(),
                enabled: false,
                ids: Vec::new(),
                outer: Outer {
                    id: "b".into(),
                    active: false,
                    inner: Some(Inner {
                        value: "c d".into()
                    }),
                },
            }
        );
    }

    #[test]
    fn true_and_false_bind_bools_without_affecting_strings() {
        let request: Request =
            from_query_str("name=true&enabled=true&outer.id=false&outer.active=true").unwrap();

        assert_eq!(request.name, "true");
        assert!(request.enabled);
        assert_eq!(request.outer.id, "false");
        assert!(request.outer.active);
        assert!(from_query_str::<Request>("enabled=yes").is_err());
    }

    #[test]
    fn repeated_keys_and_single_values_bind_lists() {
        let request: Request = from_query_str("ids=a&ids=b").unwrap();
        assert_eq!(request.ids, ["a", "b"]);

        let request: Request = from_query_str("ids=a").unwrap();
        assert_eq!(request.ids, ["a"]);
    }

    #[test]
    fn rejects_duplicate_and_conflicting_paths() {
        for query in [
            "name=a&name=b",
            "outer=a&outer.id=b",
            "outer.id=b&outer=a",
            ".id=a",
        ] {
            assert!(from_query_str::<Request>(query).is_err(), "{query}");
        }
    }
}
