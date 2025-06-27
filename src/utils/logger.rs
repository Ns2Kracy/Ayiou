use std::path::Path;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{
    EnvFilter, Registry,
    fmt::{self, time::ChronoUtc},
    prelude::*,
};

use crate::app::LoggingConfig;

/// Initialize the logging system based on configuration
pub fn init(log_config: &LoggingConfig) -> Result<(), String> {
    // Initialize the base subscriber with filter
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "{}={},tower_http=debug,axum::rejection=trace",
            env!("CARGO_CRATE_NAME"),
            log_config.level
        ))
    });

    // Start building the subscriber
    let subscriber = Registry::default().with(filter);

    // Create a pretty formatter for human-readable output
    let fmt_layer = fmt::layer()
        .with_target(false)
        .with_level(true)
        .with_timer(ChronoUtc::new("%F %T".to_string()))
        .with_ansi(true);

    if let Some(file_path) = &log_config.file_path {
        let directory = Path::new(file_path)
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let filename = Path::new(file_path)
            .file_name()
            .unwrap_or_else(|| "ayiah.log".as_ref())
            .to_string_lossy();

        if !directory.exists() {
            std::fs::create_dir_all(directory).expect("Failed to create log directory");
        }
        let (non_blocking, _guard) = tracing_appender::non_blocking(RollingFileAppender::new(
            Rotation::DAILY,
            directory,
            filename.to_string(),
        ));

        // Create formatter layer with the non-blocking writer
        let layer = fmt_layer
            .with_ansi(false) // Disable colors in file output
            .with_writer(non_blocking);

        tracing::subscriber::set_global_default(subscriber.with(layer))
            .map_err(|e| format!("Failed to set global default subscriber: {e}"))?;
    } else {
        tracing::subscriber::set_global_default(subscriber.with(fmt_layer))
            .map_err(|e| format!("Failed to set global default subscriber: {e}"))?;
    }

    Ok(())
}
