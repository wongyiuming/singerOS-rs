use axum::{routing::get, Json, Router};
use serde_json::{json, Value};
use std::{net::SocketAddr, path::PathBuf};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cert = PathBuf::from(std::env::var("SINGER_CERT").expect("SINGER_CERT"));
    let key = PathBuf::from(std::env::var("SINGER_KEY").expect("SINGER_KEY"));
    let addr: SocketAddr = std::env::var("SINGER_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:18443".into())
        .parse()
        .expect("valid SINGER_ADDR");

    let app = Router::new()
        .route("/singeros/healthz", get(health))
        .route("/singeros/api/capabilities", get(capabilities));

    let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key)
        .await
        .expect("load TLS material");

    axum_server::bind_rustls(addr, tls)
        .serve(app.into_make_service())
        .await
        .expect("serve SingerOS");
}

async fn health() -> Json<Value> {
    Json(json!({"ok": true, "app": "SingerOS-rs"}))
}

async fn capabilities() -> Json<Value> {
    Json(json!({"handwritten_js": 0, "runtime": "rust"}))
}
