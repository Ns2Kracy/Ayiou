use crate::core::driver::{Driver, DriverEvent};
use anyhow::Result;
use async_trait::async_trait;
use tokio::{
    io::{self, AsyncBufReadExt},
    sync::mpsc,
};

#[derive(Debug)]
pub struct ConsoleDriver;

#[async_trait]
impl Driver for ConsoleDriver {
    async fn start(&self, tx: mpsc::Sender<DriverEvent>) -> Result<()> {
        println!("Console Driver Started. Type something...");
        let stdin = io::stdin();
        let mut reader = io::BufReader::new(stdin).lines();

        // In a real driver, we might want to keep a handle to tx or store it in the struct
        // if we need to send things asynchronously from other places.
        // But here, we just loop stdin.

        while let Ok(Some(line)) = reader.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            if tx.send(DriverEvent::Message(line)).await.is_err() {
                break;
            }
        }
        Ok(())
    }

    async fn send(&self, content: String) -> Result<()> {
        println!("\x1b[36m[ConsoleDriver Output]\x1b[0m {}", content);
        Ok(())
    }
}
