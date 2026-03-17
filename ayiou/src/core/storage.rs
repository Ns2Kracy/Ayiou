use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Serialize, de::DeserializeOwned};

pub trait StoreCodec: Send + Sync + 'static {
    fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>>;
    fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T>;
}

#[derive(Default, Clone, Copy)]
pub struct JsonCodec;

impl StoreCodec for JsonCodec {
    fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(value)?)
    }

    fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

#[async_trait]
pub trait Store: Send + Sync + 'static {
    async fn get_raw(&self, key: &str) -> Result<Option<Vec<u8>>>;
    async fn set_raw(&self, key: &str, value: Vec<u8>) -> Result<()>;
    async fn delete(&self, key: &str) -> Result<bool>;
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>>;
}

#[async_trait]
pub trait StoreSerdeExt: Store {
    async fn get_with<T: DeserializeOwned + Send, C>(
        &self,
        key: &str,
        codec: &C,
    ) -> Result<Option<T>>
    where
        C: StoreCodec,
    {
        let raw = self.get_raw(key).await?;
        raw.map(|bytes| codec.decode::<T>(&bytes)).transpose()
    }

    async fn set_with<T: Serialize + Send + Sync, C>(
        &self,
        key: &str,
        value: &T,
        codec: &C,
    ) -> Result<()>
    where
        C: StoreCodec,
    {
        let bytes = codec.encode(value)?;
        self.set_raw(key, bytes).await
    }

    async fn get_json<T: DeserializeOwned + Send>(&self, key: &str) -> Result<Option<T>> {
        self.get_with(key, &JsonCodec).await
    }

    async fn set_json<T: Serialize + Send + Sync>(&self, key: &str, value: &T) -> Result<()> {
        self.set_with(key, value, &JsonCodec).await
    }
}

impl<T: Store + ?Sized> StoreSerdeExt for T {}

#[derive(Default)]
pub struct MemoryStore {
    data: DashMap<String, Arc<[u8]>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Store for MemoryStore {
    async fn get_raw(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.data.get(key).map(|v| v.value().to_vec()))
    }

    async fn set_raw(&self, key: &str, value: Vec<u8>) -> Result<()> {
        self.data.insert(key.to_string(), Arc::from(value));
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<bool> {
        Ok(self.data.remove(key).is_some())
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        let mut keys: Vec<String> = self
            .data
            .iter()
            .filter_map(|entry| {
                if entry.key().starts_with(prefix) {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect();
        keys.sort();
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_store_json_roundtrip() {
        #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq)]
        struct Data {
            n: i32,
        }

        let store = MemoryStore::new();
        store
            .set_json("plugin:test", &Data { n: 42 })
            .await
            .unwrap();

        let got: Option<Data> = store.get_json("plugin:test").await.unwrap();
        assert_eq!(got, Some(Data { n: 42 }));

        let keys = store.list_prefix("plugin:").await.unwrap();
        assert_eq!(keys, vec!["plugin:test".to_string()]);
    }

    struct PlainStringCodec;

    impl StoreCodec for PlainStringCodec {
        fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
            let s = serde_json::to_string(value)?;
            Ok(s.into_bytes())
        }

        fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T> {
            let s = std::str::from_utf8(bytes)?;
            Ok(serde_json::from_str::<T>(s)?)
        }
    }

    #[tokio::test]
    async fn memory_store_custom_codec_roundtrip() {
        let store = MemoryStore::new();
        let codec = PlainStringCodec;
        store
            .set_with("plugin:num", &123_i32, &codec)
            .await
            .unwrap();
        let got: Option<i32> = store.get_with("plugin:num", &codec).await.unwrap();
        assert_eq!(got, Some(123));
    }
}
