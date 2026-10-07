//! Query string extraction with protobuf field paths.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use axum::body::Body;
use axum::extract::FromRequestParts;
use connectrpc::ConnectError;
use http::request::Parts;
use serde::de::value::{Error, MapDeserializer};
use serde::de::{DeserializeOwned, Deserializer, IntoDeserializer, Visitor};

use crate::error_response;

/// Query extractor that accepts dotted protobuf field paths such as
/// `?datasource.symbol.ticker=AAPL`, deserializing them into nested messages.
///
/// Values are passed as strings, except `true`/`false` for fields that reject
/// a string (protobuf `bool` fields).
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
    // Query values carry no type, and ProtoJSON bool fields reject strings.
    // A `true`/`false` value rejected as a string is retried as a bool.
    let bool_keys = RefCell::new(BTreeSet::new());
    loop {
        let mut root = BTreeMap::new();
        for (key, value) in &pairs {
            let leaf = Node::Leaf {
                key: key.clone(),
                value: value.clone(),
                bool_keys: &bool_keys,
            };
            insert(&mut root, key, leaf)?;
        }
        let known_bools = bool_keys.borrow().len();
        match T::deserialize(Node::Branch(root)) {
            Err(_) if bool_keys.borrow().len() > known_bools => continue,
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
        None => match map.insert(head.to_owned(), leaf) {
            None => Ok(()),
            Some(_) => Err(serde::de::Error::custom(format!(
                "duplicate field {head:?}"
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

enum Node<'a> {
    Leaf {
        key: String,
        value: String,
        bool_keys: &'a RefCell<BTreeSet<String>>,
    },
    Branch(BTreeMap<String, Node<'a>>),
}

impl<'de> Deserializer<'de> for Node<'_> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Node::Leaf {
                key,
                value,
                bool_keys,
            } => {
                if bool_keys.borrow().contains(&key) {
                    return visitor.visit_bool(value == "true");
                }
                let is_bool = value == "true" || value == "false";
                let result = visitor.visit_string(value);
                if result.is_err() && is_bool {
                    bool_keys.borrow_mut().insert(key);
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
