use anyhow::Result;
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt};
use tracing::{info, warn};

use crate::{
    core::{adapter::Adapter, context::Context, driver::Driver},
    driver::AdapterBuilder,
};

#[derive(Clone)]
pub struct ConsoleDriver {
    message: String,
    output_format: String,
    adapter_builder: Option<AdapterBuilder>,
}

impl ConsoleDriver {
    pub fn new(
        message: Option<impl Into<String>>,
        output_format: Option<impl Into<String>>,
    ) -> Self {
        Self {
            message: message.map_or_else(|| "".to_string(), |s| s.into()),
            output_format: output_format.map_or_else(
                || "\x1b[36m[ConsoleDriver Output]\x1b[0m {}".to_string(),
                |s| s.into(),
            ),
            adapter_builder: None,
        }
    }

    pub fn register_adapter<A, F>(mut self, builder: F) -> Self
    where
        A: Adapter + 'static + Clone,
        F: Fn(Context) -> A + Send + Sync + 'static,
    {
        self.adapter_builder = Some(Arc::new(move |ctx| Box::new(builder(ctx))));
        self
    }
}

impl Default for ConsoleDriver {
    fn default() -> Self {
        info!("Console Driver Started. Type something...");
        Self::new(None::<String>, None::<String>)
    }
}

#[async_trait::async_trait]
impl Driver for ConsoleDriver {
    fn create_adapter(&self, ctx: Context) -> Option<Box<dyn Adapter>> {
        self.adapter_builder.as_ref().map(|builder| builder(ctx))
    }

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
