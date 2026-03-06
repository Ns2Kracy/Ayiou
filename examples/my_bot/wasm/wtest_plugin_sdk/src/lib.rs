use ayiou_wasm_sdk::{Plugin, ReplyAction, export_plugin};

struct WtestPlugin;

impl Plugin for WtestPlugin {
    fn on_command(command: &str, args: &str) -> Option<ReplyAction> {
        if command != "wtest" {
            return None;
        }

        let text = if args.is_empty() {
            "[wasm] command matched".to_string()
        } else {
            format!("[wasm] command matched: {}", args)
        };

        Some(ReplyAction::text(text))
    }

    fn on_regex(text: &str) -> Option<ReplyAction> {
        text.starts_with("wasm-demo://")
            .then(|| ReplyAction::text(format!("[wasm] regex matched: {}", text)))
    }

    fn on_cron(expr: &str) -> Option<ReplyAction> {
        (!expr.is_empty()).then(|| ReplyAction::text("[wasm] cron tick"))
    }
}

export_plugin!(WtestPlugin);
