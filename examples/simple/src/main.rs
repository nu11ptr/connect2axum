use simple::app;

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8000")
        .await
        .expect("bind example listener");
    println!(
        "Connect/REST server listening on {}",
        listener.local_addr().expect("listener address")
    );
    axum::serve(listener, app()).await.expect("serve example");
}
