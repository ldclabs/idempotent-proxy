use async_trait::async_trait;
use axum::{
    body::Body,
    response::{IntoResponse, Response},
};
use http::{
    header::{HeaderMap, HeaderName, HeaderValue},
    StatusCode,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_bytes::ByteBuf;

#[async_trait]
pub trait Cacher<T: Serialize + DeserializeOwned> {
    async fn obtain(&self, key: &str, ttl_ms: u64) -> anyhow::Result<bool>;
    async fn polling_get(&self, key: &str, poll_interval_ms: u64) -> anyhow::Result<Option<T>>;
    async fn set(&self, key: &str, val: &T, ttl_ms: u64) -> anyhow::Result<bool>;
    async fn del(&self, key: &str) -> anyhow::Result<()>;
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ResponseData {
    pub headers: Vec<(String, String)>,
    pub body: ByteBuf,
    pub mime: ContentType,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ContentType {
    Json(String),
    Cbor(String),
    Other(String),
}

impl ContentType {
    pub fn into_string(self) -> String {
        match self {
            Self::Json(v) => v,
            Self::Cbor(v) => v,
            Self::Other(v) => v,
        }
    }
}

impl Default for ContentType {
    fn default() -> Self {
        Self::Other("text/plain".to_string())
    }
}

impl ResponseData {
    pub fn with_headers(&mut self, headers: &HeaderMap, filtering: &str) {
        let filtering: Vec<&str> = filtering
            .split(',')
            .filter_map(|s| {
                let s = s.trim();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            })
            .collect();
        self.headers.reserve_exact(if filtering.is_empty() {
            headers.len()
        } else {
            filtering.len()
        });

        for (k, v) in headers.iter() {
            if let Ok(v) = v.to_str() {
                let k = k.as_str().to_ascii_lowercase();
                if k == "content-type" {
                    self.mime = match v {
                        v if v.contains("application/json") => ContentType::Json(v.to_string()),
                        v if v.contains("application/cbor") => ContentType::Cbor(v.to_string()),
                        v => ContentType::Other(v.to_string()),
                    };
                } else if k != "content-length"
                    && (filtering.is_empty() || filtering.contains(&k.as_str()))
                {
                    self.headers.push((k, v.to_string()));
                }
            }
        }
    }

    pub fn with_body(&mut self, body: &[u8], filtering: &str) -> anyhow::Result<()> {
        let filtering: Vec<&str> = filtering
            .split(',')
            .filter_map(|s| {
                let s = s.trim();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            })
            .collect();

        match self.mime {
            ContentType::Json(_) if filtering.len() > 0 => {
                let obj: serde_json::Value = serde_json::from_slice(body)?;
                let mut new_obj = serde_json::Map::with_capacity(filtering.len());
                for k in filtering {
                    if let Some(v) = obj.get(&k) {
                        new_obj.insert(k.to_string(), v.to_owned());
                    }
                }
                self.body = ByteBuf::from(serde_json::to_vec(&new_obj)?);
            }
            ContentType::Cbor(_) if filtering.len() > 0 => {
                let obj: ciborium::Value = ciborium::from_reader(body)?;
                let obj = obj
                    .into_map()
                    .and_then(|mut list| {
                        list.retain(|v| v.0.as_text().is_some_and(|t| filtering.contains(&t)));
                        Ok(list)
                    })
                    .map(|v| ciborium::Value::Map(v));
                let mut buf = Vec::new();
                match obj {
                    Ok(v) => ciborium::into_writer(&v, &mut buf)?,
                    Err(v) => ciborium::into_writer(&v, &mut buf)?,
                }
                self.body = ByteBuf::from(buf);
            }
            _ => {
                self.body.extend_from_slice(body);
            }
        }
        Ok(())
    }
}

impl IntoResponse for ResponseData {
    fn into_response(self) -> Response {
        let body = self.body.into_vec();
        let len = body.len();
        let mut res = Response::new(Body::from(body));
        *res.status_mut() = StatusCode::OK;
        *res.version_mut() = http::Version::HTTP_11;
        for (ref k, v) in self.headers {
            res.headers_mut().append(
                HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_bytes(v.as_bytes()).unwrap(),
            );
        }

        res.headers_mut().insert(
            "content-type",
            HeaderValue::from_bytes(self.mime.into_string().as_bytes()).unwrap(),
        );
        res.headers_mut().insert("content-length", len.into());
        res
    }
}
