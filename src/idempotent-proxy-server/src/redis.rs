use async_trait::async_trait;
use rustis::bb8::{CustomizeConnection, ErrorSink, Pool};
use rustis::client::PooledClientManager;
use rustis::commands::{GenericCommands, SetCondition, SetExpiration, StringCommands};
use rustis::resp::BulkString;
use tokio::time::{sleep, Duration};

use idempotent_proxy_types::{cache::Cacher, err_string};

pub struct RedisClient {
    pool: Pool<PooledClientManager>,
    pub poll_interval: u64,
    pub cache_ttl: u64,
}

pub async fn new(
    url: &str,
    poll_interval: u64,
    cache_ttl: u64,
) -> Result<RedisClient, rustis::Error> {
    let manager = PooledClientManager::new(url).unwrap();
    let pool = Pool::builder()
        .max_size(10)
        .min_idle(Some(1))
        .max_lifetime(None)
        .idle_timeout(Some(Duration::from_secs(600)))
        .connection_timeout(Duration::from_secs(3))
        .error_sink(Box::new(RedisMonitor {}))
        .connection_customizer(Box::new(RedisMonitor {}))
        .build(manager)
        .await?;
    Ok(RedisClient {
        pool,
        poll_interval,
        cache_ttl,
    })
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

#[async_trait]
impl Cacher for RedisClient {
    async fn obtain(&self, key: &str, ttl: u64) -> Result<bool, String> {
        let conn = self.pool.get().await.map_err(err_string)?;
        let res = conn
            .set_with_options(
                key,
                BulkString::from(vec![0]),
                SetCondition::NX,
                SetExpiration::Px(ttl),
                false,
            )
            .await
            .map_err(err_string)?;
        Ok(res)
    }

    async fn polling_get(
        &self,
        key: &str,
        poll_interval: u64,
        counter: u64,
    ) -> Result<Vec<u8>, String> {
        let conn = self.pool.get().await.map_err(err_string)?;
        let mut counter = counter;
        while counter > 0 {
            let res: Option<BulkString> = conn.get(key).await.map_err(err_string)?;
            match res {
                None => return Err("not obtained".to_string()),
                Some(bs) => {
                    if bs.len() > 1 {
                        return Ok(bs.into());
                    }
                }
            }

            counter -= 1;
            sleep(Duration::from_millis(poll_interval)).await;
        }

        Err(("polling get cache timeout").to_string())
    }

    async fn set(&self, key: &str, val: Vec<u8>, ttl: u64) -> Result<bool, String> {
        let conn = self.pool.get().await.map_err(err_string)?;
        let res = conn
            .set_with_options(
                key,
                BulkString::from(val),
                SetCondition::XX,
                SetExpiration::Px(ttl),
                false,
            )
            .await
            .map_err(err_string)?;
        Ok(res)
    }

    async fn del(&self, key: &str) -> Result<(), String> {
        let conn = self.pool.get().await.map_err(err_string)?;
        let _ = conn.del(key).await.map_err(err_string)?;
        Ok(())
    }
}
