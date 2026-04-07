mod controller;
mod route;
mod service;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app_state = service::app_state::AppState::from_env()
        .await
        .expect("failed to initialize application state");
    let app = route::create_router(app_state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
