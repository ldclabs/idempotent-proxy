use axum::{routing, Router};
use axum_server::tls_rustls::RustlsConfig;
use base64::{engine::general_purpose, Engine};
use dotenvy::dotenv;
use http::HeaderValue;
use k256::ecdsa;
use reqwest::ClientBuilder;
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};
use structured_logger::{async_json::new_writer, get_env_level, Builder};
use tokio::signal;

mod handler;
mod redis;

const APP_NAME: &str = env!("CARGO_PKG_NAME");
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() {
    dotenv().expect(".env file not found");

    Builder::with_level(&get_env_level().to_string())
        .with_target_writer("*", new_writer(tokio::io::stdout()))
        .init();

    let req_timeout: u64 = std::env::var("REQUEST_TIMEOUT")
        .map(|n| n.parse().unwrap())
        .unwrap_or(10000u64)
        .max(1000u64);

    let http_client = ClientBuilder::new()
        .http2_keep_alive_interval(Some(Duration::from_secs(25)))
        .http2_keep_alive_timeout(Duration::from_secs(15))
        .http2_keep_alive_while_idle(true)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_millis(req_timeout))
        .gzip(true)
        .build()
        .unwrap();

    let redis_client = redis::new(
        &std::env::var("REDIS_URL").expect("REDIS_URL not found"),
        std::env::var("POLL_INTERVAL")
            .map(|n| n.parse().unwrap())
            .unwrap_or(100u64)
            .max(10u64),
        req_timeout,
    )
    .await
    .unwrap();

    let url_vars: HashMap<String, String> = std::env::vars()
        .filter(|(k, _)| k.starts_with("URL_"))
        .collect();

    let header_vars: HashMap<String, HeaderValue> = std::env::vars()
        .filter(|(k, _)| k.starts_with("HEADER_"))
        .map(|(k, v)| (k, v.parse().expect("invalid header value")))
        .collect();

    let ecdsa_pub_keys: Vec<ecdsa::VerifyingKey> = std::env::vars()
        .filter(|(k, _)| k.starts_with("ECDSA_PUB_KEY"))
        .map(|(_, v)| {
            let v = general_purpose::URL_SAFE_NO_PAD
                .decode(v)
                .expect("invalid base64");
            ecdsa::VerifyingKey::from_sec1_bytes(&v).expect("invalid ecdsa key")
        })
        .collect();

    let ed25519_pub_keys: Vec<ed25519_dalek::VerifyingKey> = std::env::vars()
        .filter(|(k, _)| k.starts_with("ED25519_PUB_KEY"))
        .map(|(_, v)| {
            let v = general_purpose::URL_SAFE_NO_PAD
                .decode(v)
                .expect("invalid base64");
            if v.len() != 32 {
                panic!("invalid eddsa key");
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&v);
            ed25519_dalek::VerifyingKey::from_bytes(&key).expect("invalid ecdsa key")
        })
        .collect();

    let handle = axum_server::Handle::new();
    let app = Router::new()
        .route("/*any", routing::any(handler::proxy))
        .with_state(handler::AppState {
            http_client: Arc::new(http_client),
            cacher: Arc::new(redis_client),
            url_vars: Arc::new(url_vars),
            header_vars: Arc::new(header_vars),
            ecdsa_pub_keys: Arc::new(ecdsa_pub_keys),
            ed25519_pub_keys: Arc::new(ed25519_pub_keys),
        });

    let addr: SocketAddr = std::env::var("SERVER_ADDR")
        .unwrap_or("127.0.0.1:8080".to_string())
        .parse()
        .unwrap();

    let cert_file = std::env::var("TLS_CERT_FILE").unwrap_or_default();
    let key_file = std::env::var("TLS_KEY_FILE").unwrap_or_default();
    match key_file.is_empty() {
        true => {
            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            log::warn!(target: "server", "{}@{} listening on {:?}", APP_NAME, APP_VERSION, addr);
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal(handle))
                .await
                .unwrap();
        }
        false => {
            let config = RustlsConfig::from_pem_file(&cert_file, &key_file)
                .await
                .unwrap_or_else(|_| panic!("read tls file failed: {}, {}", cert_file, key_file));
            log::warn!(target: "server", "{}@{} listening on {:?} with tls", APP_NAME, APP_VERSION,addr);
            axum_server::bind_rustls(addr, config)
                .handle(handle)
                .serve(app.into_make_service())
                .await
                .unwrap();
        }
    }
}

async fn shutdown_signal(handle: axum_server::Handle) {
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

    log::warn!(target: "server", "received termination signal, starting graceful shutdown");
    // 10 secs is how long server will wait to force shutdown
    handle.graceful_shutdown(Some(Duration::from_secs(10)));
}
