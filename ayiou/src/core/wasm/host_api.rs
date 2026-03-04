use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use ayiou_wasm_sdk::HostCall;
use tokio::sync::Mutex;

#[async_trait]
pub trait WasmHostApi: Send + Sync + 'static {
    async fn on_call(&self, call: HostCall) -> Result<()>;
}

#[derive(Clone, Default)]
pub struct NoopWasmHost;

#[async_trait]
impl WasmHostApi for NoopWasmHost {
    async fn on_call(&self, _call: HostCall) -> Result<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct RecordingWasmHost {
    calls: Arc<Mutex<Vec<HostCall>>>,
}

impl RecordingWasmHost {
    pub async fn calls(&self) -> Vec<HostCall> {
        self.calls.lock().await.clone()
    }
}

#[async_trait]
impl WasmHostApi for RecordingWasmHost {
    async fn on_call(&self, call: HostCall) -> Result<()> {
        self.calls.lock().await.push(call);
        Ok(())
    }
}
