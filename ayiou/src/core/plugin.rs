use std::sync::Arc;

use anyhow::Result;
use log::info;

use crate::core::adapter::MsgContext;

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

/// Trait for parsing command arguments
pub trait ArgsParser: Sized + Default {
    /// Parse arguments from a string
    fn parse(args: &str) -> std::result::Result<Self, ArgsParseError>;

    /// Get usage/help text for this args type (optional)
    fn usage() -> Option<&'static str> {
        None
    }
}

/// Parsed command line components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandLine {
    command: String,
    args: String,
}

impl CommandLine {
    pub fn new(command: impl Into<String>, args: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: args.into(),
        }
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn args(&self) -> &str {
        &self.args
    }
}

/// Parse command line from text.
///
/// `prefixes` will be stripped from the first token when matched.
pub fn parse_command_line(text: &str, prefixes: &[&str]) -> Option<CommandLine> {
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
    for prefix in prefixes {
        if let Some(stripped) = token.strip_prefix(prefix)
            && !stripped.is_empty()
        {
            command = stripped;
            break;
        }
    }

    Some(CommandLine::new(command, args))
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

impl<T: ArgsParser> ArgsParser for Box<T> {
    fn parse(args: &str) -> std::result::Result<Self, ArgsParseError> {
        Ok(Box::new(T::parse(args)?))
    }

    fn usage() -> Option<&'static str> {
        T::usage()
    }
}

/// Trait for self-contained command execution
/// Implement this for your Args struct to avoid specifying a handler manually
#[async_trait::async_trait]
pub trait Command<C>: ArgsParser + Send + Sync + 'static {
    async fn run(self, ctx: C) -> Result<()>;
}

#[async_trait::async_trait]
impl<C, T: Command<C>> Command<C> for Box<T>
where
    C: Send + 'static,
{
    async fn run(self, ctx: C) -> Result<()> {
        (*self).run(ctx).await
    }
}

// ============================================================================
// Cron schedule wrapper
// ============================================================================

/// A parsed cron schedule that can compute upcoming trigger times
#[derive(Clone, Debug)]
pub struct CronSchedule {
    inner: cron::Schedule,
    source: String,
}

impl CronSchedule {
    /// Parse a cron expression string
    pub fn parse(expr: &str) -> std::result::Result<Self, ArgsParseError> {
        use std::str::FromStr;
        cron::Schedule::from_str(expr)
            .map(|inner| Self {
                inner,
                source: expr.to_string(),
            })
            .map_err(|e| ArgsParseError::new(format!("Invalid cron expression: {}", e)))
    }

    /// Get the source cron expression
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Get an iterator of upcoming trigger times from now
    pub fn upcoming(&self) -> impl Iterator<Item = chrono::DateTime<chrono::Utc>> + '_ {
        self.inner.upcoming(chrono::Utc)
    }

    /// Get the next trigger time after a given datetime
    pub fn next_after(
        &self,
        after: &chrono::DateTime<chrono::Utc>,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        self.inner.after(after).next()
    }

    /// Check if a datetime matches this schedule
    pub fn includes(&self, dt: chrono::DateTime<chrono::Utc>) -> bool {
        self.inner.includes(dt)
    }
}

impl std::fmt::Display for CronSchedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.source)
    }
}

// ============================================================================
// Regex wrapper for validated strings
// ============================================================================

/// A string that has been validated against a regex pattern
#[derive(Clone, Debug)]
pub struct RegexValidated {
    value: String,
    pattern: &'static str,
}

impl RegexValidated {
    /// Validate a string against a regex pattern
    pub fn validate(
        value: &str,
        pattern: &'static str,
    ) -> std::result::Result<Self, ArgsParseError> {
        let re = regex::Regex::new(pattern)
            .map_err(|e| ArgsParseError::new(format!("Invalid regex pattern: {}", e)))?;
        if re.is_match(value) {
            Ok(Self {
                value: value.to_string(),
                pattern,
            })
        } else {
            Err(ArgsParseError::new(format!(
                "Value '{}' does not match pattern '{}'",
                value, pattern
            )))
        }
    }

    /// Get the validated value
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Get the pattern used for validation
    pub fn pattern(&self) -> &'static str {
        self.pattern
    }
}

