//! Runtime helpers for generated REST/OpenAPI endpoint wrappers over
//! ConnectRPC services.

use axum::body::Body;
use buffa::view::{MessageView, OwnedView};
use connectrpc::{CodecFormat, ConnectError, Encodable, RequestContext, Response, ServiceResult};
use http::header::CONTENT_TYPE;
use http::{Extensions, HeaderMap, StatusCode};

const JSON_CONTENT_TYPE: &str = "application/json";

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
/// This performs Buffa's encode-then-decode conversion. In generated REST
/// handlers this happens after Axum has deserialized path/query/body data into
/// an owned request value.
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

/// Converts a Connect service result into a JSON HTTP response.
///
/// The response body is encoded through ConnectRPC's own [`Encodable`] contract
/// with [`CodecFormat::Json`], so generated REST handlers support any body type
/// the service trait accepts for the target protobuf output message.
pub fn service_response<M, B>(response: ServiceResult<B>) -> http::Response<Body>
where
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
    B: Encodable<M>,
{
    match response.encode::<M>(CodecFormat::Json) {
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

fn add_trailers(
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
    use connectrpc::{ErrorCode, Response};
    use http::header::{CONTENT_TYPE, HeaderValue};

    use super::{VERSION, error_response, request_context, service_response};

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

    #[derive(Clone, Debug)]
    struct JsonNumber;

    impl connectrpc::Encodable<JsonNumber> for JsonNumber {
        fn encode(
            &self,
            codec: connectrpc::CodecFormat,
        ) -> Result<Bytes, connectrpc::ConnectError> {
            match codec {
                connectrpc::CodecFormat::Proto => Ok(Bytes::new()),
                connectrpc::CodecFormat::Json => Ok(Bytes::from_static(b"42")),
                _ => Err(connectrpc::ConnectError::internal(
                    "unsupported codec in test",
                )),
            }
        }
    }
}
