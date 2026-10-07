//! Query string extraction with protobuf field paths.

use std::collections::BTreeMap;

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
/// Values stay strings, matching axum's `Query` for non-nested fields.
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
    let mut root = BTreeMap::new();
    for (key, value) in form_urlencoded::parse(query.as_bytes()) {
        insert(&mut root, &key, value.into_owned())?;
    }
    T::deserialize(Node::Branch(root))
}

fn insert(map: &mut BTreeMap<String, Node>, key: &str, value: String) -> Result<(), Error> {
    let (head, rest) = match key.split_once('.') {
        Some((head, rest)) => (head, Some(rest)),
        None => (key, None),
    };
    if head.is_empty() {
        return Err(serde::de::Error::custom(format!(
            "invalid field path {key:?}"
        )));
    }
    match rest {
        None => match map.insert(head.to_owned(), Node::Leaf(value)) {
            None => Ok(()),
            Some(_) => Err(serde::de::Error::custom(format!(
                "duplicate field {head:?}"
            ))),
        },
        Some(rest) => match map
            .entry(head.to_owned())
            .or_insert_with(|| Node::Branch(BTreeMap::new()))
        {
            Node::Branch(child) => insert(child, rest, value),
            Node::Leaf(_) => Err(serde::de::Error::custom(format!(
                "field {head:?} is both a value and a message"
            ))),
        },
    }
}

enum Node {
    Leaf(String),
    Branch(BTreeMap<String, Node>),
}

impl<'de> Deserializer<'de> for Node {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Node::Leaf(value) => visitor.visit_string(value),
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

impl<'de> IntoDeserializer<'de, Error> for Node {
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
        outer: Outer,
    }

    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(default)]
    struct Outer {
        id: String,
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
                outer: Outer {
                    id: "b".into(),
                    inner: Some(Inner {
                        value: "c d".into()
                    }),
                },
            }
        );
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
