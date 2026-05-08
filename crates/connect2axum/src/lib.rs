//! Runtime helpers for generated REST/OpenAPI endpoint wrappers over
//! ConnectRPC services.

use axum::body::Body;
use buffa::Message;
use buffa::view::{MessageView, OwnedView};
use connectrpc::{
    CodecFormat, ConnectError, Encodable, ErrorCode, RequestContext, Response, ServiceResult,
};
use http::header::CONTENT_TYPE;
use http::{Extensions, HeaderMap, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub mod streaming;

const JSON_CONTENT_TYPE: &str = "application/json";

pub use streaming::http::{JsonLines, ndjson_request_stream, stream_response};
pub use streaming::ws::{
    close_ws, connect_error_to_ws_close_frame, make_ws_request, make_ws_stream_request,
    process_ws_response, process_ws_stream_response, upgrade_to_ws,
};

/// The crate version, as declared by Cargo.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Converts the parts of an HTTP request into a ConnectRPC request context.
///
/// Connect Rust exposes request headers and extensions directly on
/// [`RequestContext`], so generated REST handlers can preserve both without
/// translating through Connect's wire protocol layer.
#[must_use]
pub fn request_context(headers: HeaderMap, extensions: Extensions) -> RequestContext {
    RequestContext::new(headers).with_extensions(extensions)
}

/// Converts an owned Buffa message into the `OwnedView<...View<'static>>`
/// request type expected by generated Connect service traits.
///
/// This mirrors ConnectRPC's JSON request path for view-based handlers:
/// JSON is deserialized into the Buffa owned message, then Buffa re-encodes it
/// as protobuf bytes and decodes an [`OwnedView`] over those bytes. Protobuf
/// clients can decode directly from protobuf bytes, but REST JSON starts from
/// owned Buffa values to preserve Buffa's ProtoJSON behavior.
pub fn owned_view<V>(message: &V::Owned) -> Result<OwnedView<V>, ConnectError>
where
    V: MessageView<'static>,
{
    OwnedView::<V>::from_owned(message).map_err(|err| {
        ConnectError::internal(format!(
            "failed to convert REST request into Buffa view: {err}"
        ))
    })
}

/// Deserializes ProtoJSON into a Buffa owned message and converts it to an
/// `OwnedView`.
///
/// Generated REST handlers usually let Axum deserialize request bodies into
/// Buffa-owned request parts. This helper exists for raw-body paths that need
/// to follow the exact same shape as ConnectRPC's JSON view decoder.
pub fn json_owned_view<V>(body: &[u8]) -> Result<OwnedView<V>, ConnectError>
where
    V: MessageView<'static>,
    V::Owned: DeserializeOwned,
{
    let message = serde_json::from_slice::<V::Owned>(body).map_err(|err| {
        ConnectError::invalid_argument(format!("failed to decode JSON request: {err}"))
    })?;
    owned_view::<V>(&message)
}

/// Wraps a response body so JSON encoding can fall back through the Buffa owned
/// message when the inner body is a view.
///
/// ConnectRPC-generated view response bodies support protobuf encoding, but
/// return `Unimplemented` for JSON because Buffa views are protobuf-wire views,
/// not serde values. This wrapper keeps protobuf output direct and handles JSON
/// by encoding protobuf, decoding the owned output message, then serializing
/// that owned message with Buffa's ProtoJSON serde implementation.
#[derive(Clone, Debug)]
pub struct JsonCompatibleView<B> {
    body: B,
}

/// Creates a [`JsonCompatibleView`] response wrapper.
pub fn json_compatible_view<B>(body: B) -> JsonCompatibleView<B> {
    JsonCompatibleView { body }
}

impl<M, B> Encodable<M> for JsonCompatibleView<B>
where
    M: Message + Serialize,
    B: Encodable<M>,
{
    fn encode(&self, codec: CodecFormat) -> Result<buffa::bytes::Bytes, ConnectError> {
        match codec {
            CodecFormat::Proto => self.body.encode(CodecFormat::Proto),
            CodecFormat::Json => encode_json_compatible::<M, B>(&self.body),
            _ => self.body.encode(codec),
        }
    }
}

/// Converts a Connect service result into a JSON HTTP response.
///
/// The response body is encoded through ConnectRPC's own [`Encodable`] contract
/// with [`CodecFormat::Json`] first. If the body is a view that only supports
/// protobuf, REST JSON falls back through the Buffa owned output message.
pub fn service_response<M, B>(response: ServiceResult<B>) -> http::Response<Body>
where
    M: Message + Serialize,
    B: Encodable<M>,
{
    match response {
        Ok(response) => json_response::<M, B>(response),
        Err(err) => error_response(err),
    }
}

/// Converts a successful Connect response into a JSON HTTP response.
pub fn json_response<M, B>(response: Response<B>) -> http::Response<Body>
where
    M: Message + Serialize,
    B: Encodable<M>,
{
    let Response {
        body,
        headers,
        trailers,
        compress,
    } = response;

    match encode_json_compatible::<M, B>(&body).map(|body| Response {
        body,
        headers,
        trailers,
        compress,
    }) {
        Ok(response) => encoded_json_response(response),
        Err(err) => error_response(err),
    }
}

/// Converts a Connect error into an HTTP JSON error response.
#[must_use]
pub fn error_response(err: ConnectError) -> http::Response<Body> {
    let status = err.http_status();
    let body = err.to_json();
    let mut response = http::Response::builder()
        .status(status)
        .header(CONTENT_TYPE, JSON_CONTENT_TYPE);

    for (key, value) in err.response_headers() {
        response = response.header(key, value);
    }

    let response = add_trailers(response, err.trailers());
    response.body(Body::from(body)).unwrap_or_else(|_| {
        http::Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .expect("static empty error response should build")
    })
}

fn encoded_json_response(response: connectrpc::EncodedResponse) -> http::Response<Body> {
    let mut builder = http::Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, JSON_CONTENT_TYPE);

    for (key, value) in response.headers.iter() {
        builder = builder.header(key, value);
    }

    let builder = add_trailers(builder, &response.trailers);
    builder
        .body(Body::from(response.body))
        .unwrap_or_else(|_| error_response(ConnectError::internal("failed to build REST response")))
}

