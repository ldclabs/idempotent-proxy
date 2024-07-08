use axum::{
    body::to_bytes,
    extract::{Request, State},
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose, Engine};
use http::{header::AsHeaderName, HeaderMap, HeaderValue, StatusCode};
use k256::ecdsa;
use reqwest::Client;
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use crate::redis::RedisClient;
use idempotent_proxy_types::auth;
use idempotent_proxy_types::cache::{Cacher, ResponseData};
use idempotent_proxy_types::*;

#[derive(Clone)]
pub struct AppState {
    pub http_client: Arc<Client>,
    pub cacher: Arc<RedisClient>,
    pub agents: Arc<BTreeSet<String>>,
    pub url_vars: Arc<HashMap<String, String>>,
    pub header_vars: Arc<HashMap<String, HeaderValue>>,
    pub ecdsa_pub_keys: Arc<Vec<ecdsa::VerifyingKey>>,
    pub ed25519_pub_keys: Arc<Vec<ed25519_dalek::VerifyingKey>>,
}

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
    // Access control
    let agent = if !app.ecdsa_pub_keys.is_empty() || !app.ed25519_pub_keys.is_empty() {
        let token = extract_header(req.headers(), &HEADER_PROXY_AUTHORIZATION, || {
            "".to_string()
        });

        match app.verify_token(&token) {
            Err(err) => return Err((StatusCode::PROXY_AUTHENTICATION_REQUIRED, err)),
            Ok(agent) => agent,
        }
    } else {
        "ANON".to_string()
    };

    if !app.agents.is_empty() && !app.agents.contains(&agent) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("agent {} is not allowed", agent),
        ));
    }

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
                "missing header: x-forwarded-host".to_string(),
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
            "missing header: idempotency-key".to_string(),
        ));
    }

    let idempotency_key = format!("{}:{}:{}", agent, method, idempotency_key);

    let lock = app
        .cacher
        .obtain(&idempotency_key, app.cacher.cache_ttl)
        .await
        .map_err(err_response)?;
    if !lock {
        let data = app
            .cacher
            .polling_get(
                &idempotency_key,
                app.cacher.poll_interval,
                app.cacher.cache_ttl / app.cacher.poll_interval,
            )
            .await
            .map_err(err_response)?;

        let res = ResponseData::try_from(&data[..]).map_err(err_response)?;
        log::info!(target: "handler",
                    action = "cachehit",
                    method = method,
                    url = url.to_string(),
                    status = res.status,
                    agent = agent,
                    idempotency_key = idempotency_key;
                    "");
        return Ok(res.into_response());
    }

    let res = {
        let method = req.method();
        let json_mask = extract_header(req.headers(), &HEADER_X_JSON_MASK, || "".to_string());
        let response_headers =
            extract_header(req.headers(), &HEADER_RESPONSE_HEADERS, || "".to_string());

        let mut headers = req.headers().clone();
        app.alter_headers(&mut headers);

        let mut rreq = reqwest::Request::new(method.clone(), url.clone());
        *rreq.headers_mut() = headers;

        if !method.is_safe() {
            let body = to_bytes(req.into_body(), 1024 * 1024)
                .await
                .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
            *rreq.body_mut() = Some(reqwest::Body::from(body));
        }

        let rres = app.http_client.execute(rreq).await.map_err(err_response)?;
        let status = rres.status();
        let headers = rres.headers().to_owned();
        let res_body = rres.bytes().await.map_err(err_response)?;

        // If the HTTP status code is 500 or below, it's considered a server response and should be cached; any exceptions should be handled by the client. Otherwise, it's considered a non-response from the server and should not be cached.
        if status >= StatusCode::OK && status <= StatusCode::INTERNAL_SERVER_ERROR {
            let mut rd = ResponseData::new(status.as_u16());
            rd.with_headers(&headers, &response_headers);
            rd.with_body(&res_body, &json_mask).map_err(err_response)?;
            let data = rd.to_bytes().map_err(err_response)?;

            let _ = app
                .cacher
                .set(&idempotency_key, data, app.cacher.cache_ttl)
                .await
                .map_err(err_response)?;

            Ok(rd.into_response())
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
                agent = agent,
                idempotency_key = idempotency_key;
                "");
            Ok(res)
        }
        Err((status, msg)) => {
            let _ = app.cacher.del(&idempotency_key).await;
            log::warn!(target: "handler",
                action = "proxying",
                method = method,
                url = url.to_string(),
                status = status.as_u16(),
                agent = agent,
                idempotency_key = idempotency_key;
                "{}", msg);
            Err((status, msg))
        }
    }
}

fn err_response(err: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

fn extract_header<K>(hm: &HeaderMap, key: K, or: impl FnOnce() -> String) -> String
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

#[cfg(test)]
mod test {

    #[test]
    fn test_challenge() {}
}
