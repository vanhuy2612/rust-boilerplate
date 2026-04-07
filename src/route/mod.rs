use axum::{Router, routing::get};
use std::sync::Arc;

use crate::controller::system_controller;
use crate::service::app_state::AppState;

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(system_controller::hello))
        .route("/api/v1/cpu-bound", get(system_controller::cpu_bound))
        .route("/api/v1/io-read-db", get(system_controller::io_read_db))
        .route("/api/v1/io-write-db", get(system_controller::io_write_db))
        .route(
            "/api/v1/io-publish-2-queue",
            get(system_controller::io_publish_to_queue),
        )
        .route(
            "/api/v1/io-call-external-api",
            get(system_controller::io_call_external_api),
        )
        .with_state(state)
}
