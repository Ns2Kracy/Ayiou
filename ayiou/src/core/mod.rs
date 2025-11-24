pub mod adapter;
pub mod context;
pub mod driver;
pub mod error;
pub mod event;
pub mod plugin;

pub use adapter::Adapter;
pub use context::Context;
pub use driver::Driver;
pub use event::Event;
pub use plugin::Plugin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetType {
    Private,
    Group,
    Channel,
}
