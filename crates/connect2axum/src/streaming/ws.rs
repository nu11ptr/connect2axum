//! JSON WebSocket streaming helpers for generated REST adapters.

use axum::extract::{
    WebSocketUpgrade,
    ws::{
        CloseFrame, Message as WsMessage, WebSocket,
        close_code::{AGAIN, AWAY, ERROR, INVALID, NORMAL, POLICY, SIZE, UNSUPPORTED},
    },
};
use axum::response::Response as AxumResponse;
use buffa::Message;
use buffa::view::{MessageView, OwnedView};
use connectrpc::{
    ConnectError, Encodable, ErrorCode, Response as ConnectResponse, ServiceResult, ServiceStream,
};
use futures_util::{
    SinkExt as _, StreamExt as _,
    stream::{SplitSink, SplitStream},
};
use http::{Extensions, HeaderMap};
use serde::{Serialize, de::DeserializeOwned};

use crate::{encode_json_compatible, json_owned_view};

const CLOSE_REASON_MAX_BYTES: usize = 123;

enum WsItem<V>
where
    V: MessageView<'static>,
{
    Item(OwnedView<V>),
    End,
    Skip,
}

/// Upgrades an axum WebSocket request and passes the split stream/sink to a
/// generated handler callback.
pub async fn upgrade_to_ws<C, Fut>(
    ws_upgrade: WebSocketUpgrade,
    headers: HeaderMap,
    extensions: Extensions,
    callback: C,
) -> AxumResponse
where
    C: FnOnce(
            HeaderMap,
            Extensions,
            SplitStream<WebSocket>,
            SplitSink<WebSocket, WsMessage>,
        ) -> Fut
        + Send
        + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    ws_upgrade.on_upgrade(move |socket| async move {
        let (sink, stream) = socket.split();
        callback(headers, extensions, stream, sink).await;
    })
}

/// Reads the first JSON text frame as a complete Buffa request message.
pub async fn make_ws_request<V>(
    mut ws: SplitStream<WebSocket>,
) -> Result<OwnedView<V>, ConnectError>
where
    V: MessageView<'static>,
    V::Owned: DeserializeOwned,
{
    while let Some(message) = ws.next().await {
        match convert_ws_to_item::<V>(message)? {
            WsItem::Item(item) => return Ok(item),
            WsItem::End => break,
            WsItem::Skip => {}
        }
    }

    Err(ConnectError::invalid_argument(
        "WebSocket closed before a request message was received",
    ))
}

/// Converts JSON text frames into the request stream expected by generated
/// ConnectRPC streaming handlers.
///
/// Client and bidirectional streaming WebSocket clients can end the request
/// stream without closing the socket by sending an empty text frame.
#[must_use]
pub fn make_ws_stream_request<V>(ws: SplitStream<WebSocket>) -> ServiceStream<OwnedView<V>>
where
    V: MessageView<'static> + Send + 'static,
    V::Owned: DeserializeOwned + Send + 'static,
    OwnedView<V>: Send + 'static,
{
    Box::pin(async_stream::stream! {
        let mut ws = ws;
        while let Some(message) = ws.next().await {
            match convert_ws_to_item::<V>(message) {
                Ok(WsItem::Item(item)) => yield Ok(item),
                Ok(WsItem::End) => break,
                Ok(WsItem::Skip) => {}
                Err(err) => {
                    yield Err(err);
                    break;
                }
            }
        }
    })
}

/// Sends a unary ConnectRPC response as one JSON text frame, then closes the
/// WebSocket.
pub async fn process_ws_response<M, B>(
    response: ServiceResult<B>,
    mut ws: SplitSink<WebSocket, WsMessage>,
) where
    M: Message + Serialize,
    B: Encodable<M>,
{
    let result = handle_ws_response::<M, B>(response, &mut ws).await;
    finish_ws(&mut ws, result.err()).await;
}

/// Sends a ConnectRPC response stream as JSON text frames, then closes the
/// WebSocket.
pub async fn process_ws_stream_response<M, B>(
    response: ServiceResult<ServiceStream<B>>,
    mut ws: SplitSink<WebSocket, WsMessage>,
) where
    M: Message + Serialize + 'static,
    B: Encodable<M> + Send + 'static,
{
    let result = handle_ws_stream_response::<M, B>(response, &mut ws).await;
    finish_ws(&mut ws, result.err()).await;
}

/// Closes a WebSocket connection with the close code derived from a ConnectRPC
/// error.
pub async fn close_ws(mut ws: SplitSink<WebSocket, WsMessage>, err: ConnectError) {
    finish_ws(&mut ws, Some(err)).await;
}

/// Converts a ConnectRPC error into a WebSocket close frame.
#[must_use]
pub fn connect_error_to_ws_close_frame(err: &ConnectError) -> CloseFrame {
    let code = match err.code {
        ErrorCode::Canceled => AWAY,
        ErrorCode::InvalidArgument | ErrorCode::FailedPrecondition | ErrorCode::OutOfRange => {
            INVALID
        }
        ErrorCode::PermissionDenied | ErrorCode::Unauthenticated => POLICY,
        ErrorCode::ResourceExhausted => SIZE,
        ErrorCode::Unimplemented => UNSUPPORTED,
        ErrorCode::DeadlineExceeded | ErrorCode::Unavailable => AGAIN,
        _ => ERROR,
    };

    CloseFrame {
        code,
        reason: close_reason(err).into(),
    }
}

