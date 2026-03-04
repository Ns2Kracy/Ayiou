use std::sync::Arc;

use ayiou_admin_proto::ConfigBackend;
use ayiou_control_plane::config_store::{ConfigStore, InMemoryConfigStore};
use uuid::Uuid;

#[cfg(feature = "postgres-backend")]
use ayiou_control_plane::config_store::postgres::PostgresConfigStore;
#[cfg(feature = "redis-backend")]
use ayiou_control_plane::config_store::redis::RedisConfigStore;
#[cfg(feature = "sqlite-backend")]
use ayiou_control_plane::config_store::sqlite::SqliteConfigStore;

async fn run_config_store_contract(store: Arc<dyn ConfigStore>) {
    let suffix = Uuid::new_v4().simple().to_string();
    let bot_id = format!("bot-{}", &suffix[..8]);
    let plugin_name = format!("echo-{}", &suffix[8..16]);

    let initial = store.get(&bot_id, &plugin_name).await.unwrap();
    assert!(initial.is_none());

    let v1 = store
        .put(
            &bot_id,
            &plugin_name,
            ConfigBackend::Toml,
            "threshold = 1",
            None,
        )
        .await
        .unwrap();
    assert_eq!(v1, 1);

    let err = store
        .put(
            &bot_id,
            &plugin_name,
            ConfigBackend::Toml,
            "threshold = 2",
            Some(v1 + 1),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("version conflict"));

    let v2 = store
        .put(
            &bot_id,
            &plugin_name,
            ConfigBackend::Toml,
            "threshold = 2",
            Some(v1),
        )
        .await
        .unwrap();
    assert_eq!(v2, 2);

    let loaded = store.get(&bot_id, &plugin_name).await.unwrap().unwrap();
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

#[cfg(feature = "sqlite-backend")]
#[tokio::test]
async fn sqlite_backend_persists_across_store_instances() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("config.db");
    let db_url = format!("sqlite://{}", db_path.display());

    let store1 = SqliteConfigStore::new(db_url.clone());
    store1
        .put(
            "bot-persist",
            "echo",
            ConfigBackend::Toml,
            "threshold = 7",
            None,
        )
        .await
        .unwrap();

    let store2 = SqliteConfigStore::new(db_url);
    let loaded = store2.get("bot-persist", "echo").await.unwrap().unwrap();
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.content, "threshold = 7");
}

#[cfg(feature = "postgres-backend")]
#[tokio::test]
#[ignore = "requires AYIOU_TEST_POSTGRES_DSN"]
async fn postgres_backend_passes_config_store_contract() {
    let dsn = std::env::var("AYIOU_TEST_POSTGRES_DSN")
        .expect("set AYIOU_TEST_POSTGRES_DSN to run postgres contract test");
    let store: Arc<dyn ConfigStore> = Arc::new(PostgresConfigStore::new(dsn));
    run_config_store_contract(store).await;
}

#[cfg(feature = "redis-backend")]
#[tokio::test]
#[ignore = "requires AYIOU_TEST_REDIS_URL"]
async fn redis_backend_passes_config_store_contract() {
    let endpoint = std::env::var("AYIOU_TEST_REDIS_URL")
        .expect("set AYIOU_TEST_REDIS_URL to run redis contract test");
    let namespace = format!("ayiou:test:{}", Uuid::new_v4().simple());
    let store: Arc<dyn ConfigStore> = Arc::new(
        RedisConfigStore::new(endpoint)
            .unwrap()
            .with_namespace(namespace),
    );
    run_config_store_contract(store).await;
}
