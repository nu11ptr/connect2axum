use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use connectrpc::{RequestContext, Response, ServiceResult, ServiceStream};
use futures_util::StreamExt as _;

#[rustfmt::skip]
#[path = "generated/connect/mod.rs"]
pub mod connect;

#[rustfmt::skip]
#[path = "generated/buffa/mod.rs"]
pub mod proto;

#[rustfmt::skip]
#[path = "generated/connect2axum/streaming/v1/ws_streaming.connect2rest.rs"]
pub mod rest;

#[rustfmt::skip]
#[path = "generated/connect2axum/streaming/v1/ws_streaming.connect2ws.rs"]
pub mod ws;

use connect::streaming::v1::{GreeterServiceExt as _, OwnedHelloRequestView};
use proto::streaming::v1::{HelloReply, HelloSummary};

#[derive(Clone, Debug, Default)]
pub struct Greeter;

impl connect::streaming::v1::GreeterService for Greeter {
    async fn expand(
        &self,
        _ctx: RequestContext,
        request: OwnedHelloRequestView,
    ) -> ServiceResult<ServiceStream<HelloReply>> {
        let stream = futures_util::stream::iter([
            Ok(reply(&request, "Hello")),
            Ok(reply(&request, "Welcome aboard")),
        ]);
        Response::stream_ok(stream)
    }

    async fn collect<'a>(
        &'a self,
        _ctx: RequestContext,
        mut requests: ServiceStream<OwnedHelloRequestView>,
    ) -> ServiceResult<impl connectrpc::Encodable<HelloSummary> + Send + use<'a>> {
        let mut names = Vec::new();

        while let Some(request) = requests.next().await {
            let request = request?;
            names.push(full_name(&request));
        }

        Response::ok(HelloSummary {
            names,
            ..Default::default()
        })
    }

    async fn chat(
        &self,
        _ctx: RequestContext,
        requests: ServiceStream<OwnedHelloRequestView>,
    ) -> ServiceResult<ServiceStream<HelloReply>> {
        let stream = requests.map(|request| request.map(|request| reply(&request, "Hello")));
        Response::stream_ok(stream)
    }

    async fn unary(
        &self,
        _ctx: RequestContext,
        request: OwnedHelloRequestView,
    ) -> ServiceResult<impl connectrpc::Encodable<HelloReply> + Send> {
        Response::ok(reply(&request, "Hello"))
    }
}

pub fn app() -> Router {
    let greeter = Arc::new(Greeter);
    let rest = rest::greeter_service_rest::make_router(greeter.clone());
    let ws = ws::greeter_service_ws::make_router(greeter.clone());
    let connect = greeter.register(connectrpc::Router::new());

    Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest("/v1", rest.merge(ws))
        .fallback_service(connect.into_axum_service())
}

fn reply(request: &OwnedHelloRequestView, prefix: &str) -> HelloReply {
    HelloReply {
        message: format!("{prefix}, {} {}!", request.first_name, request.last_name),
        ..Default::default()
    }
}

