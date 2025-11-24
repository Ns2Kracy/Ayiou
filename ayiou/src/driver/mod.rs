#![forbid(unsafe_code)]

pub mod console;
pub mod wsclient;

use std::sync::Arc;

pub use console::ConsoleDriver;
use tokio::sync::{Mutex, mpsc};
pub use wsclient::WSClientDriver;

use crate::core::{Adapter, Context};

pub type AdapterBuilder = Arc<dyn Fn(Context) -> Box<dyn Adapter> + Send + Sync>;
pub type OutboundReceiver = Arc<Mutex<Option<mpsc::Receiver<String>>>>;
