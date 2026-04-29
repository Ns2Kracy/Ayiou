use std::sync::Arc;

use ayiou::adapter::console::adapter::ConsoleAdapter;
use ayiou::core::storage::{MemoryStore, Store};
use ayiou::{Bot, ConsoleBot};

#[test]
fn bot_uses_memory_store_by_default() {
    let bot = ConsoleBot::console();
    let _store: Arc<dyn Store> = bot.store();
}

#[test]
fn bot_accepts_custom_store() {
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let bot = Bot::new(ConsoleAdapter::new()).with_store(store.clone());
    let loaded = bot.store();
    assert!(Arc::ptr_eq(&loaded, &store));
}
