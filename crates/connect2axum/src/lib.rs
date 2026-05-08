//! Runtime helpers for generated REST/OpenAPI endpoint wrappers over
//! ConnectRPC services.

use axum::body::Body;
use buffa::Message;
use buffa::bytes::{Bytes, BytesMut};
use buffa::view::{MessageView, OwnedView};
use connectrpc::{
    CodecFormat, ConnectError, Encodable, ErrorCode, RequestContext, Response, ServiceResult,
};
use http::header::CONTENT_TYPE;
use http::{Extensions, HeaderMap, StatusCode};
use percent_encoding::percent_decode_str;
use serde::Serialize;
use serde::de::{self, DeserializeOwned, DeserializeSeed, IgnoredAny, MapAccess, Visitor};

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

/// Converts protobuf wire bytes into the `OwnedView<...View<'static>>` request
/// type expected by generated Connect service traits.
pub fn owned_view_from_bytes<V>(bytes: Bytes) -> Result<OwnedView<V>, ConnectError>
where
    V: MessageView<'static>,
{
    OwnedView::<V>::decode(bytes).map_err(|err| {
        ConnectError::invalid_argument(format!(
            "failed to decode REST request protobuf bytes into Buffa view: {err}"
        ))
    })
}

/// Converts a generated wire request into the `OwnedView<...View<'static>>`
/// request type expected by generated Connect service traits.
pub fn owned_view_from_wire<V>(request: WireRequest) -> Result<OwnedView<V>, ConnectError>
where
    V: MessageView<'static>,
{
    owned_view_from_bytes(request.into_bytes())
}

/// Incremental protobuf request buffer used by generated REST transcoders.
#[derive(Clone, Debug, Default)]
pub struct WireRequest {
    buf: BytesMut,
}

impl WireRequest {
    /// Create an empty protobuf wire request.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buf: BytesMut::new(),
        }
    }

    /// Append a string field, skipping protobuf's default empty value.
    pub fn push_string(&mut self, field_number: u32, value: &str) {
        if value.is_empty() {
            return;
        }

        buffa::encoding::Tag::new(field_number, buffa::encoding::WireType::LengthDelimited)
            .encode(&mut self.buf);
        buffa::types::encode_string(value, &mut self.buf);
    }

    /// Percent-decodes a path capture and appends it as a string field.
    pub fn push_path_string(
        &mut self,
        field_number: u32,
        encoded_value: &str,
    ) -> Result<(), ConnectError> {
        let value = percent_decode_str(encoded_value)
            .decode_utf8()
            .map_err(|err| {
                ConnectError::invalid_argument(format!(
                    "failed to percent-decode REST path field: {err}"
                ))
            })?;
        self.push_string(field_number, value.as_ref());
        Ok(())
    }

    /// Appends a query string field.
    ///
    /// Both protobuf JSON names and original proto field names are accepted.
    /// Query values are form-url-decoded and encoded directly into the protobuf
    /// buffer.
    pub fn push_query_string(
        &mut self,
        raw_query: Option<&str>,
        json_name: &str,
        proto_name: &str,
        field_number: u32,
    ) {
        let Some(raw_query) = raw_query else {
            return;
        };

        for (key, value) in form_urlencoded::parse(raw_query.as_bytes()) {
            if key == json_name || key == proto_name {
                self.push_string(field_number, value.as_ref());
            }
        }
    }

    /// Parses a protobuf-JSON string body and appends it to the request.
    ///
    /// This is used for `google.api.http` bindings such as `body: "name"`
    /// where the request body is the JSON representation of that field, not an
    /// object containing the field.
    pub fn push_json_body_string(
        &mut self,
        body: &[u8],
        field_number: u32,
    ) -> Result<(), ConnectError> {
        let mut deserializer = serde_json::Deserializer::from_slice(body);
        JsonStringSeed {
            request: self,
            field_number,
        }
        .deserialize(&mut deserializer)
        .map_err(|err| {
            ConnectError::invalid_argument(format!(
                "failed to decode JSON string request body: {err}"
            ))
        })?;
        deserializer.end().map_err(|err| {
            ConnectError::invalid_argument(format!(
                "failed to decode JSON string request body: {err}"
            ))
        })
    }

    /// Parses a protobuf-JSON object field and appends it to the request.
    pub fn push_json_object_string_field(
        &mut self,
        body: &[u8],
        json_name: &str,
        proto_name: &str,
        field_number: u32,
    ) -> Result<(), ConnectError> {
        let mut deserializer = serde_json::Deserializer::from_slice(body);
        JsonObjectStringFieldSeed {
            request: self,
            json_name,
            proto_name,
            field_number,
        }
        .deserialize(&mut deserializer)
        .map_err(|err| {
            ConnectError::invalid_argument(format!(
                "failed to decode JSON object request body: {err}"
            ))
        })?;
        deserializer.end().map_err(|err| {
            ConnectError::invalid_argument(format!(
                "failed to decode JSON object request body: {err}"
            ))
        })
    }

    /// Finish the request buffer.
    #[must_use]
    pub fn into_bytes(self) -> Bytes {
        self.buf.freeze()
    }
}

