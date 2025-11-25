use crate::core::Driver;
use anyhow::Result;
use std::sync::Arc;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, stdin, stdout},
    sync::{Mutex, mpsc},
};
use tracing::info;

pub struct ConsoleDriver {
    output: Arc<Mutex<tokio::io::Stdout>>,
}

impl ConsoleDriver {
    pub fn new() -> Self {
        Self {
            output: Arc::new(Mutex::new(stdout())),
        }
    }
}

impl Default for ConsoleDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for ConsoleDriver {
    async fn run(&self, tx: mpsc::Sender<String>) -> Result<()> {
        info!("Console driver started");
        let stdin = stdin();
        let mut reader = BufReader::new(stdin).lines();
        while let Some(line) = reader.next_line().await? {
            tx.send(line).await?;
        }
        Ok(())
    }

    async fn send(&self, message: String) -> Result<()> {
        let mut out = self.output.lock().await;
        out.write_all(format!("{}\n", message).as_bytes()).await?;
        out.flush().await?;
        Ok(())
    }
}