async fn handle_ws_response<M, B>(
    response: ServiceResult<B>,
    ws: &mut SplitSink<WebSocket, WsMessage>,
) -> Result<(), ConnectError>
where
    M: Message + Serialize,
    B: Encodable<M>,
{
    match response {
        Ok(response) => {
            let ConnectResponse { body, .. } = response;
            let text = encode_ws_msg::<M, B>(&body)?;
            send_ws_text(ws, text).await
        }
        Err(err) => Err(err),
    }
}

async fn handle_ws_stream_response<M, B>(
    response: ServiceResult<ServiceStream<B>>,
    ws: &mut SplitSink<WebSocket, WsMessage>,
) -> Result<(), ConnectError>
where
    M: Message + Serialize + 'static,
    B: Encodable<M> + Send + 'static,
{
    let ConnectResponse {
        body: mut items, ..
    } = response?;

    while let Some(item) = items.next().await {
        let text = {
            let item = item?;
            encode_ws_msg::<M, B>(&item)?
        };
        send_ws_text(ws, text).await?;
    }

    Ok(())
}

fn encode_ws_msg<M, B>(body: &B) -> Result<String, ConnectError>
where
    M: Message + Serialize,
    B: Encodable<M>,
{
    let body = encode_json_compatible::<M, B>(body)?;
    String::from_utf8(body.to_vec()).map_err(|err| {
        ConnectError::internal(format!("failed to encode WebSocket JSON response: {err}"))
    })
}

async fn send_ws_text(
    ws: &mut SplitSink<WebSocket, WsMessage>,
    text: String,
) -> Result<(), ConnectError> {
    ws.send(WsMessage::Text(text.into())).await.map_err(|err| {
        ConnectError::unavailable(format!("failed to send WebSocket response frame: {err}"))
    })
}

async fn finish_ws(ws: &mut SplitSink<WebSocket, WsMessage>, err: Option<ConnectError>) {
    let frame = match err.as_ref() {
        Some(err) => connect_error_to_ws_close_frame(err),
        None => CloseFrame {
            code: NORMAL,
            reason: "".into(),
        },
    };
    let _ = ws.send(WsMessage::Close(Some(frame))).await;
    let _ = ws.close().await;
}

fn convert_ws_to_item<V>(message: Result<WsMessage, axum::Error>) -> Result<WsItem<V>, ConnectError>
where
    V: MessageView<'static>,
    V::Owned: DeserializeOwned,
{
    match message {
        Ok(WsMessage::Text(message)) if message.as_str().is_empty() => Ok(WsItem::End),
        Ok(WsMessage::Text(message)) => json_owned_view::<V>(message.as_str().as_bytes())
            .map(WsItem::Item)
            .map_err(|err| {
                ConnectError::invalid_argument(format!(
                    "failed to decode WebSocket JSON request frame: {err}"
                ))
            }),
        Ok(WsMessage::Binary(_)) => Err(ConnectError::invalid_argument(
            "binary WebSocket messages are not supported; send JSON text frames",
        )),
        Ok(WsMessage::Close(close_frame)) => match close_frame_to_error(close_frame) {
            Some(err) => Err(err),
            None => Ok(WsItem::End),
        },
        Ok(WsMessage::Ping(_) | WsMessage::Pong(_)) => Ok(WsItem::Skip),
        Err(err) => Err(ConnectError::unavailable(format!(
            "failed to receive WebSocket frame: {err}"
        ))),
    }
}

fn close_frame_to_error(close_frame: Option<CloseFrame>) -> Option<ConnectError> {
    close_frame.and_then(|frame| {
        (frame.code != NORMAL)
            .then(|| ConnectError::aborted(format!("WebSocket closed with code {}", frame.code)))
    })
}

fn close_reason(err: &ConnectError) -> String {
    let reason = err.to_string();
    if reason.len() <= CLOSE_REASON_MAX_BYTES {
        return reason;
    }

    let mut end = CLOSE_REASON_MAX_BYTES;
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    reason[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use axum::extract::ws::close_code::{AGAIN, ERROR, INVALID, NORMAL, POLICY, SIZE, UNSUPPORTED};
    use connectrpc::ConnectError;

    use super::{close_frame_to_error, connect_error_to_ws_close_frame};

    #[test]
    fn maps_connect_errors_to_websocket_close_codes() {
        assert_eq!(
            connect_error_to_ws_close_frame(&ConnectError::invalid_argument("bad")).code,
            INVALID
        );
        assert_eq!(
            connect_error_to_ws_close_frame(&ConnectError::permission_denied("no")).code,
            POLICY
        );
        assert_eq!(
            connect_error_to_ws_close_frame(&ConnectError::resource_exhausted("full")).code,
            SIZE
        );
        assert_eq!(
            connect_error_to_ws_close_frame(&ConnectError::unimplemented("missing")).code,
            UNSUPPORTED
        );
        assert_eq!(
            connect_error_to_ws_close_frame(&ConnectError::unavailable("later")).code,
            AGAIN
        );
        assert_eq!(
            connect_error_to_ws_close_frame(&ConnectError::internal("boom")).code,
            ERROR
        );
    }

    #[test]
    fn normal_close_frame_is_stream_end() {
        assert!(close_frame_to_error(None).is_none());
        assert!(
            close_frame_to_error(Some(axum::extract::ws::CloseFrame {
                code: NORMAL,
                reason: "".into(),
            }))
            .is_none()
        );
    }

    #[test]
    fn truncates_close_reasons_to_protocol_limit() {
        let err = ConnectError::internal("x".repeat(256));
        let frame = connect_error_to_ws_close_frame(&err);

        assert!(frame.reason.as_str().len() <= 123);
    }
}
