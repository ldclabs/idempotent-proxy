use async_trait::async_trait;
use axum::{
    body::Body,
    response::{IntoResponse, Response},
};
use ciborium::{from_reader, into_writer, Value};
use http::{
    header::{HeaderMap, HeaderName, HeaderValue},
    StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::err_string;

#[async_trait]
pub trait Cacher {
    async fn obtain(&self, key: &str, ttl_ms: u64) -> Result<bool, String>;
    async fn polling_get(
        &self,
        key: &str,
        poll_interval_ms: u64,
        counter: u64,
    ) -> Result<Vec<u8>, String>;
    async fn set(&self, key: &str, val: Vec<u8>, ttl_ms: u64) -> Result<bool, String>;
    async fn del(&self, key: &str) -> Result<(), String>;
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ResponseData {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: ByteBuf,
    pub mime: String,
}

impl Default for ResponseData {
    fn default() -> Self {
        Self::new(200)
    }
}

impl ResponseData {
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: ByteBuf::new(),
            mime: "text/plain".to_string(),
        }
    }

    pub fn with_headers(&mut self, headers: &HeaderMap, filtering: &str) {
        let filtering = filtering.to_ascii_lowercase();
        let filtering: Vec<&str> = split_filtering(filtering.as_str());
        self.headers.reserve_exact(if filtering.is_empty() {
            headers.len()
        } else {
            filtering.len()
        });

        for (k, v) in headers.iter() {
            if let Ok(v) = v.to_str() {
                let k = k.as_str();
                if k == "content-type" {
                    self.mime = v.to_string();
                } else if k != "content-length" && (filtering.is_empty() || filtering.contains(&k))
                {
                    self.headers.push((k.to_string(), v.to_string()));
                }
            }
        }
    }

    pub fn with_body(&mut self, body: &[u8], filtering: &str) -> Result<(), String> {
        let filtering: Vec<&str> = split_filtering(filtering);
        if self.status >= 300 || filtering.is_empty() {
            self.body.extend_from_slice(body);
            return Ok(());
        }

        match &self.mime {
            v if !filtering.is_empty() && v.contains("application/json") => {
                let obj: serde_json::Value = serde_json::from_slice(body).map_err(err_string)?;
                let mut new_obj = serde_json::Map::with_capacity(filtering.len());
                for k in filtering {
                    if let Some(v) = obj.get(k) {
                        new_obj.insert(k.to_string(), v.to_owned());
                    }
                }
                self.body = ByteBuf::from(serde_json::to_vec(&new_obj).map_err(err_string)?);
            }
            v if !filtering.is_empty() && v.contains("application/cbor") => {
                let obj: Value = from_reader(body).map_err(err_string)?;
                let obj = obj
                    .into_map()
                    .map(|mut list| {
                        list.retain(|v| v.0.as_text().is_some_and(|t| filtering.contains(&t)));
                        list
                    })
                    .map(Value::Map);
                let mut buf = Vec::new();
                match obj {
                    Ok(v) => into_writer(&v, &mut buf).map_err(err_string)?,
                    Err(_) => into_writer(&Value::Map(vec![]), &mut buf).map_err(err_string)?,
                }
                self.body = ByteBuf::from(buf);
            }
            _ => {
                self.body.extend_from_slice(body);
            }
        }
        Ok(())
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        into_writer(self, &mut buf).map_err(err_string)?;
        Ok(buf)
    }
}

impl TryFrom<&[u8]> for ResponseData {
    type Error = String;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        from_reader(value).map_err(err_string)
    }
}

fn split_filtering(filtering: &str) -> Vec<&str> {
    filtering
        .split(',')
        .filter_map(|s| {
            let s = s.trim();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        })
        .collect()
}