impl std::fmt::Display for RegexValidated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl AsRef<str> for RegexValidated {
    fn as_ref(&self) -> &str {
        &self.value
    }
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

/// Plugin trait: main entry point for message handling
#[async_trait::async_trait]
pub trait Plugin<C>: Send + Sync + 'static {
    /// Metadata (name/description/version)
    fn meta(&self) -> PluginMetadata {
        PluginMetadata::default()
    }

    /// Get list of commands this plugin handles (optimization)
    /// If empty, plugin is considered a "wildcard" and checked for every message.
    /// Commands are matched against the first word of the message (whitespace separated).
    fn commands(&self) -> Vec<String> {
        Vec::new()
    }

    /// Optional command prefixes that this plugin accepts.
    ///
    /// Example: ["/", "!"] means command `echo` also matches `/echo` and `!echo`.
    fn command_prefixes(&self) -> Vec<String> {
        Vec::new()
    }

    /// Check if this plugin matches the context (default: always match)
    fn matches(&self, _ctx: &C) -> bool {
        true
    }

    /// Handle the message, return Ok(true) to block subsequent handlers
    async fn handle(&self, ctx: &C) -> Result<bool>;
}

pub type PluginBox<C> = Box<dyn Plugin<C>>;

pub trait IntoPluginBox<C> {
    fn into_plugin_box(self) -> PluginBox<C>;
}

impl<P, C> IntoPluginBox<C> for P
where
    P: Plugin<C>,
    C: 'static,
{
    fn into_plugin_box(self) -> PluginBox<C> {
        Box::new(self)
    }
}

type PluginList<C> = Arc<[Arc<dyn Plugin<C>>]>;

#[derive(Clone)]
pub struct PluginManager<C> {
    /// Plugins pending registration (build phase)
    pending: Vec<Arc<dyn Plugin<C>>>,
    /// Runtime plugin snapshot (immutable after build)
    snapshot: Option<PluginList<C>>,
}

impl<C: MsgContext> Default for PluginManager<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> PluginManager<C>
where
    C: MsgContext,
{
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            snapshot: None,
        }
    }

    /// Register a plugin (build phase)
    pub fn register<P: Plugin<C>>(&mut self, plugin: P) {
        let meta = plugin.meta();
        info!(
            "Registering plugin: {} v{} - {}",
            meta.name, meta.version, meta.description
        );
        self.pending.push(Arc::new(plugin));
    }

    /// Register multiple plugins (supports different plugin types)
    pub fn register_all(&mut self, plugins: impl IntoIterator<Item = PluginBox<C>>) {
        for plugin in plugins {
            let meta = plugin.meta();
            info!(
                "Registering plugin: {} v{} - {}",
                meta.name, meta.version, meta.description
            );
            self.pending.push(Arc::from(plugin));
        }
    }

    /// Build snapshot (plugin list becomes immutable after this)
    pub fn build(&mut self) -> PluginList<C> {
        let snapshot: PluginList<C> = self.pending.drain(..).collect();
        self.snapshot = Some(snapshot.clone());
        snapshot
    }

    /// Get snapshot (if built)
    pub fn snapshot(&self) -> Option<PluginList<C>> {
        self.snapshot.clone()
    }

    /// Get all plugin metadata
    pub fn list(&self) -> Vec<PluginMetadata> {
        if let Some(ref snapshot) = self.snapshot {
            snapshot.iter().map(|p| p.meta()).collect()
        } else {
            self.pending.iter().map(|p| p.meta()).collect()
        }
    }

    /// Get plugin count
    pub fn count(&self) -> usize {
        if let Some(ref snapshot) = self.snapshot {
            snapshot.len()
        } else {
            self.pending.len()
        }
    }

    /// Check if plugin exists by name
    pub fn has(&self, name: &str) -> bool {
        if let Some(ref snapshot) = self.snapshot {
            snapshot.iter().any(|p| p.meta().name == name)
        } else {
            self.pending.iter().any(|p| p.meta().name == name)
        }
    }
}

use std::collections::HashMap;

