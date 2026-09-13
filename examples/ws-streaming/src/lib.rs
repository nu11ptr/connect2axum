#![allow(refining_impl_trait)]

use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use connectrpc::{
    CodecFormat, ConnectError, Encodable, EncodedBody, RequestContext, Response, ServiceRequest,
    ServiceResult, ServiceStream, StreamMessage,
};
use flexstr::{IntoOptimizedFlexStr as _, SharedStr};
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

use connect::streaming::v1::GreeterServiceExt as _;
use proto::streaming::v1::__buffa::view::{HelloReplyView, HelloRequestView};
use proto::streaming::v1::{HelloReply, HelloRequest, HelloSummary};

/// A domain reply owns its data while streams and channels retain it. The
/// generated protobuf view borrows that data only for the duration of encoding.
#[derive(Clone, Debug)]
pub struct Greeting {
    message: SharedStr,
}

impl Greeting {
    fn view(&self) -> HelloReplyView<'_> {
        HelloReplyView {
            message: self.message.as_ref(),
            ..Default::default()
        }
    }
}

impl Encodable<HelloReply> for Greeting {
    fn encode(&self, codec: CodecFormat) -> Result<buffa::bytes::Bytes, ConnectError> {
        connect2axum::json_view(self.view()).encode(codec)
    }

    fn encode_segments(&self, codec: CodecFormat) -> Result<EncodedBody, ConnectError> {
        connect2axum::json_view(self.view()).encode_segments(codec)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Greeter;

impl connect::streaming::v1::GreeterService for Greeter {
    async fn expand(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, HelloRequest>,
    ) -> ServiceResult<ServiceStream<Greeting>> {
        let stream = futures_util::stream::iter([
            Ok(reply(&request, "Hello")),
            Ok(reply(&request, "Welcome aboard")),
        ]);
        Response::stream_ok(stream)
    }

    async fn collect<'a>(
        &'a self,
        _ctx: RequestContext,
        mut requests: ServiceStream<StreamMessage<HelloRequest>>,
    ) -> ServiceResult<impl connectrpc::Encodable<HelloSummary> + Send + use<'a>> {
        let mut names = Vec::new();

        while let Some(request) = requests.next().await {
            let request = request?;
            names.push(full_name(request.view()));
        }

        Response::ok(HelloSummary {
            names,
            ..Default::default()
        })
    }

    async fn chat(
        &self,
        _ctx: RequestContext,
        requests: ServiceStream<StreamMessage<HelloRequest>>,
    ) -> ServiceResult<ServiceStream<Greeting>> {
        let stream = requests.map(|request| request.map(|request| reply(request.view(), "Hello")));
        Response::stream_ok(stream)
    }

