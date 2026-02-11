// Convenience re-exports: use ayiou::prelude::*;
pub use crate::{Bot, ConsoleBot, OneBotV11Bot};

pub use crate::core::adapter::{
    Adapter, MsgContext, ProtocolAdapter, spawn_driver_adapter, spawn_protocol_adapter,
};
pub use crate::core::driver::Driver;
pub use crate::core::extract::{Args, Rest, TupleArgs};
pub use crate::core::plugin::{
    ArgsParseError, ArgsParser, Command, CronSchedule, DispatchOptions, Dispatcher, Plugin,
    PluginManager, PluginMetadata, RegexValidated,
};

pub use crate::adapter::console::adapter::ConsoleAdapter;
pub use crate::adapter::console::ctx::Ctx as ConsoleCtx;
pub use crate::adapter::console::ext::ConsoleBotExt;
pub use crate::adapter::onebot::v11::ctx::Ctx;
pub use crate::adapter::onebot::v11::ext::OneBotV11BotExt;
pub use crate::adapter::onebot::v11::model::{
    ApiResponse, GroupInfoData, GroupMemberInfoData, LoginInfoData, Message, MessageSegment,
    OneBotAction, SendMessageData,
};

// Re-export derive macros
pub use ayiou_macros::{Plugin, bot_plugin, command};

// Re-export async_trait for CommandHandler impl
pub use async_trait::async_trait;
