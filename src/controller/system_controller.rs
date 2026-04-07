use axum::{
    Json,
    body::Body,
    extract::State,
    http::{HeaderValue, Response, StatusCode, header},
    response::IntoResponse,
};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::service::{app_state::AppState, cpu_service, io_service};

pub async fn hello() -> Json<Value> {
    Json(json!({ "message": "Hello Axum" }))
}

pub async fn cpu_bound() -> Response<Body> {
    match cpu_service::render_resized_jpeg().await {
        Ok(bytes) => jpeg_response(StatusCode::OK, bytes),
        Err(error) => {
            tracing::error!(%error, "failed to render cpu-bound response");
            jpeg_response(StatusCode::INTERNAL_SERVER_ERROR, Vec::new())
        }
    }
}

pub async fn io_read_db(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match io_service::read_db(&state).await {
        Ok(payload) => respond_json(payload),
        Err(error) => error_response(error),
    }
}

pub async fn io_write_db(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match io_service::write_db(&state).await {
        Ok(payload) => respond_json(payload),
        Err(error) => error_response(error),
    }
}

pub async fn io_publish_to_queue(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match io_service::publish_to_queue(&state).await {
        Ok(payload) => respond_json(payload),
        Err(error) => error_response(error),
    }
}

pub async fn io_call_external_api(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match io_service::call_external_api(&state).await {
        Ok(payload) => respond_json(payload),
        Err(error) => error_response(error),
    }
}

fn respond_json(payload: Value) -> (StatusCode, Json<Value>) {
    (StatusCode::OK, Json(payload))
}

fn error_response(error: io_service::IoServiceError) -> (StatusCode, Json<Value>) {
    let status = error.status_code();
    let message = error.to_string();

    (
        status,
        Json(json!({
            "error": message,
        })),
    )
}

fn jpeg_response(status: StatusCode, bytes: Vec<u8>) -> Response<Body> {
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    response
}