    async fn unary<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, HelloRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<HelloReply> + Send + use<'a>> {
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

fn reply(request: &HelloRequestView<'_>, prefix: &str) -> Greeting {
    Greeting {
        message: format!("{prefix}, {} {}!", request.first_name, request.last_name).into_opt(),
    }
}

fn full_name(request: &HelloRequestView<'_>) -> String {
    format!("{} {}", request.first_name, request.last_name)
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use buffa::Message as _;
    use buffa::bytes::{Bytes, BytesMut};
    use connectrpc::envelope::{Envelope, flags};
    use connectrpc::{CodecFormat, Encodable as _};
    use flexstr::ToOwnedFlexStr as _;
    use futures_util::{SinkExt as _, StreamExt as _};
    use http::header::CONTENT_TYPE;
    use http::{Method, Request, StatusCode};
    use tokio_tungstenite::tungstenite::Message;
    use tower::ServiceExt as _;

    use super::{Greeting, HelloReply, HelloRequest, app};

    #[tokio::test]
    async fn domain_replies_cross_a_channel_and_preserve_protojson() {
        let messages = ["", "quoted \"text\", slash \\, newline\n雪"];
        let (send, mut recv) = tokio::sync::mpsc::channel(1);
        let producer = tokio::spawn(async move {
            for message in messages {
                send.send(Greeting {
                    message: message.to_owned_opt(),
                })
                .await
                .expect("send domain reply");
            }
        });

        for message in messages {
            let greeting = recv.recv().await.expect("receive domain reply");
            let json = greeting.encode(CodecFormat::Json).expect("encode JSON");
            let proto = greeting
                .encode(CodecFormat::Proto)
                .expect("encode protobuf");
            let decoded = HelloReply::decode_from_slice(&proto).expect("decode protobuf");

            assert_eq!(decoded.message, message);
            assert_eq!(json.as_ref(), serde_json::to_vec(&decoded).unwrap());
            if message.is_empty() {
                assert_eq!(json.as_ref(), b"{}");
            } else {
                assert_eq!(
                    serde_json::from_slice::<HelloReply>(&json).unwrap(),
                    decoded
                );
            }
        }
        assert!(recv.recv().await.is_none());
        producer.await.expect("producer completes");
    }

    #[tokio::test]
    async fn native_connect_server_streams_domain_replies_as_json_and_protobuf() {
        for (codec, content_type) in [
            (CodecFormat::Json, "application/connect+json"),
            (CodecFormat::Proto, "application/connect+proto"),
        ] {
            let request = HelloRequest {
                first_name: "Jane \"JJ\"".into(),
                last_name: "Doe".into(),
                ..Default::default()
            };
            let payload = match codec {
                CodecFormat::Json => Bytes::from(serde_json::to_vec(&request).unwrap()),
                CodecFormat::Proto => request.encode_to_bytes(),
                _ => unreachable!(),
            };
            let response = app()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/streaming.v1.GreeterService/Expand")
                        .header(CONTENT_TYPE, content_type)
                        .header("connect-protocol-version", "1")
                        .body(Body::from(Envelope::data(payload).encode()))
                        .expect("request builds"),
                )
                .await
                .expect("router responds");

            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[CONTENT_TYPE], content_type);
            let body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body bytes");
            let mut body = BytesMut::from(body.as_ref());
            for prefix in ["Hello", "Welcome aboard"] {
                let envelope = Envelope::decode(&mut body)
                    .expect("valid envelope")
                    .expect("reply envelope");
                assert_eq!(envelope.flags, flags::DATA);
                let reply: HelloReply = match codec {
                    CodecFormat::Json => serde_json::from_slice(&envelope.data).unwrap(),
                    CodecFormat::Proto => HelloReply::decode_from_slice(&envelope.data).unwrap(),
                    _ => unreachable!(),
                };
                assert_eq!(reply.message, format!("{prefix}, Jane \"JJ\" Doe!"));
            }
            let end = Envelope::decode(&mut body)
                .expect("valid end envelope")
                .expect("end of stream");
            assert_eq!(end.flags, flags::END_STREAM);
            let end: serde_json::Value = serde_json::from_slice(&end.data).unwrap();
            assert!(end.get("error").is_none(), "{end}");
            assert!(body.is_empty());
        }
    }

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

    #[test]
    fn generated_asyncapi_documents_websocket_routes() {
        let document: serde_json::Value =
            serde_json::from_str(include_str!("generated/asyncapi/asyncapi.json"))
                .expect("generated asyncapi is valid json");

        assert_eq!(document["asyncapi"], "3.1.0");
        assert!(document["channels"].get("/hello/expand/ws").is_some());
        assert!(document["channels"].get("/hello/collect/ws").is_some());
        assert!(document["channels"].get("/hello/chat/ws").is_some());
        assert!(document["channels"].get("/hello/unary/ws").is_none());
        assert!(
            document["operations"]
                .get("GreeterService_Chat_receive")
                .is_some()
        );
        assert_eq!(
            document["operations"]["GreeterService_Collect_receive"]["x-connect2axum-end-of-stream"]
                ["payload"],
            ""
        );
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
