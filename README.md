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