fn full_name(request: &OwnedHelloRequestView) -> String {
    format!("{} {}", request.first_name, request.last_name)
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use futures_util::{SinkExt as _, StreamExt as _};
    use http::header::CONTENT_TYPE;
    use http::{Method, Request, StatusCode};
    use tokio_tungstenite::tungstenite::Message;
    use tower::ServiceExt as _;

    use super::app;

    #[tokio::test]
    async fn generated_websocket_routes_do_not_include_unary_methods() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/v1/hello/unary/ws")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn rest_server_streaming_endpoint_still_returns_ndjson() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/hello/expand")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"firstName":"Jane","lastName":"Doe"}"#))
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body bytes");
        let lines = ndjson(&bytes);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["message"], "Hello, Jane Doe!");
        assert_eq!(lines[1]["message"], "Welcome aboard, Jane Doe!");
    }

    #[tokio::test]
    async fn websocket_server_streaming_endpoint_returns_json_frames() {
        let (mut ws, _server) = connect("/v1/hello/expand/ws").await;
        ws.send(Message::Text(
            r#"{"firstName":"Jane","lastName":"Doe"}"#.into(),
        ))
        .await
        .expect("send request");

        let messages = collect_text_messages(ws).await;

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["message"], "Hello, Jane Doe!");
        assert_eq!(messages[1]["message"], "Welcome aboard, Jane Doe!");
    }

    #[tokio::test]
    async fn websocket_bidi_streaming_endpoint_maps_json_frames() {
        let (mut ws, _server) = connect("/v1/hello/chat/ws").await;
        ws.send(Message::Text(
            r#"{"firstName":"Jane","lastName":"Doe"}"#.into(),
        ))
        .await
        .expect("send first request");
        ws.send(Message::Text(
            r#"{"firstName":"Ada","lastName":"Lovelace"}"#.into(),
        ))
        .await
        .expect("send second request");

        let first = next_text_message(&mut ws).await;
        let second = next_text_message(&mut ws).await;
        ws.close(None).await.expect("close client websocket");

        assert_eq!(first["message"], "Hello, Jane Doe!");
        assert_eq!(second["message"], "Hello, Ada Lovelace!");
    }

    #[tokio::test]
    async fn websocket_client_streaming_endpoint_returns_json_frame() {
        let (mut ws, _server) = connect("/v1/hello/collect/ws").await;
        ws.send(Message::Text(
            r#"{"firstName":"Jane","lastName":"Doe"}"#.into(),
        ))
        .await
        .expect("send first request");
        ws.send(Message::Text(
            r#"{"firstName":"Ada","lastName":"Lovelace"}"#.into(),
        ))
        .await
        .expect("send second request");
        ws.send(Message::Text("".into()))
            .await
            .expect("send stream end marker");

        let messages = collect_text_messages(ws).await;

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["names"][0], "Jane Doe");
        assert_eq!(messages[0]["names"][1], "Ada Lovelace");
    }

    #[tokio::test]
    async fn websocket_binary_request_closes_with_error() {
        let (mut ws, _server) = connect("/v1/hello/expand/ws").await;
        ws.send(Message::Binary(vec![0, 1, 2].into()))
            .await
            .expect("send binary request");

        let close = next_close_frame(&mut ws).await;

        assert_ne!(
            close.code,
            tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal
        );
    }

    #[tokio::test]
    async fn websocket_malformed_json_closes_with_error() {
        let (mut ws, _server) = connect("/v1/hello/expand/ws").await;
        ws.send(Message::Text("{".into()))
            .await
            .expect("send malformed request");

        let close = next_close_frame(&mut ws).await;

        assert_ne!(
            close.code,
            tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal
        );
    }

    async fn next_close_frame(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> tokio_tungstenite::tungstenite::protocol::CloseFrame {
        let close = loop {
            if let Message::Close(close) = ws
                .next()
                .await
                .expect("close frame")
                .expect("websocket frame")
            {
                break close;
            }
        };

        close.expect("close has code")
    }

    async fn connect(
        path: &str,
    ) -> (
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let addr = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app()).await.expect("serve test app");
        });
        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}{path}"))
            .await
            .expect("connect websocket");

        (ws, server)
    }

    async fn collect_text_messages(
        mut ws: tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> Vec<serde_json::Value> {
        let mut messages = Vec::new();
        while let Some(message) = ws.next().await {
            match message.expect("websocket frame") {
                Message::Text(text) => {
                    messages.push(serde_json::from_str(text.as_str()).expect("json frame"));
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        messages
    }

    async fn next_text_message(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> serde_json::Value {
        loop {
            match ws.next().await.expect("websocket frame").expect("message") {
                Message::Text(text) => {
                    return serde_json::from_str(text.as_str()).expect("json frame");
                }
                Message::Close(close) => panic!("unexpected close frame: {close:?}"),
                _ => {}
            }
        }
    }

    fn ndjson(bytes: &[u8]) -> Vec<serde_json::Value> {
        std::str::from_utf8(bytes)
            .expect("utf8 body")
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("json line"))
            .collect()
    }
}
