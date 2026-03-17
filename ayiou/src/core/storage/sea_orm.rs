use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use sea_orm::entity::prelude::*;
use sea_orm::{
    ActiveModelTrait, ConnectOptions, Database, DatabaseConnection, EntityTrait, QueryFilter, Set,
};

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

        Ok(Self { db })
    }

    pub fn db(&self) -> &DatabaseConnection {
        &self.db
    }

    pub async fn register_entity(&self, entity: Vec<impl EntityTrait>) -> Result<()> {
        for e in entity {
            self.db
                .get_schema_builder()
                .register(e)
                .sync(&self.db)
                .await?;
        }
        Ok(())
    }
}
