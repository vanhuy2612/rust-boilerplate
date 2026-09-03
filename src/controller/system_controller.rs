use axum::{
    Json,
    body::Body,
    extract::State,
    http::{HeaderValue, Response, StatusCode, header},
};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::service::{app_state::AppState, cpu_service};

pub async fn hello() -> Json<Value> {
    Json(json!({ "message": "Hello Axum" }))
}

pub async fn cpu_bound(State(state): State<Arc<AppState>>) -> Response<Body> {
    match cpu_service::render_resized_jpeg(&state.cpu_worker_pool).await {
        Ok(bytes) => jpeg_response(StatusCode::OK, bytes),
        Err(error) => {
            tracing::error!(%error, "failed to render cpu-bound response");
            jpeg_response(StatusCode::INTERNAL_SERVER_ERROR, Vec::new())
        }
    }
}

fn jpeg_response(status: StatusCode, bytes: Vec<u8>) -> Response<Body> {
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    response
}
