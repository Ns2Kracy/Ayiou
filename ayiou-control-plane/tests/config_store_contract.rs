use std::sync::Arc;

use ayiou_admin_proto::ConfigBackend;
use ayiou_control_plane::config_store::{ConfigStore, InMemoryConfigStore};

#[cfg(feature = "sqlite-backend")]
use ayiou_control_plane::config_store::sqlite::SqliteConfigStore;

async fn run_config_store_contract(store: Arc<dyn ConfigStore>) {
    let initial = store.get("bot-a", "echo").await.unwrap();
    assert!(initial.is_none());

    let v1 = store
        .put("bot-a", "echo", ConfigBackend::Toml, "threshold = 1", None)
        .await
        .unwrap();
    assert_eq!(v1, 1);

    let err = store
        .put(
            "bot-a",
            "echo",
            ConfigBackend::Toml,
            "threshold = 2",
            Some(v1 + 1),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("version conflict"));

    let v2 = store
        .put(
            "bot-a",
            "echo",
            ConfigBackend::Toml,
            "threshold = 2",
            Some(v1),
        )
        .await
        .unwrap();
    assert_eq!(v2, 2);

    let loaded = store.get("bot-a", "echo").await.unwrap().unwrap();
    assert_eq!(loaded.version, 2);
    assert_eq!(loaded.backend, ConfigBackend::Toml);
    assert_eq!(loaded.content, "threshold = 2");
}

#[tokio::test]
async fn in_memory_backend_passes_config_store_contract() {
    let store: Arc<dyn ConfigStore> = Arc::new(InMemoryConfigStore::default());
    run_config_store_contract(store).await;
}

#[cfg(feature = "sqlite-backend")]
#[tokio::test]
async fn sqlite_backend_passes_config_store_contract() {
    let store: Arc<dyn ConfigStore> = Arc::new(SqliteConfigStore::in_memory());
    run_config_store_contract(store).await;
}
