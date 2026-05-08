use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use connectrpc::{RequestContext, Response, ServiceResult};

#[rustfmt::skip]
#[path = "generated/connect/mod.rs"]
pub mod connect;

#[rustfmt::skip]
#[path = "generated/buffa/mod.rs"]
pub mod proto;

#[rustfmt::skip]
#[path = "generated/connect2axum/hello/v1/hello.connect2rest.rs"]
pub mod rest;

use connect::hello::v1::GreeterServiceExt as _;
use connect::hello::v1::OwnedHelloRequestView;
use proto::hello::v1::HelloReply;

#[derive(Clone, Debug, Default)]
pub struct Greeter;

impl connect::hello::v1::GreeterService for Greeter {
    async fn say_hello<'a>(
        &'a self,
        _ctx: RequestContext,
        request: OwnedHelloRequestView,
    ) -> ServiceResult<impl connectrpc::Encodable<HelloReply> + Send + use<'a>> {
        Response::ok(HelloReply {
            message: format!(
                "Hello, {} {} {}!",
                request.salutation, request.first_name, request.last_name
            ),
            ..Default::default()
        })
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

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use http::header::CONTENT_TYPE;
    use http::{Method, Request, StatusCode};
    use tower::ServiceExt as _;

    use super::app;

    #[tokio::test]
    async fn rest_endpoint_says_hello() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/hello/Jane?salutation=Ahoy")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"lastName":"Doe"}"#))
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body bytes");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json response");
        assert_eq!(json["message"], "Hello, Ahoy Jane Doe!");
    }

    #[tokio::test]
    async fn connect_protocol_endpoint_says_hello() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/hello.v1.GreeterService/SayHello")
                    .header(CONTENT_TYPE, "application/json")
                    .header("connect-protocol-version", "1")
                    .body(Body::from(
                        r#"{"salutation":"Ahoy","firstName":"Jane","lastName":"Doe"}"#,
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
        assert_eq!(json["message"], "Hello, Ahoy Jane Doe!");
    }
}
