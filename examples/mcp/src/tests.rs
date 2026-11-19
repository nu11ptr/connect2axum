use std::sync::Arc;

use axum::body::{Body, to_bytes};
use http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt as _;

use super::{Library, app_with};

const PROTOCOL: &str = "2026-07-28";

fn rpc(method: &str, params: Value) -> Request<Body> {
    let mut params = params;
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": PROTOCOL,
        "io.modelcontextprotocol/clientInfo": { "name": "example-test", "version": "1.0.0" },
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("host", "localhost")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", PROTOCOL)
        .header("mcp-method", method);
    if let Some(name) = params["name"].as_str() {
        builder = builder.header("mcp-name", name);
    }
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    builder.body(Body::from(body.to_string())).unwrap()
}

async fn send(app: axum::Router, request: Request<Body>) -> Value {
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn call(app: axum::Router, name: &str, arguments: Value) -> Value {
    let body = send(
        app,
        rpc(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        ),
    )
    .await;
    body["result"].clone()
}

#[tokio::test]
async fn lists_unary_tools_except_excluded_and_streaming() {
    let body = send(app_with(Arc::default()), rpc("tools/list", json!({}))).await;
    let tools = body["result"]["tools"].as_array().unwrap();
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec!["LibraryService_AddBook", "LibraryService_SearchBooks"]
    );

    let add = &tools[0];
    assert_eq!(
        add["description"],
        "Adds a book to the catalog and returns it with its assigned ID."
    );
    let schema = &add["inputSchema"];
    assert_eq!(schema["required"], json!(["title", "author"]));
    assert_eq!(
        schema["properties"]["genre"]["enum"],
        json!([
            "GENRE_UNSPECIFIED",
            "GENRE_FICTION",
            "GENRE_HISTORY",
            "GENRE_SCIENCE"
        ])
    );
    assert_eq!(
        schema["properties"]["tags"]["additionalProperties"],
        json!({ "type": "string" })
    );
    assert_eq!(
        schema["properties"]["publisher"]["properties"]["name"]["type"],
        "string"
    );
    assert!(
        tools[1]["inputSchema"]["properties"]["author"]["description"]
            .as_str()
            .unwrap()
            .contains("At most one of: `author`, `genre`.")
    );
}

#[tokio::test]
async fn tools_call_the_service_including_rpcs_without_rest_bindings() {
    let library = Arc::new(Library::default());

    let added = call(
        app_with(library.clone()),
        "LibraryService_AddBook",
        json!({
            "title": "The Left Hand of Darkness",
            "author": "Ursula K. Le Guin",
            "genre": "GENRE_FICTION",
            "tags": { "shelf": "B3" },
            "publisher": { "name": "Ace", "country": "US" }
        }),
    )
    .await;
    assert_eq!(added["isError"], false, "{added}");
    let book = &added["structuredContent"];
    assert_eq!(book["id"], "book-1");
    assert_eq!(book["genre"], "GENRE_FICTION");
    assert_eq!(book["tags"]["shelf"], "B3");
    assert_eq!(book["publisher"]["name"], "Ace");

    let found = call(
        app_with(library),
        "LibraryService_SearchBooks",
        json!({ "author": "Ursula K. Le Guin" }),
    )
    .await;
    assert_eq!(found["isError"], false, "{found}");
    assert_eq!(
        found["structuredContent"]["books"][0]["title"],
        "The Left Hand of Darkness"
    );
}

#[tokio::test]
async fn service_and_decode_errors_are_tool_errors() {
    let missing = call(
        app_with(Arc::default()),
        "LibraryService_AddBook",
        json!({ "title": "Untitled" }),
    )
    .await;
    assert_eq!(missing["isError"], true);
    assert_eq!(missing["structuredContent"]["code"], "invalid_argument");

    let bad_enum = call(
        app_with(Arc::default()),
        "LibraryService_AddBook",
        json!({ "title": "T", "author": "A", "genre": "GENRE_POETRY" }),
    )
    .await;
    assert_eq!(bad_enum["isError"], true, "{bad_enum}");
    assert_eq!(bad_enum["structuredContent"]["code"], "invalid_argument");
}

#[tokio::test]
async fn excluded_rpc_is_not_callable_over_mcp_but_stays_on_connect() {
    let body = send(
        app_with(Arc::default()),
        rpc(
            "tools/call",
            json!({ "name": "LibraryService_DeleteBook", "arguments": { "id": "x" } }),
        ),
    )
    .await;
    assert_eq!(body["error"]["message"], "tool not found");

    let response = app_with(Arc::default())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/library.v1.LibraryService/DeleteBook")
                .header("content-type", "application/json")
                .header("connect-protocol-version", "1")
                .body(Body::from(r#"{"id":"missing"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rest_and_mcp_share_one_service() {
    let library = Arc::new(Library::default());
    let response = app_with(library.clone())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/books")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"Dune","author":"Frank Herbert"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let found = call(app_with(library), "LibraryService_SearchBooks", json!({})).await;
    assert_eq!(found["structuredContent"]["books"][0]["title"], "Dune");
}
