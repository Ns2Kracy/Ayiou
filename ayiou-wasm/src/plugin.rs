use anyhow::{Result, anyhow};
use async_trait::async_trait;
use ayiou::core::{
    context::Context,
    model::CommandInvocation,
    plugin::{
        ApplyConfigOutcome, ConfigUpdate, HandleOutcome, HandlerDecl, PluginHealth, RuntimePlugin,
        RuntimePluginManifest, RuntimePluginServices,
    },
};
use tokio::sync::Mutex;

use crate::types::{
    WasmHandleOutcomeDto, WasmHandlerDto, WasmHealthDto, WasmManifestDto, WasmPluginPackageDto,
};

#[derive(Debug, Clone)]
pub enum WasmGuestCall {
    Static {
        handle_outcome: WasmHandleOutcomeDto,
        health: WasmHealthDto,
    },
}

pub struct WasmRuntimePlugin {
    instance_id: String,
    manifest: RuntimePluginManifest,
    handlers: Vec<HandlerDecl>,
    guest: Mutex<WasmGuestCall>,
}

impl WasmRuntimePlugin {
    pub fn new(
        instance_id: impl Into<String>,
        manifest: RuntimePluginManifest,
        handlers: Vec<HandlerDecl>,
    ) -> Self {
        Self::with_guest(
            instance_id,
            manifest,
            handlers,
            WasmGuestCall::Static {
                handle_outcome: WasmHandleOutcomeDto::default(),
                health: WasmHealthDto::default(),
            },
        )
    }

    pub fn with_guest(
        instance_id: impl Into<String>,
        manifest: RuntimePluginManifest,
        handlers: Vec<HandlerDecl>,
        guest: WasmGuestCall,
    ) -> Self {
        Self {
            instance_id: instance_id.into(),
            manifest,
            handlers,
            guest: Mutex::new(guest),
        }
    }

    pub fn from_package(
        instance_id: impl Into<String>,
        package: WasmPluginPackageDto,
    ) -> Result<Self> {
        Self::from_parts(
            instance_id,
            package.manifest,
            package.handlers,
            package.handle_outcome,
            package.health,
        )
    }

    pub fn from_dtos(
        instance_id: impl Into<String>,
        manifest: WasmManifestDto,
        handlers: Vec<WasmHandlerDto>,
        handle_outcome: WasmHandleOutcomeDto,
    ) -> Result<Self> {
        Self::from_parts(
            instance_id,
            manifest,
            handlers,
            handle_outcome,
            WasmHealthDto::default(),
        )
    }

    pub fn from_parts(
        instance_id: impl Into<String>,
        manifest: WasmManifestDto,
        handlers: Vec<WasmHandlerDto>,
        handle_outcome: WasmHandleOutcomeDto,
        health: WasmHealthDto,
    ) -> Result<Self> {
        let mut runtime_manifest = RuntimePluginManifest::new(manifest.kind);
        if let Some(version) = manifest.version {
            runtime_manifest = runtime_manifest.version(version);
        }
        if let Some(description) = manifest.description {
            runtime_manifest = runtime_manifest.description(description);
        }
        let handlers = handlers
            .into_iter()
            .map(handler_from_dto)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self::with_guest(
            instance_id,
            runtime_manifest,
            handlers,
            WasmGuestCall::Static {
                handle_outcome,
                health,
            },
        ))
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
}

fn handler_from_dto(dto: WasmHandlerDto) -> Result<HandlerDecl> {
    let mut handler = if !dto.commands.is_empty() {
        HandlerDecl::message_commands(dto.commands, std::iter::empty::<String>())
    } else if !dto.regex_patterns.is_empty() {
        HandlerDecl::message_regex(dto.regex_patterns)
    } else if dto.wildcard {
        HandlerDecl::wildcard_message()
    } else {
        return Err(anyhow!(
            "wasm handler must declare commands, regex_patterns, or wildcard=true"
        ));
    };
    handler.priority = dto.priority;
    handler.block = dto.block;
    Ok(handler)
}

#[async_trait]
impl RuntimePlugin for WasmRuntimePlugin {
    fn kind(&self) -> &str {
        self.manifest.kind.as_str()
    }

    fn manifest(&self) -> RuntimePluginManifest {
        self.manifest.clone()
    }

    fn declared_handlers(&self) -> Vec<HandlerDecl> {
        self.handlers.clone()
    }

    async fn init(&mut self, _services: RuntimePluginServices) -> Result<()> {
        Ok(())
    }

    async fn start(&mut self, _services: RuntimePluginServices) -> Result<()> {
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    async fn apply_config(&mut self, update: ConfigUpdate) -> Result<ApplyConfigOutcome> {
        Ok(ApplyConfigOutcome::applied(update.version))
    }

    async fn handle(&self, ctx: &Context) -> Result<HandleOutcome> {
        self.handle_with_invocation(ctx, None).await
    }

    async fn handle_with_invocation(
        &self,
        _ctx: &Context,
        _invocation: Option<CommandInvocation>,
    ) -> Result<HandleOutcome> {
        let guest = self.guest.lock().await;
        let outcome = match &*guest {
            WasmGuestCall::Static { handle_outcome, .. } if handle_outcome.block => {
                HandleOutcome::block()
            }
            WasmGuestCall::Static { .. } => HandleOutcome::pass(),
        };
        Ok(outcome)
    }

    fn health(&self) -> PluginHealth {
        match self.guest.try_lock() {
            Ok(guest) => match &*guest {
                WasmGuestCall::Static { health, .. } => PluginHealth {
                    healthy: health.healthy,
                    detail: health.detail.clone(),
                },
            },
            Err(_) => PluginHealth {
                healthy: false,
                detail: Some("wasm plugin health unavailable while guest is busy".to_string()),
            },
        }
    }
}
