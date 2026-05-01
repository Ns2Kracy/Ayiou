use std::sync::Arc;

use crate::core::model::CommandInvocation;

// ============================================================================
// Args parsing types
// ============================================================================

/// Error returned when argument parsing fails
#[derive(Debug, Clone)]
pub struct ArgsParseError {
    message: String,
    help: Option<String>,
}

impl ArgsParseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            help: None,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn help(&self) -> Option<&str> {
        self.help.as_deref()
    }
}

impl std::fmt::Display for ArgsParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ArgsParseError {}

/// Parse command line from text.
///
/// `prefixes` will be stripped from the first token when matched.
pub fn parse_command_line(text: &str, prefixes: &[&str]) -> Option<CommandInvocation> {
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return None;
    }

    let token_end = trimmed
        .char_indices()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(idx, _)| idx)
        .unwrap_or(trimmed.len());

    let token = &trimmed[..token_end];
    let args = trimmed[token_end..].trim_start();

    let mut command = token;
    let mut matched_prefix = None;
    for prefix in prefixes {
        if let Some(stripped) = token.strip_prefix(prefix)
            && !stripped.is_empty()
        {
            command = stripped;
            matched_prefix = Some((*prefix).to_string());
            break;
        }
    }

    Some(CommandInvocation::new(command, args, matched_prefix))
}

/// Split command arguments into tokens.
///
/// Supports both single and double quotes, and `\\` escape.
pub fn tokenize_command_args(args: &str) -> std::result::Result<Vec<String>, ArgsParseError> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut chars = args.chars().peekable();
    let mut quote: Option<char> = None;

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                if let Some(next) = chars.next() {
                    buf.push(next);
                } else {
                    return Err(ArgsParseError::new("Trailing escape in arguments"));
                }
            }
            '\'' | '"' => {
                if let Some(active) = quote {
                    if active == ch {
                        quote = None;
                    } else {
                        buf.push(ch);
                    }
                } else {
                    quote = Some(ch);
                }
            }
            c if c.is_whitespace() && quote.is_none() => {
                if !buf.is_empty() {
                    out.push(std::mem::take(&mut buf));
                }
            }
            _ => buf.push(ch),
        }
    }

    if quote.is_some() {
        return Err(ArgsParseError::new("Unterminated quoted argument"));
    }

    if !buf.is_empty() {
        out.push(buf);
    }

    Ok(out)
}

pub fn parse_typed_arg<T>(
    tokens: &[String],
    index: &mut usize,
    name: &str,
) -> std::result::Result<T, ArgsParseError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let value = tokens
        .get(*index)
        .ok_or_else(|| ArgsParseError::new(format!("Missing argument: {}", name)))?;
    *index += 1;

    value
        .parse::<T>()
        .map_err(|err| ArgsParseError::new(format!("Failed to parse argument `{}`: {}", name, err)))
}

pub fn ensure_no_extra_args(
    tokens: &[String],
    index: usize,
) -> std::result::Result<(), ArgsParseError> {
    if let Some(extra) = tokens.get(index) {
        return Err(ArgsParseError::new(format!(
            "Unexpected extra argument: {}",
            extra
        )));
    }

    Ok(())
}

// ============================================================================
// Plugin metadata
// ============================================================================

#[derive(Clone, Debug)]
pub struct PluginMetadata {
    pub name: String,
    pub description: String,
    pub version: String,
}

impl PluginMetadata {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            version: "0.0.0".to_string(),
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }
}

impl Default for PluginMetadata {
    fn default() -> Self {
        Self {
            name: "unnamed".to_string(),
            description: String::new(),
            version: "0.0.0".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchOptions {
    command_prefixes: Arc<[String]>,
}

impl DispatchOptions {
    pub fn new(command_prefixes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let mut prefixes: Vec<String> = command_prefixes
            .into_iter()
            .map(Into::into)
            .filter(|p| !p.is_empty())
            .collect();
        prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
        prefixes.dedup();

        Self {
            command_prefixes: prefixes.into(),
        }
    }

    pub fn command_prefixes(&self) -> &[String] {
        self.command_prefixes.as_ref()
    }
}

impl Default for DispatchOptions {
    fn default() -> Self {
        Self {
            command_prefixes: Arc::from([]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_line_strips_prefix_and_extracts_args() {
        let line = parse_command_line("/echo hello world", &["/", "!"]).unwrap();
        assert_eq!(line.command(), "echo");
        assert_eq!(line.args(), "hello world");
    }

    #[test]
    fn tokenize_command_args_supports_quotes_and_escape() {
        let tokens =
            tokenize_command_args("\"hello world\" 'x y' z\\ z").expect("tokenize should succeed");
        assert_eq!(tokens, vec!["hello world", "x y", "z z"]);
    }
}
