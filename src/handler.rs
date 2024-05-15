use crate::{
    auth,
    redis::{RedisClient, ResponseData},
};

use axum::{
    body::to_bytes,
    extract::{Request, State},
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose, Engine};
use http::{header::AsHeaderName, HeaderName, HeaderValue};
use hyper::{HeaderMap, StatusCode};
use k256::ecdsa;
use reqwest::Client;
use serde_json::{from_reader, Map, Value};
use sha3::{Digest, Sha3_256};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone)]
pub struct AppState {
    pub http_client: Arc<Client>,
    pub redis_client: Arc<RedisClient>,
    pub url_vars: Arc<HashMap<String, String>>,
    pub header_vars: Arc<HashMap<String, HeaderValue>>,
    pub ecdsa_pub_keys: Arc<Vec<ecdsa::VerifyingKey>>,
    pub ed25519_pub_keys: Arc<Vec<ed25519_dalek::VerifyingKey>>,
}

static HEADER_PROXY_AUTHORIZATION: HeaderName = HeaderName::from_static("proxy-authorization");
static HEADER_X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");
static HEADER_X_FORWARDED_HOST: HeaderName = HeaderName::from_static("x-forwarded-host");
static HEADER_X_FORWARDED_PROTO: HeaderName = HeaderName::from_static("x-forwarded-proto");
static HEADER_IDEMPOTENCY_KEY: HeaderName = HeaderName::from_static("idempotency-key");
static HEADER_X_JSON_MASK: HeaderName = HeaderName::from_static("x-json-mask");
static HEADER_RESPONSE_HEADERS: HeaderName = HeaderName::from_static("response-headers");

impl AppState {
    pub fn alter_headers(&self, headers: &mut HeaderMap) {
        headers.remove(&http::header::HOST);
        headers.remove(&http::header::FORWARDED);
        headers.remove(&HEADER_PROXY_AUTHORIZATION);
        headers.remove(&HEADER_X_FORWARDED_FOR);
        headers.remove(&HEADER_X_FORWARDED_HOST);
        headers.remove(&HEADER_X_FORWARDED_PROTO);

        if !self.header_vars.is_empty() {
            for val in headers.values_mut() {
                if let Ok(s) = val.to_str() {
                    if let Some(v) = self.header_vars.get(s) {
                        *val = v.clone();
                    }
                }
            }
        }
    }

    // TODO: support JWT and CWT
    pub fn verify_token(&self, access_token: &str) -> Result<String, String> {
        if !access_token.starts_with("Bearer ") {
            return Err("invalid proxy-authorization header".to_string());
        }
        let token = general_purpose::URL_SAFE_NO_PAD
            .decode(access_token.strip_prefix("Bearer ").unwrap().as_bytes())
            .map_err(|err| err.to_string())?;
        if !self.ecdsa_pub_keys.is_empty() {
            return auth::ecdsa_verify(&self.ecdsa_pub_keys, &token)
                .map(|t| t.1)
                .map_err(|err| format!("proxy authentication verify failed: {}", err));
        }
        if !self.ed25519_pub_keys.is_empty() {
            return auth::ed25519_verify(&self.ed25519_pub_keys, &token)
                .map(|t| t.1)
                .map_err(|err| format!("proxy authentication verify failed: {}", err));
        }

        Err("proxy authentication verify failed".to_string())
    }
}

pub async fn proxy(
    State(app): State<AppState>,
    req: Request,
) -> Result<Response, (StatusCode, String)> {
    let method = req.method().to_string();
    let path = req.uri().path();
    let url = if path.starts_with("/URL_") {
        let url = app
            .url_vars
            .get(path.strip_prefix('/').unwrap())
            .map(|s| s.to_string())
            .unwrap_or_default();
        if !url.starts_with("http") {
            return Err((StatusCode::BAD_REQUEST, format!("invalid url: {}", url)));
        }

        url
    } else {
        let host = extract_header(req.headers(), &HEADER_X_FORWARDED_HOST, || "".to_string());
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
    let idempotency_key = extract_header(req.headers(), &HEADER_IDEMPOTENCY_KEY, || "".to_string());
    if idempotency_key.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "missing idempotency-key header".to_string(),
        ));
    }

    // Access control
    let subject = if !app.ecdsa_pub_keys.is_empty() || !app.ed25519_pub_keys.is_empty() {
        let token = extract_header(req.headers(), &HEADER_PROXY_AUTHORIZATION, || {
            "".to_string()
        });

        match app.verify_token(&token) {
            Err(err) => return Err((StatusCode::PROXY_AUTHENTICATION_REQUIRED, err)),
            Ok(subject) => subject,
        }
    } else {
        "".to_string()
    };

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
                    subject = subject,
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
        let json_mask = extract_header(req.headers(), &HEADER_X_JSON_MASK, || "".to_string())
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
        let response_headers =
            extract_header(req.headers(), &HEADER_RESPONSE_HEADERS, || "".to_string())
                .split(',')
                .filter_map(|s| match HeaderName::from_bytes(s.trim().as_bytes()) {
                    Ok(v) => Some(v),
                    Err(_) => None,
                })
                .collect::<Vec<HeaderName>>();
        let mut headers = req.headers().clone();
        app.alter_headers(&mut headers);

        let mut rreq = reqwest::Request::new(method.clone(), url.clone());
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
            let json_filtering = !json_mask.is_empty()
                && headers
                    .get("content-type")
                    .is_some_and(|v| v.to_str().is_ok_and(|v| v.contains("application/json")));

            let headers = if response_headers.is_empty() {
                headers
            } else {
                // Response headers filtering
                headers
                    .iter()
                    .filter(|(k, _)| response_headers.contains(k))
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect()
            };

            let data = if json_filtering {
                // JSON response filtering
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
                subject = subject,
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
                subject = subject,
                idempotency_key = idempotency_key;
                "{}", msg);
            Err((status, msg))
        }
    }
}

pub fn extract_header<K>(hm: &HeaderMap, key: K, or: impl FnOnce() -> String) -> String
where
    K: AsHeaderName,
{
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

#[cfg(test)]
mod test {

    #[test]
    fn test_challenge() {}
}
