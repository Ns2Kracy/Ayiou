use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ConnectOptions, Database, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::core::storage::Store;

#[derive(Clone)]
pub struct SeaOrmStore {
    db: DatabaseConnection,
}

impl SeaOrmStore {
    pub async fn connect(url: &str) -> Result<Self> {
        let mut options = ConnectOptions::new(url.to_string());
        options.sqlx_logging(false);

        let db = Database::connect(options).await?;
        db.get_schema_builder().register(Entity).sync(&db).await?;

        Ok(Self { db })
    }

    pub fn db(&self) -> &DatabaseConnection {
        &self.db
    }
}

#[async_trait]
impl Store for SeaOrmStore {
    async fn get_raw(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(Entity::find_by_id(key.to_string())
            .one(&self.db)
            .await?
            .map(|record| record.value))
    }

    async fn set_raw(&self, key: &str, value: Vec<u8>) -> Result<()> {
        let now = Utc::now().timestamp_millis();

        if let Some(model) = Entity::find_by_id(key.to_string()).one(&self.db).await? {
            let mut active: ActiveModel = model.into();
            active.value = Set(value);
            active.updated_at_ms = Set(now);
            active.update(&self.db).await?;
        } else {
            ActiveModel {
                key: Set(key.to_string()),
                value: Set(value),
                created_at_ms: Set(now),
                updated_at_ms: Set(now),
            }
            .insert(&self.db)
            .await?;
        }

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<bool> {
        let result = Entity::delete_by_id(key.to_string()).exec(&self.db).await?;
        Ok(result.rows_affected > 0)
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        let mut keys = Entity::find()
            .filter(Column::Key.starts_with(prefix))
            .all(&self.db)
            .await?
            .into_iter()
            .map(|record| record.key)
            .collect::<Vec<_>>();
        keys.sort();
        Ok(keys)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "kv_records")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub key: String,
    pub value: Vec<u8>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
