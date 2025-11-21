use crate::core::{Adapter, Driver};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt};
use tracing::warn;

#[derive(Debug)]
pub struct ConsoleDriver;

#[async_trait]
impl Driver for ConsoleDriver {
    async fn run(&self, adapter: Arc<dyn Adapter>) -> Result<()> {
        println!("Console Driver Started. Type something...");
        let stdin = io::stdin();
        let mut reader = io::BufReader::new(stdin).lines();

        while let Ok(Some(line)) = reader.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            if let Err(e) = adapter.handle(line).await {
                warn!("Adapter failed to handle event: {}", e);
            }
        }
        Ok(())
    }

    async fn send(&self, content: String) -> Result<()> {
        println!("\x1b[36m[ConsoleDriver Output]\x1b[0m {}", content);
        Ok(())
    }
}
