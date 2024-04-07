use async_trait::async_trait;
use axum::{
    body::Body,
    response::{IntoResponse, Response},
};
use ciborium::{from_reader, into_writer};
use http::{
    header::{HeaderMap, HeaderName, HeaderValue},
    StatusCode,
};
use rustis::bb8::{CustomizeConnection, ErrorSink, Pool};
use rustis::client::PooledClientManager;
use rustis::commands::{GenericCommands, SetCondition, SetExpiration, StringCommands};
use rustis::resp::BulkString;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use tokio::time::{sleep, Duration};

pub type RedisPool = Pool<PooledClientManager>;

pub async fn new(cfg: &str) -> Result<RedisPool, rustis::Error> {
    let manager = PooledClientManager::new(cfg).unwrap();
    RedisPool::builder()
        .max_size(10)
        .min_idle(Some(1))
        .max_lifetime(None)
        .idle_timeout(Some(Duration::from_secs(600)))
        .connection_timeout(Duration::from_secs(3))
        .error_sink(Box::new(RedisMonitor {}))
        .connection_customizer(Box::new(RedisMonitor {}))
        .build(manager)
        .await
}

#[derive(Debug, Clone, Copy)]
struct RedisMonitor;

impl<E: std::fmt::Display> ErrorSink<E> for RedisMonitor {
    fn sink(&self, error: E) {
        log::error!(target: "redis", "{}", error);
    }

    fn boxed_clone(&self) -> Box<dyn ErrorSink<E>> {
        Box::new(*self)
    }
}

#[async_trait]
impl<C: Send + 'static, E: 'static> CustomizeConnection<C, E> for RedisMonitor {
    async fn on_acquire(&self, _connection: &mut C) -> Result<(), E> {
        log::info!(target: "redis", "connection acquired");
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct ResponseData {
    pub headers: Vec<(String, ByteBuf)>,
    pub body: ByteBuf,
}

impl ResponseData {
    pub async fn try_get(pool: &RedisPool, key: &str) -> anyhow::Result<Option<Self>> {
        let conn = pool.get().await?;
        let mut counter = 30;
        while counter > 0 {
            let res: Option<BulkString> = conn.get(key).await?;
            match res {
                None => return Ok(None),
                Some(bs) => {
                    if bs.len() > 1 {
                        let data: ResponseData = from_reader(bs.as_bytes())?;
                        return Ok(Some(data));
                    }
                }
            }

            counter -= 1;
            sleep(Duration::from_secs(1)).await;
        }

        Err(anyhow::anyhow!("get cache timeout"))
    }

    pub async fn lock_for_set(pool: &RedisPool, key: &str) -> anyhow::Result<bool> {
        let conn = pool.get().await?;
        let res = conn
            .set_with_options(
                key,
                BulkString::from(vec![0]),
                SetCondition::NX,
                SetExpiration::Ex(60),
                false,
            )
            .await?;
        Ok(res)
    }

    pub async fn clear(pool: &RedisPool, key: &str) -> anyhow::Result<()> {
        let conn = pool.get().await?;
        let _ = conn.del(key).await?;
        Ok(())
    }

    pub async fn set(pool: &RedisPool, key: &str, data: Self) -> anyhow::Result<bool> {
        let conn = pool.get().await?;
        let mut buf = Vec::new();
        into_writer(&data, &mut buf)?;
        let res = conn
            .set_with_options(
                key,
                BulkString::from(buf),
                SetCondition::XX,
                SetExpiration::Ex(60),
                false,
            )
            .await?;
        Ok(res)
    }

    pub fn from_response(headers: &HeaderMap, body: &[u8]) -> Self {
        let mut h = Vec::new();
        for (k, v) in headers.iter() {
            h.push((k.as_str().to_string(), ByteBuf::from(v.as_bytes().to_vec())));
        }
        Self {
            headers: h,
            body: ByteBuf::from(body.to_vec()),
        }
    }
}

impl IntoResponse for ResponseData {
    fn into_response(self) -> Response {
        let mut res = Response::new(Body::from(self.body.into_vec()));
        *res.status_mut() = StatusCode::OK;
        *res.version_mut() = http::Version::HTTP_11;
        for (ref k, v) in self.headers {
            res.headers_mut().insert(
                HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_bytes(v.as_slice()).unwrap(),
            );
        }
        res
    }
}
