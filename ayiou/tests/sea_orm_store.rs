use ayiou::core::storage::Store;
use ayiou::core::storage::StoreSerdeExt;
use ayiou::core::storage::sea_orm::SeaOrmStore;

#[tokio::test]
async fn sea_orm_store_roundtrip_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("kv.sqlite").display()
    );
    let store = SeaOrmStore::connect(&url).await.unwrap();

    store
        .set_json("plugin:test", &vec![1_u64, 2, 3])
        .await
        .unwrap();
    let loaded: Option<Vec<u64>> = store.get_json("plugin:test").await.unwrap();
    assert_eq!(loaded, Some(vec![1, 2, 3]));

    assert!(store.delete("plugin:test").await.unwrap());
    let missing: Option<Vec<u64>> = store.get_json("plugin:test").await.unwrap();
    assert_eq!(missing, None);
}

#[tokio::test]
async fn sea_orm_store_lists_keys_by_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("kv.sqlite").display()
    );
    let store = SeaOrmStore::connect(&url).await.unwrap();

    store.set_raw("plugin:a:1", b"1".to_vec()).await.unwrap();
    store.set_raw("plugin:a:2", b"2".to_vec()).await.unwrap();
    store.set_raw("plugin:b:1", b"3".to_vec()).await.unwrap();

    let keys = store.list_prefix("plugin:a:").await.unwrap();
    assert_eq!(
        keys,
        vec!["plugin:a:1".to_string(), "plugin:a:2".to_string()]
    );
}
