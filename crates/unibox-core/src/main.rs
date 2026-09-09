use axum::{routing::get, Json, Router};
use serde::Serialize;
use std::net::SocketAddr;

#[derive(Serialize)]
struct Health {
    status: &'static str,
    service: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        service: "uniboxd",
    })
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/health", get(health));
    let addr = SocketAddr::from(([127, 0, 0, 1], 17640));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind localhost");
    axum::serve(listener, app).await.expect("serve uniboxd");
}