#[derive(Clone)]
struct TrieNode<C> {
    children: HashMap<char, TrieNode<C>>,
    handlers: Vec<Arc<dyn Plugin<C>>>,
}

impl<C> Default for TrieNode<C> {
    fn default() -> Self {
        Self {
            children: HashMap::new(),
            handlers: Vec::new(),
        }
    }
}

impl<C> TrieNode<C> {
    fn insert(&mut self, cmd: &str, plugin: Arc<dyn Plugin<C>>) {
        let mut node = self;
        for c in cmd.chars() {
            node = node.children.entry(c).or_default();
        }

        if !node.handlers.iter().any(|p| Arc::ptr_eq(p, &plugin)) {
            node.handlers.push(plugin);
        }
    }

    /// Find the longest matching command.
    /// Enforces that the match must be a full word (followed by whitespace or end of string).
    fn match_command(&self, text: &str) -> Option<&Vec<Arc<dyn Plugin<C>>>> {
        let mut node = self;
        let mut last_match = None;
        let mut chars = text.chars().peekable();

        // Check root matches (e.g. empty command? unlikely but possible)
        if !node.handlers.is_empty() {
            // If text is empty or starts with whitespace, root is a match
            if text.is_empty() || text.starts_with(char::is_whitespace) {
                last_match = Some(&node.handlers);
            }
        }

        while let Some(c) = chars.next() {
            match node.children.get(&c) {
                Some(child) => {
                    node = child;
                    // potential match found
                    if !node.handlers.is_empty() {
                        // Check boundary: End of string OR next char is whitespace
                        match chars.peek() {
                            None => last_match = Some(&node.handlers),
                            Some(&next_char) if next_char.is_whitespace() => {
                                last_match = Some(&node.handlers)
                            }
                            _ => {} // Not a boundary, continue
                        }
                    }
                }
                None => break,
            }
        }
        last_match
    }
}

#[derive(Clone)]
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

#[derive(Clone)]
pub struct Dispatcher<C> {
    /// Plugins that don't declare specific commands (always checked)
    wildcards: Arc<Vec<Arc<dyn Plugin<C>>>>,
    /// Trie root for command lookup
    root: Arc<TrieNode<C>>,
    options: DispatchOptions,
}

impl<C: MsgContext> Dispatcher<C> {
    /// Create dispatcher from plugin list
    pub fn new(plugins: PluginList<C>) -> Self {
        Self::with_options(plugins, DispatchOptions::default())
    }

    pub fn with_options(plugins: PluginList<C>, options: DispatchOptions) -> Self {
        let mut root = TrieNode::default();
        let mut wildcards = Vec::new();

        for plugin in plugins.iter() {
            let cmds = plugin.commands();
            if cmds.is_empty() {
                wildcards.push(plugin.clone());
            } else {
                let prefixes: Vec<String> = plugin
                    .command_prefixes()
                    .into_iter()
                    .filter(|p| !p.is_empty())
                    .collect();

                for cmd in cmds {
                    root.insert(&cmd, plugin.clone());

                    for prefix in &prefixes {
                        if cmd.starts_with(prefix) {
                            continue;
                        }
                        root.insert(&format!("{}{}", prefix, cmd), plugin.clone());
                    }
                }
            }
        }

        Self {
            wildcards: Arc::new(wildcards),
            root: Arc::new(root),
            options,
        }
    }

    fn normalize_command_text(&self, text: &str) -> Option<String> {
        let trimmed = text.trim_start();
        if trimmed.is_empty() {
            return None;
        }

        let token_len = trimmed
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(trimmed.len());

        let (token, rest) = trimmed.split_at(token_len);

        for prefix in self.options.command_prefixes() {
            if let Some(stripped) = token.strip_prefix(prefix)
                && !stripped.is_empty()
            {
                let mut normalized = String::with_capacity(stripped.len() + rest.len());
                normalized.push_str(stripped);
                normalized.push_str(rest);
                return Some(normalized);
            }
        }

        None
    }

    /// Dispatch event to all matching plugins
    /// Note: Command-specific plugins are prioritized over wildcards
    /// Dispatch event to all matching plugins
    /// Priority: Commands > Wildcards
    pub async fn dispatch(&self, ctx: &C) -> Result<()> {
        // 1. explicit commands
        if self.dispatch_commands(ctx).await? {
            return Ok(());
        }

        // 2. wildcards
        if self.dispatch_wildcards(ctx).await? {
            return Ok(());
        }
        Ok(())
    }

