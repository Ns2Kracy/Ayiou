//! External process management and communication

use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Result};
use ayiou::adapter::onebot::v11::ctx::Ctx;
use ayiou_protocol::{
    methods, HandleParams, HandleResult, LifecycleEvent, LifecycleParams,
    LifecycleResult, MatchesParams, MatchesResult, RpcRequest, RpcResponse,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, warn};

/// RPC call timeout duration
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

use crate::config::PluginConfig;

/// An external plugin process
pub struct ExternalProcess {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
    next_id: Mutex<u64>,
}

impl ExternalProcess {
    /// Spawn a new external process
    pub async fn spawn(config: &PluginConfig) -> Result<Self> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        if let Some(ref cwd) = config.cwd {
            cmd.current_dir(cwd);
        }

        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to capture stdin"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to capture stdout"))?;

        Ok(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
            next_id: Mutex::new(1),
        })
    }

    /// Send a JSON-RPC request and get response
    async fn call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let id = {
            let mut next_id = self.next_id.lock().await;
            let id = *next_id;
            *next_id += 1;
            id
        };

        let request = RpcRequest::new(method, params, id);
        let request_json = serde_json::to_string(&request)?;

        debug!("Sending RPC: {}", request_json);

        // Write request
        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(request_json.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
        }

        // Read response with timeout
        let response_json = {
            let mut stdout = self.stdout.lock().await;
            let mut line = String::new();
            timeout(RPC_TIMEOUT, stdout.read_line(&mut line))
                .await
                .map_err(|_| anyhow!("RPC call '{}' timed out after {:?}", method, RPC_TIMEOUT))??;
            line
        };

        debug!("Received RPC: {}", response_json.trim());

        let response: RpcResponse = serde_json::from_str(&response_json)?;

        if response.id != id {
            return Err(anyhow!("Response ID mismatch"));
        }

        if let Some(error) = response.error {
            return Err(anyhow!("RPC error {}: {}", error.code, error.message));
        }

        response
            .result
            .ok_or_else(|| anyhow!("No result in response"))
    }

    /// Check if the plugin matches a message
    pub async fn call_matches(&self, ctx: &Ctx) -> Result<bool> {
        let params = MatchesParams {
            text: ctx.text(),
            message_type: if ctx.is_group() { "group" } else { "private" }.to_string(),
            user_id: Some(ctx.user_id()),
            group_id: ctx.group_id(),
        };

        let result = self
            .call(methods::MATCHES, serde_json::to_value(&params)?)
            .await?;

        let matches_result: MatchesResult = serde_json::from_value(result)?;
        Ok(matches_result.matches)
    }

    pub async fn call_handle(&self, ctx: &Ctx) -> Result<HandleResult> {
        let params = HandleParams {
            message_type: if ctx.is_group() { "group" } else { "private" }.to_string(),
            user_id: ctx.user_id(),
            group_id: ctx.group_id(),
            text: ctx.text(),
            raw_message: ctx.raw_message().to_string(),
            self_id: None,
        };

        let result = self
            .call(methods::HANDLE, serde_json::to_value(&params)?)
            .await?;

        let handle_result: HandleResult = serde_json::from_value(result)?;
        Ok(handle_result)
    }

    /// Send startup lifecycle event
    pub async fn send_lifecycle_startup(&self) -> Result<()> {
        let params = LifecycleParams {
            event: LifecycleEvent::Startup,
        };

        let result = self
            .call(methods::LIFECYCLE, serde_json::to_value(&params)?)
            .await?;

        let _: LifecycleResult = serde_json::from_value(result)?;
        Ok(())
    }

    /// Send shutdown lifecycle event
    pub async fn send_lifecycle_shutdown(&self) -> Result<()> {
        let params = LifecycleParams {
            event: LifecycleEvent::Shutdown,
        };

        match self
            .call(methods::LIFECYCLE, serde_json::to_value(&params)?)
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                warn!("Shutdown event failed (process may be dead): {}", e);
                Ok(())
            }
        }
    }

    /// Kill the process
    pub async fn kill(self) -> Result<()> {
        let mut child = self.child.lock().await;
        child.kill().await?;
        Ok(())
    }
}
