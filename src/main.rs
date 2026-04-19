use axum::{
    routing::get,
    Router,
};

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(handler_root));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    println!("🚀 Server running in http://{}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();
}

async fn handler_root() -> &'static str {
    "Welcome to Potagia!"
}
