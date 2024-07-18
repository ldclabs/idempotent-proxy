use http::header::HeaderName;

pub mod auth;

pub static HEADER_PROXY_AUTHORIZATION: HeaderName = HeaderName::from_static("proxy-authorization");
pub static HEADER_X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");
pub static HEADER_X_FORWARDED_HOST: HeaderName = HeaderName::from_static("x-forwarded-host");
pub static HEADER_X_FORWARDED_PROTO: HeaderName = HeaderName::from_static("x-forwarded-proto");
pub static HEADER_IDEMPOTENCY_KEY: HeaderName = HeaderName::from_static("idempotency-key");
pub static HEADER_X_JSON_MASK: HeaderName = HeaderName::from_static("x-json-mask");
pub static HEADER_RESPONSE_HEADERS: HeaderName = HeaderName::from_static("response-headers");

pub fn err_string(err: impl std::fmt::Display) -> String {
    err.to_string()
}
