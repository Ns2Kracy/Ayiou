use std::{fs, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use ayiou::core::{
    context::Context,
    model::{BotId, ChannelRef, EventEnvelope, MessageEvent, PlatformId, UserRef},
    plugin::{
        MemoryConversationStore, Permission, PermissionDecision, PermissionService,
        PluginRuntimeState, RuntimePluginEngine, RuntimePluginServices,
    },
    service::RuntimeService,
};
use ayiou_wasm::{
    WasmPluginBackend, WasmPluginSource, WasmRuntimePlugin,
    host::WasmHostState,
    types::{WasmHandleOutcomeDto, WasmHandlerDto, WasmManifestDto, WasmPluginPackageDto},
};

fn test_ctx(text: &str) -> Context {
    let platform = PlatformId::new("test");
    let user = UserRef::new(platform.clone(), "user");
    let channel = ChannelRef::group(platform.clone(), "group");
    let message = MessageEvent::new(user, channel, text.to_string());
    Context::new(
        EventEnvelope::new(BotId::new("bot"), platform).with_message(message),
        None,
        (),
    )
}

#[test]
fn wasm_backend_constructs_with_resource_limits_enabled() {
    let backend = WasmPluginBackend::new().expect("backend should construct");
    let _engine = backend.engine();
}

#[tokio::test]
async fn wasm_runtime_plugin_dispatches_declared_command() -> Result<()> {
    let plugin = WasmRuntimePlugin::from_dtos(
        "fixture.instance",
        WasmManifestDto {
            kind: "fixture-wasm".to_string(),
            version: Some("0.1.0".to_string()),
            description: None,
        },
        vec![WasmHandlerDto {
            commands: vec!["ping".to_string()],
            regex_patterns: Vec::new(),
            wildcard: false,
            priority: 0,
            block: true,
        }],
        WasmHandleOutcomeDto { block: true },
    )?;
    let mut engine =
        RuntimePluginEngine::new(RuntimePluginServices::new(), PluginRuntimeState::default());
    engine.push_as("fixture.instance", Box::new(plugin));
    engine.init_all().await?;
    engine.start_all().await?;

    let handled = engine.handle_all(&test_ctx("ping")).await?;

    assert!(handled);
    Ok(())
}

#[tokio::test]
async fn wasm_backend_loads_package_and_dispatches() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let artifact_path = dir.path().join("fixture.wasm");
    fs::write(&artifact_path, wat::parse_str("(component)")?)?;
    fs::write(
        dir.path().join("ayiou-plugin.json"),
        serde_json::to_vec(
            &WasmPluginPackageDto::new("fixture-wasm")
                .version("0.1.0")
                .handler(WasmHandlerDto::command("ping").block(true))
                .block(true),
        )?,
    )?;

    let backend = WasmPluginBackend::new()?;
    let plugin = backend
        .load_plugin(WasmPluginSource::new("fixture.instance", artifact_path))
        .await?;
    let mut engine =
        RuntimePluginEngine::new(RuntimePluginServices::new(), PluginRuntimeState::default());
    engine.push_as("fixture.instance", Box::new(plugin));
    engine.init_all().await?;
    engine.start_all().await?;

    assert!(engine.handle_all(&test_ctx("ping")).await?);
    Ok(())
}

struct AllowAdmin;

#[tokio::test]
async fn wasm_backend_loads_registered_plugin_from_managed_package() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let artifact_path = dir.path().join("fixture.wasm");
    fs::write(&artifact_path, wat::parse_str("(component)")?)?;
    fs::write(
        dir.path().join("ayiou-plugin.json"),
        serde_json::to_vec(
            &WasmPluginPackageDto::new("managed-wasm")
                .version("0.1.0")
                .description("managed by ayiou")
                .handler(WasmHandlerDto::command("managed").block(true))
                .block(true),
        )?,
    )?;

    let backend = WasmPluginBackend::new()?;
    let registered = backend
        .load_registered(WasmPluginSource::new("managed.instance", artifact_path))
        .await?;
    assert!(registered.reload_descriptor().is_reloadable());
    Ok(())
}

impl RuntimeService for AllowAdmin {
    fn name(&self) -> &'static str {
        "allow-admin"
    }
}

#[async_trait]
impl PermissionService for AllowAdmin {
    async fn check(&self, _ctx: &Context, permission: &Permission) -> Result<PermissionDecision> {
        Ok(match permission {
            Permission::Custom(name) if name == "admin" => PermissionDecision::Allow,
            _ => PermissionDecision::Deny("denied".to_string()),
        })
    }
}

#[tokio::test]
async fn wasm_host_import_facade_enforces_permissions_and_conversation_scope() -> Result<()> {
    let permission_service: Arc<dyn PermissionService> = Arc::new(AllowAdmin);
    let store = Arc::new(MemoryConversationStore::default());
    let host = WasmHostState::new(
        "plugin-a",
        RuntimePluginServices::new().with_permission_service(Some(permission_service)),
    )
    .with_context(test_ctx("hello"));

    host.host_log("info", "fixture log");
    assert!(host.permission_check("custom", "admin").await?);
    assert!(!host.permission_check("custom", "guest").await?);

    host.conversation_put(store.clone(), "state", "ready".to_string(), None)
        .await?;
    assert_eq!(
        host.conversation_get(store.clone(), "state").await?,
        Some("ready".to_string())
    );
    host.conversation_remove(store.clone(), "state").await?;
    assert_eq!(host.conversation_get(store, "state").await?, None);
    Ok(())
}
