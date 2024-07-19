use async_trait::async_trait;
use idempotent_proxy_types::unix_ms;
use std::{
    collections::{
        hash_map::{Entry, HashMap},
        BTreeSet,
    },
    sync::Arc,
};
use tokio::{
    sync::RwLock,
    time::{sleep, Duration},
};

use super::Cacher;

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
struct PriorityKey(u64, String);

#[derive(Clone, Default)]
pub struct MemoryCacher {
    priority_queue: Arc<RwLock<BTreeSet<PriorityKey>>>,
    kv: Arc<RwLock<HashMap<String, (u64, Vec<u8>)>>>,
}

impl MemoryCacher {
    fn clean_expired_values(&self) -> tokio::task::JoinHandle<()> {
        let kv = self.kv.clone();
        let priority_queue = self.priority_queue.clone();
        tokio::spawn(async move {
            let now = unix_ms();
            let mut pq = priority_queue.write().await;
            let mut kv = kv.write().await;
            while let Some(PriorityKey(expire_at, key)) = pq.pop_first() {
                if expire_at > now {
                    pq.insert(PriorityKey(expire_at, key));
                    break;
                }

                kv.remove(&key);
            }
        })
    }
}

#[async_trait]
impl Cacher for MemoryCacher {
    async fn obtain(&self, key: &str, ttl: u64) -> Result<bool, String> {
        let mut kv = self.kv.write().await;
        let now = unix_ms();
        match kv.entry(key.to_string()) {
            Entry::Occupied(mut entry) => {
                let (expire_at, value) = entry.get_mut();
                if *expire_at > now {
                    return Ok(false);
                }

                let mut pq = self.priority_queue.write().await;
                pq.remove(&PriorityKey(*expire_at, key.to_string()));

                *expire_at = now + ttl;
                *value = vec![];
                pq.insert(PriorityKey(*expire_at, key.to_string()));
                Ok(true)
            }
            Entry::Vacant(entry) => {
                let expire_at = now + ttl;
                entry.insert((expire_at, vec![]));
                self.priority_queue
                    .write()
                    .await
                    .insert(PriorityKey(expire_at, key.to_string()));
                Ok(true)
            }
        }
    }

    async fn polling_get(
        &self,
        key: &str,
        poll_interval: u64,
        mut counter: u64,
    ) -> Result<Vec<u8>, String> {
        while counter > 0 {
            let kv = self.kv.read().await;
            let res = kv.get(key);
            match res {
                None => return Err("not obtained".to_string()),
                Some((expire_at, value)) => {
                    if *expire_at <= unix_ms() {
                        self.clean_expired_values();
                    }

                    if value.len() > 0 {
                        return Ok(value.clone());
                    }
                }
            }

            counter -= 1;
            sleep(Duration::from_millis(poll_interval)).await;
        }

        Err(("polling get cache timeout").to_string())
    }

    async fn set(&self, key: &str, val: Vec<u8>, ttl: u64) -> Result<bool, String> {
        let mut kv = self.kv.write().await;
        match kv.get_mut(key) {
            Some((expire_at, value)) => {
                let now = unix_ms();
                if *expire_at <= now {
                    kv.remove(key);
                    self.clean_expired_values();
                    return Err("value expired".to_string());
                }

                let mut pq = self.priority_queue.write().await;
                pq.remove(&PriorityKey(*expire_at, key.to_string()));

                *expire_at = now + ttl;
                *value = val;
                pq.insert(PriorityKey(*expire_at, key.to_string()));
                Ok(true)
            }
            None => Err("not obtained".to_string()),
        }
    }

    async fn del(&self, key: &str) -> Result<(), String> {
        let mut kv = self.kv.write().await;
        if let Some(val) = kv.remove(key) {
            let mut pq = self.priority_queue.write().await;
            pq.remove(&PriorityKey(val.0, key.to_string()));
        }
        self.clean_expired_values();
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn memory_cacher() {
        let mc = MemoryCacher::default();

        assert!(mc.obtain("key1", 100).await.unwrap());
        assert!(!mc.obtain("key1", 100).await.unwrap());
        assert!(mc.polling_get("key1", 10, 2).await.is_err());
        assert!(mc.set("key", vec![1, 2, 3, 4], 100).await.is_err());
        assert!(mc.set("key1", vec![1, 2, 3, 4], 100).await.is_ok());
        assert!(!mc.obtain("key1", 100).await.unwrap());
        assert_eq!(
            mc.polling_get("key1", 10, 2).await.unwrap(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(
            mc.polling_get("key1", 10, 2).await.unwrap(),
            vec![1, 2, 3, 4]
        );

        assert!(mc.del("key").await.is_ok());
        assert!(mc.del("key1").await.is_ok());
        assert!(mc.polling_get("key1", 10, 2).await.is_err());
        assert!(mc.set("key1", vec![1, 2, 3, 4], 100).await.is_err());
        assert!(mc.obtain("key1", 100).await.unwrap());
        assert!(mc.set("key1", vec![1, 2, 3, 4], 100).await.is_ok());
        assert_eq!(
            mc.polling_get("key1", 10, 2).await.unwrap(),
            vec![1, 2, 3, 4]
        );

        sleep(Duration::from_millis(200)).await;
        assert!(mc.polling_get("key1", 10, 2).await.is_ok());
        assert!(mc.set("key1", vec![1, 2, 3, 4], 100).await.is_err());
        assert!(mc.del("key1").await.is_ok());

        assert!(mc.obtain("key1", 100).await.unwrap());
        sleep(Duration::from_millis(200)).await;
        let _ = mc.clean_expired_values().await;
        println!("{:?}", mc.priority_queue.read().await);

        let res = futures::try_join!(
            mc.obtain("key1", 100),
            mc.obtain("key1", 100),
            mc.obtain("key1", 100),
        )
        .unwrap();
        match res {
            (true, false, false) | (false, true, false) | (false, false, true) => {}
            _ => panic!("unexpected result"),
        }

        assert_eq!(mc.kv.read().await.len(), 1);
        assert_eq!(mc.priority_queue.read().await.len(), 1);

        sleep(Duration::from_millis(200)).await;
        assert_eq!(mc.kv.read().await.len(), 1);
        assert_eq!(mc.priority_queue.read().await.len(), 1);
        let _ = mc.clean_expired_values().await;

        assert!(mc.kv.read().await.is_empty());
        assert!(mc.priority_queue.read().await.is_empty());
    }
}
