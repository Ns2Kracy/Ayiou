# Ayiou

Ayiou is a Rust bot framework focused on a single Bot runtime, adapter-based message sources, and a unified plugin model.

The public authoring path is `#[plugin]`: async methods in the impl become commands and are auto-discovered by the bot at startup. Use `#[command]` when a method needs a custom command name or aliases, and use `RuntimePlugin` for advanced lifecycle, capability negotiation, or custom handler declarations.

Built-in drivers and adapters are opt-in Cargo features. The default dependency exposes the core runtime, plugin APIs, macros, and shared traits only.

```toml
ayiou = { git = "https://github.com/Ns2Kracy/Ayiou.git", features = ["adapter-onebot-v11"] }
```

For local console development:

```toml
ayiou = { git = "https://github.com/Ns2Kracy/Ayiou.git", features = ["adapter-console"] }
```

For the embedded control plane and management UI:

```toml
ayiou = { git = "https://github.com/Ns2Kracy/Ayiou.git", features = ["adapter-console", "embedded-webui"] }
```

```rust
use ayiou::{ConsoleBot, ControlPlaneOptions};

ConsoleBot::console()
    .control_plane(ControlPlaneOptions::new().token("change-me"))
    .run()
    .await;
```

The control plane bind address is optional and defaults to `127.0.0.1:32187`; the token is required. Compiled Rust plugins support lifecycle control through the UI, while code hot reload is reserved for future Rhai/WASM plugin backends.

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
async fn init(&mut self, services: RuntimePluginServices) -> anyhow::Result<()> {
    self.acl = Some(services.require_service::<AclService>()?);
    Ok(())
}
```

Required service declarations are preflighted before any plugin `init()` runs, so a missing dependency fails startup without partially initializing the plugin set.

For diagnostics or adaptive behavior, plugins can inspect registered service metadata through `services.service_descriptors()`. Descriptors include the service type key, `RuntimeService::name()`, and `RuntimeService::version()`.

## Release

Releases are published by `.github/workflows/release.yml` when a version tag is pushed:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The tag version must match both workspace package versions. The workflow publishes `ayiou-macros` first, waits until it is visible in the crates.io index, then publishes `ayiou`.

For the first crates.io release, add a repository secret named `CARGO_REGISTRY_TOKEN` with a crates.io API token. After the first release, configure crates.io Trusted Publishing for both `ayiou-macros` and `ayiou` and remove the long-lived token if desired:

- Repository: `Ns2Kracy/Ayiou`
- Workflow: `.github/workflows/release.yml`
- Environment: `crates-io`

When switching to Trusted Publishing, also set the GitHub Actions repository or `crates-io`
environment variable `CRATES_IO_TRUSTED_PUBLISHING=true`. Without that opt-in, the workflow uses
`CARGO_REGISTRY_TOKEN` and will not request a trusted-publishing token.