/// Lightweight path matcher used by generated direct request transcoders.
#[derive(Clone, Debug)]
pub struct PathDecoder<'a> {
    remaining: &'a str,
}

impl<'a> PathDecoder<'a> {
    /// Create a decoder over an HTTP request path.
    #[must_use]
    pub fn new(path: &'a str) -> Self {
        Self { remaining: path }
    }

    /// Capture the next value between a known prefix and suffix.
    pub fn capture(&mut self, prefix: &str, suffix: &str) -> Result<&'a str, ConnectError> {
        let Some(after_prefix) = self.remaining.strip_prefix(prefix) else {
            return Err(ConnectError::invalid_argument(format!(
                "REST path did not match expected prefix {prefix:?}"
            )));
        };

        if suffix.is_empty() {
            self.remaining = "";
            return Ok(after_prefix);
        }

        let Some(index) = after_prefix.find(suffix) else {
            return Err(ConnectError::invalid_argument(format!(
                "REST path did not match expected suffix {suffix:?}"
            )));
        };
        let (value, after_value) = after_prefix.split_at(index);
        self.remaining = &after_value[suffix.len()..];
        Ok(value)
    }
}

/// Read a raw Axum request body into bytes.
pub async fn body_bytes(body: Body) -> Result<Bytes, ConnectError> {
    axum::body::to_bytes(body, usize::MAX).await.map_err(|err| {
        ConnectError::invalid_argument(format!("failed to read REST request body: {err}"))
    })
}

/// Decode a raw REST JSON request body with serde.
pub fn json_body<T>(body: &[u8]) -> Result<T, ConnectError>
where
    T: DeserializeOwned,
{
    serde_json::from_slice(body).map_err(|err| {
        ConnectError::invalid_argument(format!("failed to decode REST JSON body: {err}"))
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

/// Converts a Connect service result into a JSON HTTP response with view-body
/// fallback support.
pub fn compatible_service_response<M, B>(response: ServiceResult<B>) -> http::Response<Body>
where
    M: Message + Serialize,
    B: Encodable<M>,
{
    match response {
        Ok(response) => compatible_json_response::<M, B>(response),
        Err(err) => error_response(err),
    }
}

/// Converts a successful Connect response into JSON, falling back through
/// protobuf bytes when the body can encode protobuf but not JSON directly.
pub fn compatible_json_response<M, B>(response: Response<B>) -> http::Response<Body>
where
    M: Message + Serialize,
    B: Encodable<M>,
{
    match encode_compatible_response::<M, B>(response) {
        Ok(response) => encoded_json_response(response),
        Err(err) => error_response(err),
    }
}

/// Body wrapper for service implementations that want to return a Buffa view
/// without breaking JSON clients.
#[derive(Clone, Debug)]
pub struct JsonCompatibleView<V> {
    view: V,
}

/// Wrap a view body so its [`Encodable`] implementation can provide JSON
/// compatibility.
#[must_use]
pub fn json_compatible_view<V>(view: V) -> JsonCompatibleView<V> {
    JsonCompatibleView { view }
}

/// Trait implemented by generated REST modules for Buffa output views.
pub trait JsonCompatibleBody<M> {
    /// Encode the body as protobuf wire bytes.
    fn encode_proto(&self) -> Bytes;

    /// Encode the body in the requested Connect codec.
    fn encode_compatible(&self, codec: CodecFormat) -> Result<Bytes, ConnectError>
    where
        M: Message + Serialize,
    {
        match codec {
            CodecFormat::Proto => Ok(self.encode_proto()),
            CodecFormat::Json => json_from_proto::<M>(self.encode_proto()),
            _ => Err(ConnectError::unimplemented(
                "unsupported response codec format",
            )),
        }
    }
}

impl<M, V> Encodable<M> for JsonCompatibleView<V>
where
    M: Message + Serialize,
    V: JsonCompatibleBody<M>,
{
    fn encode(&self, codec: CodecFormat) -> Result<Bytes, ConnectError> {
        self.view.encode_compatible(codec)
    }
}

/// Convert protobuf wire bytes for `M` into protobuf-JSON bytes.
pub fn json_from_proto<M>(bytes: Bytes) -> Result<Bytes, ConnectError>
where
    M: Message + Serialize,
{
    let message = M::decode_from_slice(&bytes).map_err(|err| {
        ConnectError::internal(format!(
            "failed to decode protobuf response for JSON fallback: {err}"
        ))
    })?;
    serde_json::to_vec(&message)
        .map(Bytes::from)
        .map_err(|err| {
            ConnectError::internal(format!("failed to encode protobuf response as JSON: {err}"))
        })
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

fn encode_compatible_response<M, B>(
    response: Response<B>,
) -> Result<connectrpc::EncodedResponse, ConnectError>
where
    M: Message + Serialize,
    B: Encodable<M>,
{
    let body = match response.body.encode(CodecFormat::Json) {
        Ok(body) => body,
        Err(err) if err.code == ErrorCode::Unimplemented => {
            let proto = response.body.encode(CodecFormat::Proto)?;
            json_from_proto::<M>(proto)?
        }
        Err(err) => return Err(err),
    };

    Ok(Response {
        body,
        headers: response.headers,
        trailers: response.trailers,
        compress: response.compress,
    })
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

struct JsonStringSeed<'a> {
    request: &'a mut WireRequest,
    field_number: u32,
}

impl<'de> DeserializeSeed<'de> for JsonStringSeed<'_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_option(JsonStringVisitor {
            request: self.request,
            field_number: self.field_number,
        })
    }
}

