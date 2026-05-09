# Ayiou

Ayiou is a Rust bot framework focused on a single Bot runtime, adapter-based message sources, and a unified plugin model.

The public authoring path is `#[plugin]` plus `#[command]` for command plugins, with `RuntimePlugin` available for plugins that need lifecycle, capability negotiation, or custom handler declarations.

Built-in drivers and adapters are opt-in Cargo features. The default dependency exposes the core runtime, plugin APIs, macros, and shared traits only.

```toml
ayiou = { git = "https://github.com/Ns2Kracy/Ayiou.git", features = ["adapter-onebot-v11"] }
```

For local console development:

```toml
ayiou = { git = "https://github.com/Ns2Kracy/Ayiou.git", features = ["adapter-console"] }
```

## Runtime Services

Plugins should not call other plugin instances directly. Register host-provided services on `Bot`, then request them from `RuntimePluginServices` during plugin initialization:

```rust
let bot = Bot::new(adapter)
    .with_service(AclService::new())
    .with_plugin(AdminPlugin);
```

```rust
fn manifest(&self) -> RuntimePluginManifest {
    RuntimePluginManifest::new("admin")
        .require_service::<AclService>()
}
```

```rust
async fn init(&mut self, services: RuntimePluginServices<Context>) -> anyhow::Result<()> {
    self.acl = Some(services.require_service::<AclService>()?);
    Ok(())
}
```

Required service declarations are preflighted before any plugin `init()` runs, so a missing dependency fails startup without partially initializing the plugin set.

For diagnostics or adaptive behavior, plugins can inspect registered service metadata through `services.service_descriptors()`. Descriptors include the service type key, `RuntimeService::name()`, and `RuntimeService::version()`.
