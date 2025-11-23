use crate::core::{adapter::Adapter, driver::Driver};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt};
use tracing::warn;

#[derive(Debug)]
pub struct ConsoleDriver {
    message: String,
    output_format: String,
}

impl ConsoleDriver {
    pub fn new(message: impl Into<String>, output_format: Option<impl Into<String>>) -> Self {
        Self {
            message: message.into(),
            output_format: output_format.map_or_else(
                || "\x1b[36m[ConsoleDriver Output]\x1b[0m {}".to_string(),
                |s| s.into(),
            ),
        }
    }
}

impl Default for ConsoleDriver {
    fn default() -> Self {
        Self::new("Console Driver Started. Type something...", None::<String>)
    }
}

#[async_trait]
impl Driver for ConsoleDriver {
    async fn run(&self, adapter: Arc<dyn Adapter>) -> Result<()> {
        println!("{}", self.message);
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
        println!("{}", self.output_format.replace("{}", &content));
        Ok(())
    }
}
