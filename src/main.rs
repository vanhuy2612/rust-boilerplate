mod controller;
mod route;
mod service;

use std::env;
use tracing::Level;

// Heap profiler (feature `dhat-heap`) must own the global allocator while profiling.
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static GLOBAL: dhat::Alloc = dhat::Alloc;

#[cfg(all(not(target_env = "msvc"), not(feature = "dhat-heap")))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[tokio::main]
async fn main() {
    // Reports are written to ./dhat-heap/ on shutdown and on POST /debug/dhat/snapshot.
    #[cfg(feature = "dhat-heap")]
    service::heap_profiler::start();

    tracing_subscriber::fmt()
        .with_writer(std::io::stdout)
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    let app_state = service::app_state::AppState::from_env()
        .await
        .expect("failed to initialize application state");
    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let address = format!("0.0.0.0:{port}");
    let app = route::create_router(app_state);
    let listener = tokio::net::TcpListener::bind(&address).await.unwrap();

    tracing::info!(%address, "server started");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
    tracing::info!("server stopped");

    #[cfg(feature = "dhat-heap")]
    if let Some(file) = service::heap_profiler::stop() {
        tracing::info!(%file, "dhat report written");
    }
}

/// Resolves on Ctrl+C. With the `dhat-heap` feature, also resolves after
/// `DHAT_DURATION_SECS` seconds (if set) so the profile is written even when the
/// IDE stop button kills the process instead of sending Ctrl+C
/// (snapshots can also be taken any time with POST /debug/dhat/snapshot).
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(feature = "dhat-heap")]
    {
        let duration = env::var("DHAT_DURATION_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok());
        if let Some(secs) = duration {
            tracing::info!(
                secs,
                "dhat: server will stop automatically and write its report to ./dhat-heap/"
            );
            tokio::select! {
                _ = ctrl_c => {}
                _ = tokio::time::sleep(std::time::Duration::from_secs(secs)) => {}
            }
            return;
        }
    }

    ctrl_c.await;
}
