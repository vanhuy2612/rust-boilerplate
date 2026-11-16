use axum::{
    BoxError, Router,
    error_handling::HandleErrorLayer,
    http::StatusCode,
    routing::{get, post},
};
use std::{env, sync::Arc, thread, time::Duration};
use tower::{
    ServiceBuilder, limit::GlobalConcurrencyLimitLayer, load_shed::error::Overloaded,
    timeout::error::Elapsed,
};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::controller::{debug_controller, system_controller};
use crate::service::app_state::AppState;

const DEFAULT_CPU_TIMEOUT_MS: u64 = 10_000;

pub fn create_router(state: Arc<AppState>) -> Router {
    let max_inflight = cpu_max_inflight();
    let timeout = cpu_timeout();
    tracing::info!(
        max_inflight,
        timeout_ms = timeout.as_millis() as u64,
        "cpu-bound limits"
    );

    // Bound the number of cpu-bound requests in flight. Without this, requests that
    // arrive faster than they can be processed queue up forever (each one holding a
    // connection + task), so memory grows for as long as the overload lasts.
    let cpu_bound_limits = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(handle_cpu_bound_error))
        .load_shed()
        .layer(GlobalConcurrencyLimitLayer::new(max_inflight))
        .timeout(timeout);

    let mut router = Router::new()
        .route("/", get(system_controller::hello))
        .route(
            "/api/v1/cpu-bound",
            get(system_controller::cpu_bound).layer(cpu_bound_limits),
        );

    // Memory inspection endpoints expose internals: opt-in only, never in public deployments.
    if debug_endpoints_enabled() {
        tracing::warn!("DEBUG_ENDPOINTS enabled: /debug/* memory inspection routes are exposed");
        router = router
            .route("/debug/memory", get(debug_controller::memory_stats))
            .route(
                "/debug/dhat/snapshot",
                post(debug_controller::dhat_snapshot),
            )
            .route("/debug/pprof/heap", get(debug_controller::heap_pprof))
            .route(
                "/debug/pprof/heap/flamegraph",
                get(debug_controller::heap_flamegraph),
            );
    }

    router
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(state)
}

async fn handle_cpu_bound_error(error: BoxError) -> (StatusCode, &'static str) {
    if error.is::<Overloaded>() {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "server is overloaded, retry later",
        )
    } else if error.is::<Elapsed>() {
        (StatusCode::REQUEST_TIMEOUT, "request timed out")
    } else {
        tracing::error!(%error, "unhandled cpu-bound middleware error");
        (StatusCode::INTERNAL_SERVER_ERROR, "internal error")
    }
}

/// `DEBUG_ENDPOINTS=true|1` registers the /debug/* memory inspection routes.
fn debug_endpoints_enabled() -> bool {
    matches!(
        env::var("DEBUG_ENDPOINTS").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("True")
    )
}

/// Max concurrent cpu-bound requests (running + waiting for a CPU worker).
/// `CPU_MAX_INFLIGHT` env var, default = available cores x 4.
fn cpu_max_inflight() -> usize {
    env::var("CPU_MAX_INFLIGHT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| {
            thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
                * 4
        })
}

/// Per-request timeout for cpu-bound requests. `CPU_TIMEOUT_MS` env var, default 10s.
fn cpu_timeout() -> Duration {
    let millis = env::var("CPU_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_CPU_TIMEOUT_MS);
    Duration::from_millis(millis)
}
