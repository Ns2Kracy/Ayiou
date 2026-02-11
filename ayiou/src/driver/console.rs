use anyhow::Result;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use crate::core::driver::Driver;

/// Console driver.
///
/// Inbound packets are lines read from stdin.
/// Outbound packets are printed to stdout, one line per packet.
pub struct ConsoleDriver;

impl ConsoleDriver {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ConsoleDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for ConsoleDriver {
    type Inbound = String;
    type Outbound = String;

    async fn run(
        self: Box<Self>,
        inbound_tx: mpsc::Sender<Self::Inbound>,
        mut outbound_rx: mpsc::Receiver<Self::Outbound>,
    ) -> Result<()> {
        let stdin = io::stdin();
        let mut lines = BufReader::new(stdin).lines();
        let mut stdout = io::stdout();

        loop {
            tokio::select! {
                line = lines.next_line() => {
                    match line? {
                        Some(line) => {
                            if inbound_tx.send(line).await.is_err() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                msg = outbound_rx.recv() => {
                    match msg {
                        Some(msg) => {
                            stdout.write_all(msg.as_bytes()).await?;
                            stdout.write_all(b"\n").await?;
                            stdout.flush().await?;
                        }
                        None => break,
                    }
                }
            }
        }

        Ok(())
    }
}
