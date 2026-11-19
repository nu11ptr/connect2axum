use mcp::app;

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8003")
        .await
        .expect("bind example listener");
    println!(
        "Connect/REST/MCP server listening on {}; MCP endpoint at /mcp",
        listener.local_addr().expect("listener address")
    );
    axum::serve(listener, app()).await.expect("serve example");
}
