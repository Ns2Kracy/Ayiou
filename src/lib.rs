use mimalloc::MiMalloc;

pub mod app;
pub mod controllers;
pub mod data;
pub mod initializers;
pub mod models;
pub mod tasks;
pub mod views;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
