use crate::redis::{RedisPool, ResponseData};

use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    response::{IntoResponse, Response},
};
use hyper::{HeaderMap, StatusCode};
use reqwest::Client;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub http_client: Arc<Client>,
    pub redis_client: Arc<RedisPool>,
}

pub async fn proxy(
    State(app): State<AppState>,
    req: Request,
) -> Result<Response, (StatusCode, String)> {
    let method = req.method();
    let path = req.uri().path();
    let url = if path.starts_with("/URL_") {
        let url = std::env::var(path.strip_prefix('/').unwrap()).unwrap_or_default();
        if !url.starts_with("http") {
            return Err((StatusCode::BAD_REQUEST, format!("invalid url: {}", url)));
        }
        url
    } else {
        let host = extract_header(req.headers(), "x-forwarded-host", || "".to_string());
        if host.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "missing x-forwarded-host header".to_string(),
            ));
        }
        let path_query = req
            .uri()
            .path_and_query()
            .map(|v| v.as_str())
            .unwrap_or(path);
        format!("https://{}{}", host, path_query)
    };

    let url =
        reqwest::Url::parse(&url).map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
    let idempotency_key = extract_header(req.headers(), "idempotency-key", || "".to_string());
    if idempotency_key.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "missing idempotency-key header".to_string(),
        ));
    }

    let idempotency_key = format!("{}:{}", method, idempotency_key);

    let lock = ResponseData::lock_for_set(&app.redis_client, &idempotency_key)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    if !lock {
        match ResponseData::try_get(&app.redis_client, &idempotency_key)
            .await
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        {
            Some(res) => return Ok(res.into_response()),
            None => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "get cache failed".to_string(),
                ))
            }
        }
    }

    let res = {
        println!("proxying: {} {}", method, url);
        let mut headers = req.headers().clone();
        let mut rreq = reqwest::Request::new(method.clone(), url);
        headers.remove(http::header::HOST);
        headers.remove(http::header::FORWARDED);
        headers.remove(http::header::HeaderName::from_static("x-forwarded-for"));
        headers.remove(http::header::HeaderName::from_static("x-forwarded-host"));
        headers.remove(http::header::HeaderName::from_static("x-forwarded-proto"));

        *rreq.headers_mut() = headers;
        *rreq.version_mut() = http::Version::HTTP_11;

        if !method.is_safe() {
            let body = to_bytes(req.into_body(), 1024 * 1024 * 2)
                .await
                .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
            *rreq.body_mut() = Some(reqwest::Body::from(body));
        }

        let rres = app
            .http_client
            .execute(rreq)
            .await
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        let status = rres.status();
        let headers = rres.headers().to_owned();
        let res_body = rres
            .bytes()
            .await
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
            .to_vec();

        if status == StatusCode::OK {
            let data = ResponseData::from_response(&headers, &res_body);
            let _ = ResponseData::set(&app.redis_client, &idempotency_key, data)
                .await
                .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
            let mut res = Response::new(Body::from(res_body));
            *res.status_mut() = status;
            *res.headers_mut() = headers;

            Ok(res)
        } else if status.is_success() {
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "unexpected status: {}, body: {}",
                    status,
                    String::from_utf8_lossy(&res_body)
                ),
            ))
        } else {
            Err((status, String::from_utf8_lossy(&res_body).to_string()))
        }
    };

    if res.is_err() {
        let _ = ResponseData::clear(&app.redis_client, &idempotency_key).await;
    }
    res
}

pub fn extract_header(hm: &HeaderMap, key: &str, or: impl FnOnce() -> String) -> String {
    match hm.get(key) {
        None => or(),
        Some(v) => match v.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => or(),
        },
    }
}
