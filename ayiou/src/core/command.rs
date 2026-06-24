use crate::core::model::CommandInvocation;

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

    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
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

#[must_use]
pub fn parse_command_line(text: &str, prefixes: &[&str]) -> Option<CommandInvocation> {
    parse_command_line_with_prefixes(text, prefixes.iter().copied())
}

#[must_use]
pub(crate) fn parse_command_line_with_prefixes<'a>(
    text: &str,
    prefixes: impl IntoIterator<Item = &'a str>,
) -> Option<CommandInvocation> {
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return None;
    }

    let token_end = trimmed
        .char_indices()
        .find(|(_, ch)| ch.is_whitespace())
        .map_or(trimmed.len(), |(idx, _)| idx);

    let token = &trimmed[..token_end];
    let args = trimmed[token_end..].trim_start();

    let mut matched_prefix = None;
    let command = prefixes
        .into_iter()
        .find_map(|prefix| {
            let stripped = token.strip_prefix(prefix)?;
            (!stripped.is_empty()).then(|| {
                matched_prefix = Some(prefix);
                stripped
            })
        })
        .unwrap_or(token);

    Some(CommandInvocation::new(command, args, matched_prefix))
}

pub fn tokenize_command_args(args: &str) -> std::result::Result<Vec<String>, ArgsParseError> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut chars = args.chars();
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
        .ok_or_else(|| ArgsParseError::new(format!("Missing argument: {name}")))?;
    *index += 1;

    value
        .parse::<T>()
        .map_err(|err| ArgsParseError::new(format!("Failed to parse argument `{name}`: {err}")))
}

pub fn ensure_no_extra_args(
    tokens: &[String],
    index: usize,
) -> std::result::Result<(), ArgsParseError> {
    if let Some(extra) = tokens.get(index) {
        return Err(ArgsParseError::new(format!(
            "Unexpected extra argument: {extra}"
        )));
    }

    Ok(())
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