    /// Dispatch only to matching commands
    pub async fn dispatch_commands(&self, ctx: &C) -> Result<bool> {
        let text = ctx.text();
        let normalized = self.normalize_command_text(&text);

        if let Some(plugins) = self.root.match_command(&text) {
            for plugin in plugins {
                if !plugin.matches(ctx) {
                    continue;
                }

                if let Ok(block) = plugin.handle(ctx).await
                    && block
                {
                    return Ok(true);
                }
            }
        }

        if let Some(normalized_text) = normalized
            && normalized_text != text
            && let Some(plugins) = self.root.match_command(&normalized_text)
        {
            for plugin in plugins {
                if !plugin.matches(ctx) {
                    continue;
                }

                if let Ok(block) = plugin.handle(ctx).await
                    && block
                {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Dispatch only to wildcard plugins
    pub async fn dispatch_wildcards(&self, ctx: &C) -> Result<bool> {
        for plugin in self.wildcards.iter() {
            if !plugin.matches(ctx) {
                continue;
            }

            if let Ok(block) = plugin.handle(ctx).await
                && block
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Clone)]
    struct TestCtx {
        text: String,
    }

    impl MsgContext for TestCtx {
        fn text(&self) -> String {
            self.text.clone()
        }

        fn user_id(&self) -> String {
            "u1".to_string()
        }

        fn group_id(&self) -> Option<String> {
            Some("g1".to_string())
        }
    }

    struct CounterPlugin {
        hits: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Plugin<TestCtx> for CounterPlugin {
        fn commands(&self) -> Vec<String> {
            vec!["echo".to_string()]
        }

        async fn handle(&self, _ctx: &TestCtx) -> Result<bool> {
            self.hits.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        }
    }

    #[tokio::test]
    async fn dispatch_matches_prefixed_commands() {
        let hits = Arc::new(AtomicUsize::new(0));
        let plugins: Arc<[Arc<dyn Plugin<TestCtx>>]> =
            vec![Arc::new(CounterPlugin { hits: hits.clone() }) as Arc<dyn Plugin<TestCtx>>].into();

        let dispatcher = Dispatcher::with_options(plugins, DispatchOptions::new(["/", "!", "."]));

        let ctx = TestCtx {
            text: "/echo hello".to_string(),
        };

        dispatcher.dispatch(&ctx).await.unwrap();

        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn dispatch_ignores_unknown_commands() {
        let hits = Arc::new(AtomicUsize::new(0));
        let plugins: Arc<[Arc<dyn Plugin<TestCtx>>]> =
            vec![Arc::new(CounterPlugin { hits: hits.clone() }) as Arc<dyn Plugin<TestCtx>>].into();

        let dispatcher = Dispatcher::with_options(plugins, DispatchOptions::new(["/"]));

        let ctx = TestCtx {
            text: "/unknown hello".to_string(),
        };

        dispatcher.dispatch(&ctx).await.unwrap();

        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    struct PrefixedCounterPlugin {
        hits: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Plugin<TestCtx> for PrefixedCounterPlugin {
        fn commands(&self) -> Vec<String> {
            vec!["echo".to_string()]
        }

        fn command_prefixes(&self) -> Vec<String> {
            vec!["/".to_string()]
        }

        async fn handle(&self, _ctx: &TestCtx) -> Result<bool> {
            self.hits.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        }
    }

    #[tokio::test]
    async fn dispatch_supports_plugin_level_command_prefixes() {
        let hits = Arc::new(AtomicUsize::new(0));
        let plugins: Arc<[Arc<dyn Plugin<TestCtx>>]> = vec![Arc::new(PrefixedCounterPlugin {
            hits: hits.clone(),
        }) as Arc<dyn Plugin<TestCtx>>]
        .into();

        let dispatcher = Dispatcher::new(plugins);

        let ctx = TestCtx {
            text: "/echo hello".to_string(),
        };

        dispatcher.dispatch(&ctx).await.unwrap();

        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

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
