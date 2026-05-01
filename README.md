# Ayiou

Ayiou is a Rust bot framework focused on a single Bot runtime, adapter-based message sources, and a unified plugin model.

The public authoring path is `#[plugin]` plus `#[command]` for command plugins, with `RuntimePlugin` available for plugins that need lifecycle, capability negotiation, or custom handler declarations.
