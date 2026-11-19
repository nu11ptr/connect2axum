//! Raw JSON-RPC tests of the MCP router over HTTP, without an MCP client.

use std::sync::Arc;

use axum::body::{Body, to_bytes};
use buffa::encoding::Tag;
use buffa::{DecodeContext, DecodeError, DefaultInstance, EncodeSink, Message, SizeCache};
use connect2axum_mcp::{
    McpConfig, Tool, ToolResult, Tools, error_result, make_router, tool_result,
};
use connectrpc::{ConnectError, RequestContext, Response};
use http::{Method, Request, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tower::ServiceExt as _;

const CURRENT: &str = "2026-07-28";
const LEGACY: &str = "2025-06-18";

const GREET_SCHEMA: &str = r#"{
  "type": "object",
  "properties": { "name": { "type": "string" } }
}"#;

#[derive(Default)]
struct Greeter;

#[derive(Deserialize)]
#[serde(default)]
#[derive(Default)]
struct GreetArgs {
    name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
struct Greeting {
    message: String,
}

async fn greet(_service: Arc<Greeter>, ctx: RequestContext, args: GreetArgs) -> ToolResult {
    if args.name.is_empty() {
        return error_result(ConnectError::invalid_argument("name is required"));
    }
    let via = ctx
        .header("x-via")
        .map(|value| value.to_str().unwrap().to_owned());
    let user = ctx.extensions().get::<User>().map(|user| user.0);
    let mut message = format!("Hello, {}!", args.name);
    if let Some(via) = via {
        message.push_str(&format!(" via {via}"));
    }
    if let Some(user) = user {
        message.push_str(&format!(" as {user}"));
    }
    tool_result::<Greeting, _>(Ok(Response::new(Greeting { message })))
}

#[derive(Clone, Copy)]
struct User(&'static str);

fn tools() -> Tools {
    Tools::builder(Arc::new(Greeter))
        .tool(
            Tool::new("Greeter_Greet", GREET_SCHEMA).description("Greets someone."),
            greet,
        )
        .build()
}

fn app() -> axum::Router {
    make_router(McpConfig::new("test", "1.0.0"), [tools()]).unwrap()
}

fn rpc(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn current_meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": CURRENT,
        "io.modelcontextprotocol/clientInfo": { "name": "test-client", "version": "1.0.0" },
        "io.modelcontextprotocol/clientCapabilities": {}
    })
}

fn post(body: &Value) -> http::request::Builder {
    Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("host", "localhost")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header(
            "mcp-method",
            body["method"].as_str().unwrap_or_default().to_owned(),
        )
}

