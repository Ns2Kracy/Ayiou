use std::sync::Arc;

use anyhow::{Result, anyhow};
use ayiou::core::{
    context::Context,
    model::OutboundMessage,
    plugin::{
        ConversationKey, ConversationStore, Permission, PermissionDecision, RuntimePluginServices,
    },
};

#[derive(Clone)]
pub struct WasmHostState {
    pub instance_id: String,
    pub services: RuntimePluginServices,
    pub context: Option<Context>,
}

impl WasmHostState {
    pub fn new(instance_id: impl Into<String>, services: RuntimePluginServices) -> Self {
        Self {
            instance_id: instance_id.into(),
            services,
            context: None,
        }
    }

    pub fn with_context(&self, context: Context) -> Self {
        Self {
            instance_id: self.instance_id.clone(),
            services: self.services.clone(),
            context: Some(context),
        }
    }

    pub fn host_log(&self, level: &str, message: &str) {
        match level {
            "error" => log::error!(target: "ayiou_wasm", "{}", message),
            "warn" => log::warn!(target: "ayiou_wasm", "{}", message),
            "debug" => log::debug!(target: "ayiou_wasm", "{}", message),
            "trace" => log::trace!(target: "ayiou_wasm", "{}", message),
            _ => log::info!(target: "ayiou_wasm", "{}", message),
        }
    }

    pub async fn send_message(&self, target: &str, text: &str) -> Result<String> {
        if target != "source" {
            return Err(anyhow!("unsupported outbound target `{target}`"));
        }
        let context = self
            .context
            .as_ref()
            .ok_or_else(|| anyhow!("send-message requires message context"))?;
        let channel = context
            .message()
            .ok_or_else(|| anyhow!("send-message requires message context"))?
            .channel
            .clone();
        let receipt = self
            .services
            .send(OutboundMessage::text(channel, text.to_string()))
            .await?;
        Ok(receipt.message_id.unwrap_or_default())
    }

    pub async fn permission_check(&self, kind: &str, value: &str) -> Result<bool> {
        let permission = match kind {
            "role" => Permission::role(value),
            "custom" => Permission::custom(value),
            other => return Err(anyhow!("unsupported permission kind `{other}`")),
        };
        let Some(service) = self.services.permission_checker() else {
            return Ok(false);
        };
        Ok(matches!(
            service
                .check(
                    self.context
                        .as_ref()
                        .ok_or_else(|| anyhow!("permission check requires message context"))?,
                    &permission,
                )
                .await?,
            PermissionDecision::Allow
        ))
    }

    pub async fn conversation_get<S>(&self, store: Arc<S>, key: &str) -> Result<Option<String>>
    where
        S: ConversationStore,
    {
        Ok(store
            .get(&self.conversation_key(key)?)
            .await?
            .and_then(|value| value.as_str().map(ToString::to_string)))
    }

    pub async fn conversation_put<S>(
        &self,
        store: Arc<S>,
        key: &str,
        value: String,
        ttl_ms: Option<u64>,
    ) -> Result<()>
    where
        S: ConversationStore,
    {
        store
            .put(
                self.conversation_key(key)?,
                serde_json::Value::String(value),
                ttl_ms.map(std::time::Duration::from_millis),
            )
            .await
    }

    pub async fn conversation_remove<S>(&self, store: Arc<S>, key: &str) -> Result<()>
    where
        S: ConversationStore,
    {
        store.remove(&self.conversation_key(key)?).await
    }

    fn conversation_key(&self, guest_key: &str) -> Result<ConversationKey> {
        let context = self
            .context
            .as_ref()
            .ok_or_else(|| anyhow!("conversation access requires message context"))?;
        Ok(ConversationKey::from_context(
            format!("{}:{guest_key}", self.instance_id),
            context,
        ))
    }
}
