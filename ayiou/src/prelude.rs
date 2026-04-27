// Convenience re-exports: use ayiou::prelude::*;
pub use crate::{Bot, BotRuntimeOptions, ConsoleBot, OneBotV11Bot, QueueOverflowPolicy};

pub use crate::core::adapter::{
    Adapter, MsgContext, ProtocolAdapter, spawn_driver_adapter, spawn_protocol_adapter,
};
pub use crate::core::context::Context;
pub use crate::core::driver::Driver;
pub use crate::core::model::{
    BotId, ChannelKind, ChannelRef, CommandInvocation, EventEnvelope, MessageEvent,
    MessageSegment as CoreMessageSegment, OutboundMessage, OutboundReceipt, PlatformId, UserRef,
};
pub use crate::core::observability::{
    InMemoryMetrics, MetricsSink, NoopMetrics, spawn_metrics_log_reporter,
};
pub use crate::core::plugin::{
    ArgsParseError, ArgsParser, Command, CronSchedule, DispatchOptions, Dispatcher, Plugin,
    PluginManager, PluginMetadata, RegexValidated,
};
pub use crate::core::plugin_system::{
    ApplyConfigOutcome, ConfigUpdate, HandleOutcome, HandlerDecl, HandlerEventKind,
    LegacyManagedPluginAdapter, LegacyMessagePluginAdapter, RuntimePlugin, RuntimePluginEngine,
    RuntimePluginFactory, RuntimePluginManifest, RuntimePluginServices,
};
pub use crate::core::session::{
    MemorySessionStore, SessionError, SessionKey, SessionRecord, SessionStore,
};
pub use crate::core::storage::{MemoryStore, Store, StoreSerdeExt};
pub use crate::core::supervisor::{
    BotDefinition, BotManager, BotStatus, ConfigManager, ManagedPlugin, PluginCatalog,
    PluginConfigSnapshot, PluginFactory, PluginHealth, PluginInstanceSpec, PluginManagerApi,
    RuntimeServices, Supervisor,
};

pub use crate::adapter::console::adapter::ConsoleAdapter;
pub use crate::adapter::console::ctx::Ctx as ConsoleCtx;
pub use crate::adapter::console::ext::ConsoleBotExt;
pub use crate::adapter::console::sender::ConsoleSender;
pub use crate::adapter::onebot::v11::ctx::Ctx;
pub use crate::adapter::onebot::v11::ext::OneBotV11BotExt;
pub use crate::adapter::onebot::v11::model::{
    ApiResponse, GroupInfoData, GroupMemberInfoData, LoginInfoData, Message, MessageSegment,
    OneBotAction, SendMessageData,
};
pub use crate::adapter::onebot::v11::sender::OneBotSender;

// Re-export derive macros
pub use ayiou_macros::{Plugin, bot_plugin, command};

// Re-export async_trait for CommandHandler impl
pub use async_trait::async_trait;