pub(crate) fn encode_json_compatible<M, B>(body: &B) -> Result<buffa::bytes::Bytes, ConnectError>
where
    M: Message + Serialize,
    B: Encodable<M>,
{
    match body.encode(CodecFormat::Json) {
        Ok(body) => Ok(body),
        Err(err) if err.code == ErrorCode::Unimplemented => {
            let proto = body.encode(CodecFormat::Proto)?;
            let owned = M::decode_from_slice(&proto).map_err(|err| {
                ConnectError::internal(format!(
                    "failed to decode protobuf response for JSON fallback: {err}"
                ))
            })?;
            serde_json::to_vec(&owned)
                .map(buffa::bytes::Bytes::from)
                .map_err(|err| {
                    ConnectError::internal(format!("failed to encode JSON response: {err}"))
                })
        }
        Err(err) => Err(err),
    }
}

pub(crate) fn add_trailers(
    mut response: http::response::Builder,
    trailers: &HeaderMap,
) -> http::response::Builder {
    for (key, value) in trailers {
        let trailer_key = format!("trailer-{}", key.as_str());
        response = response.header(trailer_key, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use buffa::bytes::Bytes;
    use buffa::encoding::Tag;
    use buffa::{DecodeError, DefaultInstance, Message, SizeCache};
    use connectrpc::{Encodable as _, ErrorCode, Response};
    use http::header::{CONTENT_TYPE, HeaderValue};
    use serde::Serialize;

    use super::{VERSION, error_response, json_compatible_view, request_context, service_response};

    #[test]
    fn exposes_package_version() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn request_context_preserves_headers_and_extensions() {
        #[derive(Clone, Debug, PartialEq)]
        struct RequestId(u64);

        let mut headers = http::HeaderMap::new();
        headers.insert("x-request-id", HeaderValue::from_static("abc"));
        let mut extensions = http::Extensions::new();
        extensions.insert(RequestId(7));

        let ctx = request_context(headers, extensions);

        assert_eq!(ctx.header("x-request-id").unwrap(), "abc");
        assert_eq!(ctx.extensions.get::<RequestId>(), Some(&RequestId(7)));
    }

    #[test]
    fn error_response_maps_connect_error_to_json_http_response() {
        let response = error_response(connectrpc::ConnectError::not_found("missing"));

        assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }

    #[test]
    fn error_response_preserves_headers_and_trailers() {
        let mut headers = http::HeaderMap::new();
        headers.insert("x-error", HeaderValue::from_static("yes"));
        let mut trailers = http::HeaderMap::new();
        trailers.insert("x-trailer", HeaderValue::from_static("later"));
        let err = connectrpc::ConnectError::internal("boom")
            .with_headers(headers)
            .with_trailers(trailers);

        let response = error_response(err);

        assert_eq!(response.headers().get("x-error").unwrap(), "yes");
        assert_eq!(
            response.headers().get("trailer-x-trailer").unwrap(),
            "later"
        );
    }

    #[test]
    fn service_response_encodes_success_as_json() {
        let response = service_response::<JsonNumber, _>(Ok(Response::new(JsonNumber)));

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }

    #[test]
    fn service_response_maps_errors() {
        let response = service_response::<JsonNumber, JsonNumber>(Err(
            connectrpc::ConnectError::new(ErrorCode::InvalidArgument, "bad input"),
        ));

        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn json_compatible_view_falls_back_through_owned_message() {
        let encoded = json_compatible_view(ProtoOnly)
            .encode(connectrpc::CodecFormat::Json)
            .expect("JSON fallback encodes");

        assert_eq!(encoded, Bytes::from_static(b"42"));
    }

    #[test]
    fn service_response_uses_json_fallback_for_proto_only_body() {
        let response = service_response::<JsonNumber, _>(Ok(Response::new(ProtoOnly)));

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }

    #[derive(Clone, Debug, Default, PartialEq)]
    struct JsonNumber;

    impl Serialize for JsonNumber {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            serializer.serialize_u8(42)
        }
    }

    impl DefaultInstance for JsonNumber {
        fn default_instance() -> &'static Self {
            static VALUE: std::sync::OnceLock<JsonNumber> = std::sync::OnceLock::new();
            VALUE.get_or_init(JsonNumber::default)
        }
    }

    impl Message for JsonNumber {
        fn compute_size(&self, _cache: &mut SizeCache) -> u32 {
            0
        }

        fn write_to(&self, _cache: &mut SizeCache, _buf: &mut impl buffa::bytes::BufMut) {}

        fn merge_field(
            &mut self,
            _tag: Tag,
            _buf: &mut impl buffa::bytes::Buf,
            _depth: u32,
        ) -> Result<(), DecodeError> {
            Err(DecodeError::UnexpectedEof)
        }

        fn clear(&mut self) {}
    }

    #[derive(Clone, Debug)]
    struct ProtoOnly;

    impl connectrpc::Encodable<JsonNumber> for ProtoOnly {
        fn encode(
            &self,
            codec: connectrpc::CodecFormat,
        ) -> Result<Bytes, connectrpc::ConnectError> {
            match codec {
                connectrpc::CodecFormat::Proto => Ok(Bytes::new()),
                connectrpc::CodecFormat::Json => Err(connectrpc::ConnectError::unimplemented(
                    "views do not support JSON in test",
                )),
                _ => Err(connectrpc::ConnectError::internal(
                    "unsupported codec in test",
                )),
            }
        }
    }
}
