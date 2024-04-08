use crate::redis::{RedisPool, ResponseData};

use axum::{
    body::to_bytes,
    extract::{Request, State},
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose, Engine};
use bitcoin::{key::Secp256k1, secp256k1};
use hyper::{HeaderMap, StatusCode};
use reqwest::Client;
use serde_json::{from_reader, Map, Value};
use sha3::{Digest, Sha3_256};
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
    let method = req.method().to_string();
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

    // auth
    if let Ok(pub_key) = std::env::var("TOKEN_PUB_KEY") {
        let pub_key = general_purpose::URL_SAFE_NO_PAD
            .decode(pub_key.as_bytes())
            .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
        let token = extract_header(req.headers(), "authorization", || "".to_string());
        if !token.starts_with("Bearer ") {
            return Err((StatusCode::UNAUTHORIZED, "missing Bearer token".to_string()));
        }
        let token = general_purpose::URL_SAFE_NO_PAD
            .decode(token.strip_prefix("Bearer ").unwrap().as_bytes())
            .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
        let token: (Vec<u8>, Vec<u8>) = ciborium::from_reader(token.as_slice())
            .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
        let digest = sha3_256(&token.0);
        let secp = Secp256k1::verification_only();
        let signature = secp256k1::schnorr::Signature::from_slice(&token.1)
            .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
        let pub_key = secp256k1::PublicKey::from_slice(&pub_key)
            .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
        if secp
            .verify_schnorr(
                &signature,
                &secp256k1::Message::from_digest_slice(&digest).unwrap(),
                &pub_key.into(),
            )
            .is_err()
        {
            return Err((StatusCode::UNAUTHORIZED, "invalid access_token".to_string()));
        }
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
            Some(res) => {
                // if !method.is_safe() {
                //     // drain the body
                //     let _ = to_bytes(req.into_body(), 1024 * 1024 * 2).await;
                // }
                log::info!(target: "handler",
                    action = "cachehit",
                    method = method,
                    url = url.to_string(),
                    status = 200u16,
                    idempotency_key = idempotency_key;
                    "");
                return Ok(res.into_response());
            }
            None => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "get cache failed".to_string(),
                ))
            }
        }
    }

    let res = {
        let method = req.method();
        let json_mask = extract_header(req.headers(), "x-json-mask", || "".to_string())
            .split(',')
            .filter_map(|s| {
                let s = s.trim();
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_string())
                }
            })
            .collect::<Vec<_>>();
        let mut headers = req.headers().clone();
        let mut rreq = reqwest::Request::new(method.clone(), url.clone());
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
            let data = if !json_mask.is_empty()
                && headers
                    .get("content-type")
                    .is_some_and(|v| v.to_str().is_ok_and(|v| v.contains("application/json")))
            {
                let obj: Value = from_reader(&res_body[..])
                    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
                let mut new_obj = Map::with_capacity(json_mask.len());
                for k in json_mask {
                    if let Some(v) = obj.get(&k) {
                        new_obj.insert(k, v.clone());
                    }
                }
                let res_body = serde_json::to_vec(&new_obj)
                    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
                ResponseData::from_response(&headers, &res_body)
            } else {
                ResponseData::from_response(&headers, &res_body)
            };

            let _ = ResponseData::set(&app.redis_client, &idempotency_key, &data)
                .await
                .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

            Ok(data.into_response())
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

    match res {
        Ok(res) => {
            log::info!(target: "handler",
                action = "proxying",
                method = method,
                url = url.to_string(),
                status = 200u16,
                idempotency_key = idempotency_key;
                "");
            Ok(res)
        }
        Err((status, msg)) => {
            let _ = ResponseData::clear(&app.redis_client, &idempotency_key).await;
            log::warn!(target: "handler",
                action = "proxying",
                method = method,
                url = url.to_string(),
                status = status.as_u16(),
                idempotency_key = idempotency_key;
            "");
            Err((status, msg))
        }
    }
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

pub fn sha3_256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    hasher.finalize().into()
}
