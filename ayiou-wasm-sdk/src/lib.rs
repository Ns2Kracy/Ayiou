use std::{
    cell::RefCell,
    slice, str,
    sync::atomic::{AtomicUsize, Ordering},
};

use serde::{Deserialize, Serialize};

pub mod abi {
    pub const MEMORY_EXPORT: &str = "memory";
    pub const ALLOC_EXPORT: &str = "ayiou_alloc";
    pub const ON_COMMAND_EXPORT: &str = "ayiou_on_command";
    pub const ON_REGEX_EXPORT: &str = "ayiou_on_regex";
    pub const ON_CRON_EXPORT: &str = "ayiou_on_cron";
    pub const TAKE_REPLY_EXPORT: &str = "ayiou_take_reply";
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DispatchEvent {
    Command { command: String, args: String },
    Regex { text: String },
    Cron { expr: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplyAction {
    Text { text: String },
}

impl ReplyAction {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn as_text(&self) -> &str {
        match self {
            Self::Text { text } => text,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCall {
    pub plugin: String,
    pub event: DispatchEvent,
    pub reply: Option<ReplyAction>,
}

impl HostCall {
    pub fn command(
        plugin: impl Into<String>,
        command: impl Into<String>,
        args: impl Into<String>,
    ) -> Self {
        Self {
            plugin: plugin.into(),
            event: DispatchEvent::Command {
                command: command.into(),
                args: args.into(),
            },
            reply: None,
        }
    }

    pub fn regex(plugin: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            plugin: plugin.into(),
            event: DispatchEvent::Regex { text: text.into() },
            reply: None,
        }
    }

    pub fn cron(plugin: impl Into<String>, expr: impl Into<String>) -> Self {
        Self {
            plugin: plugin.into(),
            event: DispatchEvent::Cron { expr: expr.into() },
            reply: None,
        }
    }

    pub fn with_reply(mut self, reply: ReplyAction) -> Self {
        self.reply = Some(reply);
        self
    }

    pub fn reply_text(&self) -> Option<&str> {
        self.reply.as_ref().map(ReplyAction::as_text)
    }
}

/// Pure Rust plugin logic.
///
/// Implement this trait, then call `export_plugin!(YourType)`.
/// The SDK handles ABI exports, memory allocation and UTF-8 decoding.
pub trait Plugin {
    fn on_command(_command: &str, _args: &str) -> Option<ReplyAction> {
        None
    }

    fn on_regex(_text: &str) -> Option<ReplyAction> {
        None
    }

    fn on_cron(_expr: &str) -> Option<ReplyAction> {
        None
    }
}

/// Runtime helpers used by `export_plugin!`.
pub mod runtime {
    use super::*;

    thread_local! {
        static LAST_REPLY: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
    }

    static HEAP_PTR: AtomicUsize = AtomicUsize::new(4096);

    pub fn alloc(len: i32) -> i32 {
        if len < 0 {
            return -1;
        }

        let Ok(len) = usize::try_from(len) else {
            return -1;
        };

        let ptr = HEAP_PTR.fetch_add(len, Ordering::Relaxed);
        i32::try_from(ptr).unwrap_or(-1)
    }

    pub fn read_bytes(ptr: i32, len: i32) -> Option<&'static [u8]> {
        if len < 0 {
            return None;
        }

        if len == 0 {
            return Some(&[]);
        }

        if ptr < 0 {
            return None;
        }

        let ptr = usize::try_from(ptr).ok()?;
        let len = usize::try_from(len).ok()?;
        ptr.checked_add(len)?;

        // Host writes payload bytes into module linear memory before dispatch.
        Some(unsafe { slice::from_raw_parts(ptr as *const u8, len) })
    }

    pub fn write_bytes(ptr: i32, payload: &[u8]) -> bool {
        if ptr < 0 {
            return false;
        }

        let Ok(ptr) = usize::try_from(ptr) else {
            return false;
        };

        let Ok(len) = i32::try_from(payload.len()) else {
            return false;
        };

        if len < 0 {
            return false;
        }

        // The pointer was allocated from module linear memory by `ayiou_alloc`.
        unsafe {
            core::ptr::copy_nonoverlapping(payload.as_ptr(), ptr as *mut u8, payload.len());
        }
        true
    }

    pub fn read_utf8(ptr: i32, len: i32) -> Option<&'static str> {
        let bytes = read_bytes(ptr, len)?;
        str::from_utf8(bytes).ok()
    }

    pub fn set_reply(reply: Option<ReplyAction>) {
        let encoded = reply.and_then(|value| serde_json::to_vec(&value).ok());
        LAST_REPLY.with(|slot| {
            *slot.borrow_mut() = encoded;
        });
    }

    pub fn take_reply_bytes() -> Option<Vec<u8>> {
        LAST_REPLY.with(|slot| slot.borrow_mut().take())
    }

    pub fn pack_ptr_len(ptr: i32, len: i32) -> i64 {
        let ptr = (ptr as u32) as u64;
        let len = (len as u32) as u64;
        ((len << 32) | ptr) as i64
    }
}

/// Export full Ayiou wasm ABI for a plugin type that implements [`Plugin`].
///
/// # Example
///
/// ```ignore
/// use ayiou_wasm_sdk::{Plugin, ReplyAction, export_plugin};
///
/// struct MyPlugin;
///
/// impl Plugin for MyPlugin {
///     fn on_command(command: &str, _args: &str) -> Option<ReplyAction> {
///         (command == "hello").then(|| ReplyAction::text("hello from wasm"))
///     }
/// }
///
/// export_plugin!(MyPlugin);
/// ```
#[macro_export]
macro_rules! export_plugin {
    ($plugin:ty) => {
        #[inline]
        fn __ayiou_store_reply(reply: Option<$crate::ReplyAction>) -> i32 {
            let handled = reply.is_some();
            $crate::runtime::set_reply(reply);
            handled as i32
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ayiou_alloc(len: i32) -> i32 {
            $crate::runtime::alloc(len)
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ayiou_take_reply() -> i64 {
            let Some(bytes) = $crate::runtime::take_reply_bytes() else {
                return 0;
            };

            let Ok(len) = i32::try_from(bytes.len()) else {
                return 0;
            };

            let ptr = $crate::runtime::alloc(len);
            if ptr < 0 {
                return 0;
            }

            if !$crate::runtime::write_bytes(ptr, &bytes) {
                return 0;
            }

            $crate::runtime::pack_ptr_len(ptr, len)
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ayiou_on_command(
            cmd_ptr: i32,
            cmd_len: i32,
            args_ptr: i32,
            args_len: i32,
        ) -> i32 {
            let Some(command) = $crate::runtime::read_utf8(cmd_ptr, cmd_len) else {
                return __ayiou_store_reply(None);
            };
            let Some(args) = $crate::runtime::read_utf8(args_ptr, args_len) else {
                return __ayiou_store_reply(None);
            };

            __ayiou_store_reply(<$plugin as $crate::Plugin>::on_command(command, args))
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ayiou_on_regex(text_ptr: i32, text_len: i32) -> i32 {
            let Some(text) = $crate::runtime::read_utf8(text_ptr, text_len) else {
                return __ayiou_store_reply(None);
            };

            __ayiou_store_reply(<$plugin as $crate::Plugin>::on_regex(text))
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ayiou_on_cron(expr_ptr: i32, expr_len: i32) -> i32 {
            let Some(expr) = $crate::runtime::read_utf8(expr_ptr, expr_len) else {
                return __ayiou_store_reply(None);
            };

            __ayiou_store_reply(<$plugin as $crate::Plugin>::on_cron(expr))
        }
    };
}
