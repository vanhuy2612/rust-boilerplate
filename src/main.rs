mod controller;
mod route;
mod service;

use std::env;
use tracing::Level;

#[tokio::main]
async fn main() {
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
    axum::serve(listener, app).await.unwrap();
}
