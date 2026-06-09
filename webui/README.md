# Ayiou Web UI

Embedded React control-plane UI for Ayiou.

## Stack

- Runtime and package manager: Bun
- Dev/build toolchain: Vite+
- UI components: HeroUI
- Icons: Phosphor Icons
- Lint and format: Oxc through Vite+

## Commands

```bash
bun install
bun run dev
bun run build
bun run lint
bun run format
bun run check
```

Production assets are emitted to `dist/` and embedded by the Rust `embedded-webui` feature.