struct JsonStringVisitor<'a> {
    request: &'a mut WireRequest,
    field_number: u32,
}

impl<'de> Visitor<'de> for JsonStringVisitor<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a JSON string or null")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(())
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(RequiredStringVisitor {
            request: self.request,
            field_number: self.field_number,
        })
    }
}

struct RequiredStringVisitor<'a> {
    request: &'a mut WireRequest,
    field_number: u32,
}

impl<'de> Visitor<'de> for RequiredStringVisitor<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a JSON string")
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.request.push_string(self.field_number, value);
        Ok(())
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.request.push_string(self.field_number, value);
        Ok(())
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.request.push_string(self.field_number, &value);
        Ok(())
    }
}

struct JsonObjectStringFieldSeed<'a> {
    request: &'a mut WireRequest,
    json_name: &'a str,
    proto_name: &'a str,
    field_number: u32,
}

impl<'de> DeserializeSeed<'de> for JsonObjectStringFieldSeed<'_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(JsonObjectStringFieldVisitor {
            request: self.request,
            json_name: self.json_name,
            proto_name: self.proto_name,
            field_number: self.field_number,
        })
    }
}

struct JsonObjectStringFieldVisitor<'a> {
    request: &'a mut WireRequest,
    json_name: &'a str,
    proto_name: &'a str,
    field_number: u32,
}

impl<'de> Visitor<'de> for JsonObjectStringFieldVisitor<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a JSON object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let request = self.request;
        while let Some(key) = map.next_key::<String>()? {
            if key == self.json_name || key == self.proto_name {
                map.next_value_seed(JsonStringSeed {
                    request: &mut *request,
                    field_number: self.field_number,
                })?;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use buffa::bytes::Bytes;
    use connectrpc::{ErrorCode, Response};
    use http::header::{CONTENT_TYPE, HeaderValue};

    use super::{VERSION, WireRequest, error_response, request_context, service_response};

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
    fn wire_request_encodes_query_and_json_body_strings() {
        let mut request = WireRequest::new();

        request.push_query_string(Some("salutation=Ahoy+There"), "salutation", "salutation", 1);
        request
            .push_json_body_string(br#""Doe""#, 3)
            .expect("json string body");

        assert_eq!(
            request.into_bytes().as_ref(),
            b"\x0a\x0aAhoy There\x1a\x03Doe"
        );
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
