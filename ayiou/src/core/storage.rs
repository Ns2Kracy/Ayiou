// Placeholder for database traits
// Use traits so users can swap SQLite/Postgres/MySQL/Memory freely.
pub trait StorageDriver: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str);
}
