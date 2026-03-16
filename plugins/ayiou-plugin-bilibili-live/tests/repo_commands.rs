use std::sync::Arc;

use ayiou::core::storage::MemoryStore;
use ayiou_plugin_bilibili_live::repo::SubscriptionRepo;

#[tokio::test]
async fn sub_unsub_and_list_are_scoped_per_target() {
    let store = Arc::new(MemoryStore::new());
    let repo = SubscriptionRepo::new(store);

    repo.subscribe_group(100, 42).await.unwrap();
    repo.subscribe_group(100, 99).await.unwrap();
    repo.subscribe_private(7, 42).await.unwrap();

    assert_eq!(repo.list_group(100).await.unwrap(), vec![42, 99]);
    assert_eq!(repo.list_private(7).await.unwrap(), vec![42]);

    repo.unsubscribe_group(100, 99).await.unwrap();
    assert_eq!(repo.list_group(100).await.unwrap(), vec![42]);
}
