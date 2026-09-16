use gloo_net::http::Request;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use wasm_bindgen_futures::spawn_local;

pub async fn get_json<T: DeserializeOwned>(url: &str) -> Result<T, String> {
    let response = Request::get(url).send().await.map_err(|e| e.to_string())?;
    if !(200..300).contains(&response.status()) {
        return Err(format!("GET {url}: HTTP {}", response.status()));
    }
    response.json().await.map_err(|e| e.to_string())
}

pub async fn post_json<T: Serialize, R: DeserializeOwned>(url: &str, value: &T) -> Result<R, String> {
    let request = Request::post(url).json(value).map_err(|e| e.to_string())?;
    let response = request.send().await.map_err(|e| e.to_string())?;
    if !(200..300).contains(&response.status()) {
        return Err(format!("POST {url}: HTTP {}", response.status()));
    }
    response.json().await.map_err(|e| e.to_string())
}

pub async fn post_bytes(url: &str, bytes: Vec<u8>) -> Result<(), String> {
    let request = Request::post(url).body(bytes).map_err(|e| e.to_string())?;
    let response = request.send().await.map_err(|e| e.to_string())?;
    if !(200..300).contains(&response.status()) {
        return Err(format!("POST {url}: HTTP {}", response.status()));
    }
    Ok(())
}

pub fn telemetry(event: &'static str, detail: Value) {
    spawn_local(async move {
        let payload = json!({"event": event, "detail": detail});
        let Ok(request) = Request::post("/singeros/api/client-log").json(&payload) else { return; };
        let _ = request.send().await;
    });
}
