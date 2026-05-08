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
#[path = "generated/rest/streaming/v1/streaming.connect2axum.rs"]
pub mod rest;

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
}

pub fn app() -> Router {
    let greeter = Arc::new(Greeter);
    let rest = rest::greeter_service_rest::make_router(greeter.clone());
    let connect = greeter.register(connectrpc::Router::new());

    Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest("/v1", rest)
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
    use http::header::CONTENT_TYPE;
    use http::{Method, Request, StatusCode};
    use tower::ServiceExt as _;

    use super::app;

    #[tokio::test]
    async fn rest_server_streaming_endpoint_returns_ndjson() {
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
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/x-ndjson"
        );
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body bytes");
        let lines = ndjson(&bytes);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["message"], "Hello, Jane Doe!");
        assert_eq!(lines[1]["message"], "Welcome aboard, Jane Doe!");
    }

    #[tokio::test]
    async fn rest_client_streaming_endpoint_reads_ndjson() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/hello/collect")
                    .header(CONTENT_TYPE, "application/x-ndjson")
                    .body(Body::from(
                        "{\"firstName\":\"Jane\",\"lastName\":\"Doe\"}\n\
                         {\"firstName\":\"Ada\",\"lastName\":\"Lovelace\"}\n",
                    ))
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body bytes");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json response");

        assert_eq!(json["names"][0], "Jane Doe");
        assert_eq!(json["names"][1], "Ada Lovelace");
    }

    #[tokio::test]
    async fn rest_bidi_streaming_endpoint_maps_each_ndjson_line() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/hello/chat")
                    .header(CONTENT_TYPE, "application/x-ndjson")
                    .body(Body::from(
                        "{\"firstName\":\"Jane\",\"lastName\":\"Doe\"}\n\
                         {\"firstName\":\"Ada\",\"lastName\":\"Lovelace\"}\n",
                    ))
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
        assert_eq!(lines[1]["message"], "Hello, Ada Lovelace!");
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