fn current_request(body: &Value) -> Request<Body> {
    let mut builder = post(body).header("mcp-protocol-version", CURRENT);
    if let Some(name) = body["params"]["name"].as_str() {
        builder = builder.header("mcp-name", name);
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

async fn send(app: axum::Router, request: Request<Body>) -> (StatusCode, http::HeaderMap, Value) {
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, headers, body)
}

async fn call_current(app: axum::Router, name: &str, arguments: Option<Value>) -> Value {
    let mut params = json!({ "name": name, "_meta": current_meta() });
    if let Some(arguments) = arguments {
        params["arguments"] = arguments;
    }
    let (status, _, body) = send(app, current_request(&rpc(2, "tools/call", params))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

#[tokio::test]
async fn current_protocol_lists_and_calls_tools() {
    let list = rpc(1, "tools/list", json!({ "_meta": current_meta() }));
    let (status, _, body) = send(app(), current_request(&list)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tools = body["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "Greeter_Greet");
    assert_eq!(tools[0]["description"], "Greets someone.");
    assert_eq!(
        tools[0]["inputSchema"]["properties"]["name"]["type"],
        "string"
    );

    let body = call_current(app(), "Greeter_Greet", Some(json!({ "name": "Ada" }))).await;
    let result = &body["result"];
    assert_eq!(result["isError"], false, "{body}");
    assert_eq!(result["structuredContent"]["message"], "Hello, Ada!");
}

#[tokio::test]
async fn legacy_protocol_is_served_without_sessions() {
    let initialize = rpc(
        1,
        "initialize",
        json!({
            "protocolVersion": LEGACY,
            "capabilities": {},
            "clientInfo": { "name": "legacy-client", "version": "1.0.0" }
        }),
    );
    let (status, headers, body) = send(
        app(),
        post(&initialize)
            .body(Body::from(initialize.to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["protocolVersion"], LEGACY);
    assert_eq!(body["result"]["serverInfo"]["name"], "test");
    assert!(body["result"]["capabilities"]["tools"].is_object());
    assert!(headers.get("mcp-session-id").is_none());

    let initialized = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    let (status, _, _) = send(
        app(),
        post(&initialized)
            .header("mcp-protocol-version", LEGACY)
            .body(Body::from(initialized.to_string()))
            .unwrap(),
    )
    .await;
    assert!(status.is_success(), "{status}");

    let list = rpc(2, "tools/list", json!({}));
    let (status, _, body) = send(
        app(),
        post(&list)
            .header("mcp-protocol-version", LEGACY)
            .body(Body::from(list.to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["tools"][0]["name"], "Greeter_Greet");

    let call = rpc(
        3,
        "tools/call",
        json!({ "name": "Greeter_Greet", "arguments": { "name": "Grace" } }),
    );
    let (status, _, body) = send(
        app(),
        post(&call)
            .header("mcp-protocol-version", LEGACY)
            .body(Body::from(call.to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["result"]["structuredContent"]["message"],
        "Hello, Grace!"
    );
}

#[tokio::test]
async fn service_errors_become_error_results() {
    let body = call_current(app(), "Greeter_Greet", Some(json!({ "name": "" }))).await;
    let result = &body["result"];
    assert_eq!(result["isError"], true, "{body}");
    assert_eq!(result["structuredContent"]["code"], "invalid_argument");
    assert_eq!(result["structuredContent"]["message"], "name is required");
}

#[tokio::test]
async fn undecodable_arguments_become_invalid_argument_results() {
    let body = call_current(app(), "Greeter_Greet", Some(json!({ "name": 5 }))).await;
    let result = &body["result"];
    assert_eq!(result["isError"], true, "{body}");
    assert_eq!(result["structuredContent"]["code"], "invalid_argument");
    assert!(
        result["structuredContent"]["message"]
            .as_str()
            .unwrap()
            .starts_with("failed to decode tool arguments")
    );
}

#[tokio::test]
async fn absent_arguments_are_an_empty_object() {
    let body = call_current(app(), "Greeter_Greet", None).await;
    // `{}` decodes to an empty name, which the handler rejects.
    assert_eq!(body["result"]["isError"], true, "{body}");
    assert_eq!(
        body["result"]["structuredContent"]["message"],
        "name is required"
    );
}

#[tokio::test]
async fn unknown_tools_are_protocol_errors() {
    let body = call_current(app(), "Nope_Nothing", Some(json!({}))).await;
    assert_eq!(body["error"]["code"], -32602, "{body}");
    assert_eq!(body["error"]["message"], "tool not found");
}

#[tokio::test]
async fn headers_and_extensions_reach_the_connect_context() {
    let app = app().layer(axum::middleware::from_fn(
        |mut request: axum::extract::Request, next: axum::middleware::Next| async move {
            request.extensions_mut().insert(User("ada"));
            next.run(request).await
        },
    ));
    let call = rpc(
        2,
        "tools/call",
        json!({ "name": "Greeter_Greet", "arguments": { "name": "Ada" }, "_meta": current_meta() }),
    );
    let mut request = current_request(&call);
    request
        .headers_mut()
        .insert("x-via", "gateway".parse().unwrap());

    let (_, _, body) = send(app, request).await;

    assert_eq!(
        body["result"]["structuredContent"]["message"], "Hello, Ada! via gateway as ada",
        "{body}"
    );
}

#[tokio::test]
async fn rejects_disallowed_hosts_and_get() {
    let list = rpc(1, "tools/list", json!({ "_meta": current_meta() }));
    let mut request = current_request(&list);
    request
        .headers_mut()
        .insert("host", "evil.example".parse().unwrap());
    let (status, _, _) = send(app(), request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let allowed = make_router(
        McpConfig::new("test", "1.0.0").allowed_hosts(["api.example.com"]),
        [tools()],
    )
    .unwrap();
    let mut request = current_request(&list);
    request
        .headers_mut()
        .insert("host", "api.example.com".parse().unwrap());
    let (status, _, body) = send(allowed, request).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, _, _) = send(
        app(),
        Request::builder()
            .method(Method::GET)
            .uri("/mcp")
            .header("host", "localhost")
            .header("accept", "text/event-stream")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn custom_path_is_served() {
    let app = make_router(
        McpConfig::new("test", "1.0.0").path("/agents/mcp"),
        [tools()],
    )
    .unwrap();
    let list = rpc(1, "tools/list", json!({ "_meta": current_meta() }));
    let mut request = current_request(&list);
    *request.uri_mut() = "/agents/mcp".parse().unwrap();

    let (status, _, body) = send(app, request).await;

    assert_eq!(status, StatusCode::OK, "{body}");
}

#[test]
fn rejects_duplicate_names_bad_names_and_bad_schemas() {
    let err = make_router(McpConfig::new("test", "1.0.0"), [tools(), tools()]).unwrap_err();
    assert!(err.to_string().contains("Greeter_Greet"), "{err}");
    assert!(err.to_string().contains("more than once"), "{err}");

    let bad_name = Tools::builder(Arc::new(Greeter))
        .tool(Tool::new("has space", GREET_SCHEMA), greet)
        .build();
    let err = make_router(McpConfig::new("test", "1.0.0"), [bad_name]).unwrap_err();
    assert!(err.to_string().contains("has space"), "{err}");

    let bad_schema = Tools::builder(Arc::new(Greeter))
        .tool(Tool::new("Greeter_Greet", r#"{"type":"string"}"#), greet)
        .build();
    let err = make_router(McpConfig::new("test", "1.0.0"), [bad_schema]).unwrap_err();
    assert!(err.to_string().contains("\"type\": \"object\""), "{err}");

    let not_json = Tools::builder(Arc::new(Greeter))
        .tool(Tool::new("Greeter_Greet", "{"), greet)
        .build();
    assert!(make_router(McpConfig::new("test", "1.0.0"), [not_json]).is_err());

    let err = make_router(McpConfig::new("test", "1.0.0").path("mcp"), [tools()]).unwrap_err();
    assert!(err.to_string().contains("must start with '/'"), "{err}");
}

// A JSON-only stand-in for a Buffa generated message.

impl DefaultInstance for Greeting {
    fn default_instance() -> &'static Self {
        static VALUE: std::sync::OnceLock<Greeting> = std::sync::OnceLock::new();
        VALUE.get_or_init(Greeting::default)
    }
}

impl Message for Greeting {
    fn compute_size(&self, _cache: &mut SizeCache) -> u32 {
        0
    }

    fn write_to(&self, _cache: &mut SizeCache, _buf: &mut impl EncodeSink) {}

    fn merge_field(
        &mut self,
        _tag: Tag,
        _buf: &mut impl buffa::bytes::Buf,
        _ctx: DecodeContext<'_>,
    ) -> Result<(), DecodeError> {
        Err(DecodeError::UnexpectedEof)
    }

    fn clear(&mut self) {}
}
