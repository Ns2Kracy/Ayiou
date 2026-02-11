use anyhow::Result;
use tokio::sync::mpsc;

use crate::core::adapter::MsgContext;

const DEFAULT_COMMAND_PREFIXES: [&str; 3] = ["/", "!", "."];

#[derive(Clone)]
pub struct Ctx {
    line: String,
    outgoing_tx: mpsc::Sender<String>,
}

impl Ctx {
    pub fn new(line: String, outgoing_tx: mpsc::Sender<String>) -> Self {
        Self { line, outgoing_tx }
    }

    pub fn line(&self) -> &str {
        &self.line
    }

    pub async fn reply_text(&self, text: impl Into<String>) -> Result<()> {
        self.outgoing_tx.send(text.into()).await?;
        Ok(())
    }

    pub fn command_name(&self) -> Option<String> {
        self.command_name_with_prefixes(&DEFAULT_COMMAND_PREFIXES)
    }

    pub fn command_args(&self) -> Option<String> {
        self.command_args_with_prefixes(&DEFAULT_COMMAND_PREFIXES)
    }

    pub fn command_name_with_prefixes(&self, prefixes: &[&str]) -> Option<String> {
        let trimmed = self.line.trim_start();
        if trimmed.is_empty() {
            return None;
        }

        let token_end = trimmed
            .char_indices()
            .find(|(_, ch)| ch.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(trimmed.len());

        let token = &trimmed[..token_end];

        for prefix in prefixes {
            if let Some(cmd) = token.strip_prefix(prefix)
                && !cmd.is_empty()
            {
                return Some(cmd.to_string());
            }
        }

        None
    }

    pub fn command_args_with_prefixes(&self, prefixes: &[&str]) -> Option<String> {
        let trimmed = self.line.trim_start();
        if trimmed.is_empty() {
            return None;
        }

        let token_end = trimmed
            .char_indices()
            .find(|(_, ch)| ch.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(trimmed.len());

        let token = &trimmed[..token_end];

        for prefix in prefixes {
            if token.strip_prefix(prefix).is_some() {
                let args = trimmed[token_end..].trim_start();
                return Some(args.to_string());
            }
        }

        None
    }
}

impl MsgContext for Ctx {
    fn text(&self) -> String {
        self.line.trim().to_string()
    }

    fn user_id(&self) -> String {
        "console-user".to_string()
    }

    fn group_id(&self) -> Option<String> {
        None
    }
}