impl IntoResponse for ResponseData {
    fn into_response(self) -> Response {
        let body = self.body.into_vec();
        let len = body.len();
        let mut res = Response::new(Body::from(body));
        *res.status_mut() = StatusCode::from_u16(self.status).unwrap_or(StatusCode::OK);
        for (ref k, v) in self.headers {
            res.headers_mut().append(
                HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_bytes(v.as_bytes()).unwrap(),
            );
        }

        res.headers_mut().insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_bytes(self.mime.as_bytes()).unwrap(),
        );
        res.headers_mut()
            .insert(http::header::CONTENT_LENGTH, len.into());
        res
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use hex::prelude::*;

    #[test]
    fn test_split_filtering() {
        assert_eq!(split_filtering("").len(), 0);
        assert_eq!(split_filtering("args,url"), vec!["args", "url"]);
        assert_eq!(split_filtering("args, url"), vec!["args", "url"]);
    }

    #[test]
    fn test_response_data() {
        let mut rd = ResponseData::new(200);
        rd.headers
            .push(("accept".to_string(), "application/json".to_string()));
        rd.body.extend_from_slice(b"Hello, World!");
        let data = rd.to_bytes().unwrap();
        let rd2 = ResponseData::try_from(data.as_slice()).unwrap();
        assert_eq!(rd2, rd);
        println!("rd: {}", data.to_lower_hex_string());

        let mut rd = ResponseData::new(200);
        let mut headers = HeaderMap::new();
        headers.insert("Date", "Wed, 22 May 2024 11:11:17 GMT".parse().unwrap());
        headers.insert("Content-Length", "123".parse().unwrap());
        headers.insert("Content-Type", "application/json".parse().unwrap());
        rd.with_headers(&headers, "date, content-type, Content-Length");
        assert_eq!(rd.mime, "application/json");
        assert_eq!(
            rd.headers,
            vec![(
                "date".to_string(),
                "Wed, 22 May 2024 11:11:17 GMT".to_string()
            )]
        )
    }

    #[test]
    fn test_response_data_in_json() {
        use serde_json::json;

        let mut rd = ResponseData::new(200);
        let mut headers = HeaderMap::new();
        headers.insert("Date", "Wed, 22 May 2024 11:11:17 GMT".parse().unwrap());
        headers.insert("Content-Length", "123".parse().unwrap());
        headers.insert("Content-Type", "application/json".parse().unwrap());
        rd.with_headers(&headers, "date, content-type, Content-Length");
        assert_eq!(rd.mime, "application/json");
        let body = json!({
            "args": {"api-key": "abc123"},
            "headers": {"Accept": "*/*"},
            "origin": "120.204.60.218",
            "url": "https://httpbin.org/get?api-key=abc123",
        });

        rd.with_body(
            serde_json::to_vec(&body).unwrap().as_slice(),
            "args,url,Origin",
        )
        .unwrap();
        assert_eq!(
            rd.body.as_slice(),
            r#"{"args":{"api-key":"abc123"},"url":"https://httpbin.org/get?api-key=abc123"}"#
                .as_bytes()
        );
    }

    #[test]
    fn test_response_data_in_cbor() {
        use ciborium::cbor;
        let mut rd = ResponseData::new(200);
        let mut headers = HeaderMap::new();
        headers.insert("Date", "Wed, 22 May 2024 11:11:17 GMT".parse().unwrap());
        headers.insert("Content-Length", "123".parse().unwrap());
        headers.insert("Content-Type", "application/cbor".parse().unwrap());
        rd.with_headers(&headers, "date, content-type, Content-Length");
        assert_eq!(rd.mime, "application/cbor");
        let body = cbor!({
            "args" => {"api-key" => "abc123"},
            "headers" => {"Accept" => "*/*"},
            "origin" => "120.204.60.218",
            "url" => "https://httpbin.org/get?api-key=abc123",
        })
        .unwrap();
        let mut buf = Vec::new();
        into_writer(&body, &mut buf).unwrap();
        rd.with_body(&buf, "args,url,Origin").unwrap();

        let body = cbor!({
            "args" => {"api-key" => "abc123"},
            "url" => "https://httpbin.org/get?api-key=abc123",
        })
        .unwrap();
        buf.clear();
        into_writer(&body, &mut buf).unwrap();
        assert_eq!(rd.body.as_slice(), buf.as_slice());
    }
}
