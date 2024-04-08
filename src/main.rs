use axum::{routing::post, Router};
use dotenvy::dotenv;
use reqwest::ClientBuilder;
use std::{sync::Arc, time::Duration};
use structured_logger::{async_json::new_writer, get_env_level, Builder};
use tokio::signal;

mod handler;
mod redis;

#[tokio::main]
async fn main() {
    dotenv().expect(".env file not found");
    let addr = std::env::var("SERVER_ADDR").unwrap_or("127.0.0.1:8080".to_string());

    Builder::with_level(&get_env_level().to_string())
        .with_target_writer("*", new_writer(tokio::io::stdout()))
        .init();

    let http_client = ClientBuilder::new()
        .http2_keep_alive_interval(Some(Duration::from_secs(25)))
        .http2_keep_alive_timeout(Duration::from_secs(15))
        .http2_keep_alive_while_idle(true)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(10))
        .gzip(true)
        .build()
        .unwrap();

    let redis_client = redis::new(&std::env::var("REDIS_URL").expect("REDIS_URL not found"))
        .await
        .unwrap();

    let app = Router::new()
        .route("/*any", post(handler::proxy).get(handler::proxy))
        .with_state(handler::AppState {
            http_client: Arc::new(http_client),
            redis_client: Arc::new(redis_client),
        });
    let shutdown = shutdown_signal();

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    log::warn!(target: "server", "listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .unwrap();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    log::warn!(target: "server", "signal received, starting graceful shutdown");
}
