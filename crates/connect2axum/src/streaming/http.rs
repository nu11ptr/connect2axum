//! NDJSON HTTP streaming helpers for generated REST adapters.

use axum::body::Body;
use buffa::Message;
use buffa::view::{MessageView, OwnedView};
use bytes::Bytes;
use connectrpc::{ConnectError, Encodable, Response, ServiceResult};
use futures_util::StreamExt as _;
use http::StatusCode;
use http::header::CONTENT_TYPE;
use serde::Serialize;

use crate::{add_trailers, encode_json_compatible, error_response, owned_view};

pub use axum_extra::json_lines::JsonLines;

/// Converts an NDJSON request body into the streaming request type expected by
/// generated Connect service traits.
///
/// Axum Extra handles the NDJSON framing and deserializes each line with
/// Buffa's serde helpers. Each owned message is then converted to an
/// [`OwnedView`] exactly like unary REST JSON bodies.
#[must_use]
pub fn ndjson_request_stream<V>(
    lines: JsonLines<V::Owned>,
) -> connectrpc::ServiceStream<OwnedView<V>>
where
    V: MessageView<'static> + Send + 'static,
    V::Owned: Send + 'static,
    OwnedView<V>: Send + 'static,
{
    Box::pin(lines.map(|line| match line {
        Ok(message) => owned_view::<V>(&message),
        Err(err) => Err(ConnectError::invalid_argument(format!(
            "failed to decode NDJSON request body: {err}"
        ))),
    }))
}

/// Converts a Connect streaming result into an NDJSON HTTP response.
///
/// Successful stream items are encoded through the same JSON compatibility path
/// as unary REST responses. If the service stream yields a [`ConnectError`]
/// after the response has started, the error is emitted as the final NDJSON
/// line using Connect's JSON error shape.
pub fn stream_response<M, B>(
    response: ServiceResult<connectrpc::ServiceStream<B>>,
    content_type: &str,
) -> http::Response<Body>
where
    M: Message + Serialize + 'static,
    B: Encodable<M> + Send + 'static,
{
    match response {
        Ok(response) => ndjson_response::<M, B>(response, content_type),
        Err(err) => error_response(err),
    }
}

fn ndjson_response<M, B>(
    response: Response<connectrpc::ServiceStream<B>>,
    content_type: &str,
) -> http::Response<Body>
where
    M: Message + Serialize + 'static,
    B: Encodable<M> + Send + 'static,
{
    let Response {
        body: mut items,
        headers,
        trailers,
        compress: _,
    } = response;
    let body = async_stream::stream! {
        while let Some(item) = items.next().await {
            match item {
                Ok(item) => match encode_json_compatible::<M, B>(&item) {
                    Ok(body) => yield Ok::<Bytes, ConnectError>(ndjson_line(body)),
                    Err(err) => {
                        yield Err(err);
                        break;
                    }
                },
                Err(err) => {
                    yield Ok(ndjson_line(err.to_json()));
                    break;
                }
            }
        }
    };

    let mut builder = http::Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type);

    for (key, value) in &headers {
        builder = builder.header(key, value);
    }

    let builder = add_trailers(builder, &trailers);
    builder
        .body(Body::from_stream(body))
        .unwrap_or_else(|_| error_response(ConnectError::internal("failed to build REST stream")))
}

fn ndjson_line(body: Bytes) -> Bytes {
    let mut line = Vec::with_capacity(body.len() + 1);
    line.extend_from_slice(&body);
    line.push(b'\n');
    Bytes::from(line)
}
