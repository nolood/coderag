//! Logging configuration and initialization for CodeRAG.
//!
//! Provides file-based logging with rotation and optional stderr output.

use crate::config::LoggingConfig;
use anyhow::{Context, Result};
use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{
    fmt,
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
    Layer,
};

/// Guard that must be held for the lifetime of the application.
/// When dropped, flushes any pending log writes.
///
/// # Important
/// This guard MUST be kept alive (e.g., assigned to a variable) for the
/// duration of the program. Dropping it prematurely will cause pending
/// log writes to be lost.
#[must_use = "Dropping this guard will stop logging - keep it alive for the program's lifetime"]
pub struct LoggingGuard {
    _file_guard: Option<WorkerGuard>,
    _stderr_guard: Option<WorkerGuard>,
}

/// Initialize the logging subsystem based on configuration.
///
/// Returns a guard that must be kept alive for the duration of the program.
/// Dropping the guard will flush pending log writes.
pub fn init_logging(config: &LoggingConfig, project_root: &Path) -> Result<LoggingGuard> {
    let mut file_guard = None;
    let mut stderr_guard = None;

    // Parse file log level
    let file_filter = parse_level(&config.level);

    // Build layers dynamically
    let registry = tracing_subscriber::registry();

    if config.enabled && config.stderr {
        // Both file and stderr logging
        let log_dir = resolve_log_dir(&config.directory, project_root);
        std::fs::create_dir_all(&log_dir)
            .with_context(|| format!("Failed to create log directory: {}", log_dir.display()))?;

        let rotation = parse_rotation(&config.rotation);
        let file_appender = RollingFileAppender::new(rotation, &log_dir, &config.file_prefix);
        let (non_blocking_file, guard) = tracing_appender::non_blocking(file_appender);
        file_guard = Some(guard);

        let file_layer = fmt::layer()
            .with_writer(non_blocking_file)
            .with_ansi(false)
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .with_filter(file_filter);

        let stderr_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("coderag=info"));
        let (non_blocking_stderr, guard) = tracing_appender::non_blocking(std::io::stderr());
        stderr_guard = Some(guard);

        let stderr_layer = fmt::layer()
            .with_writer(non_blocking_stderr)
            .with_target(false)
            .with_filter(stderr_filter);

        registry
            .with(file_layer)
            .with(stderr_layer)
            .try_init()
            .context("Failed to initialize logging subscriber")?;
    } else if config.enabled {
        // File logging only
        let log_dir = resolve_log_dir(&config.directory, project_root);
        std::fs::create_dir_all(&log_dir)
            .with_context(|| format!("Failed to create log directory: {}", log_dir.display()))?;

        let rotation = parse_rotation(&config.rotation);
        let file_appender = RollingFileAppender::new(rotation, &log_dir, &config.file_prefix);
        let (non_blocking_file, guard) = tracing_appender::non_blocking(file_appender);
        file_guard = Some(guard);

        let file_layer = fmt::layer()
            .with_writer(non_blocking_file)
            .with_ansi(false)
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .with_filter(file_filter);

        registry
            .with(file_layer)
            .try_init()
            .context("Failed to initialize logging subscriber")?;
    } else if config.stderr {
        // Stderr logging only
        let stderr_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("coderag=info"));
        let (non_blocking_stderr, guard) = tracing_appender::non_blocking(std::io::stderr());
        stderr_guard = Some(guard);

        let stderr_layer = fmt::layer()
            .with_writer(non_blocking_stderr)
            .with_target(false)
            .with_filter(stderr_filter);

        registry
            .with(stderr_layer)
            .try_init()
            .context("Failed to initialize logging subscriber")?;
    } else {
        // No logging - just init empty registry
        registry
            .try_init()
            .context("Failed to initialize logging subscriber")?;
    }

    Ok(LoggingGuard {
        _file_guard: file_guard,
        _stderr_guard: stderr_guard,
    })
}

fn resolve_log_dir(directory: &Path, project_root: &Path) -> std::path::PathBuf {
    if directory.is_absolute() {
        directory.to_path_buf()
    } else {
        project_root.join(directory)
    }
}

fn parse_level(level: &str) -> EnvFilter {
    let level_lower = level.to_lowercase();
    let level_str = match level_lower.as_str() {
        "trace" => "coderag=trace",
        "debug" => "coderag=debug",
        "info" => "coderag=info",
        "warn" => "coderag=warn",
        "error" => "coderag=error",
        _ => {
            eprintln!(
                "Warning: Unknown log level '{}', defaulting to 'debug'",
                level
            );
            "coderag=debug"
        }
    };
    EnvFilter::new(level_str)
}

fn parse_rotation(rotation: &str) -> Rotation {
    let rotation_lower = rotation.to_lowercase();
    match rotation_lower.as_str() {
        "hourly" => Rotation::HOURLY,
        "daily" => Rotation::DAILY,
        "minutely" => Rotation::MINUTELY,
        "never" => Rotation::NEVER,
        _ => {
            eprintln!(
                "Warning: Unknown rotation strategy '{}', defaulting to 'daily'",
                rotation
            );
            Rotation::DAILY
        }
    }
}

/// Initialize logging with defaults (for use before config is loaded).
/// This is a fallback for early startup errors.
pub fn init_early_logging() {
    let _ = tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("coderag=info")),
        )
        .with(fmt::layer().with_target(false))
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_level() {
        // Just verify no panics and correct format
        let filter = parse_level("debug");
        assert!(filter.to_string().contains("debug"));

        let filter = parse_level("TRACE");
        assert!(filter.to_string().contains("trace"));

        // Invalid level should default to debug
        let filter = parse_level("invalid");
        assert!(filter.to_string().contains("debug"));
    }

    #[test]
    fn test_parse_rotation() {
        // Rotation doesn't implement PartialEq, just verify no panic
        let _ = parse_rotation("daily");
        let _ = parse_rotation("hourly");
        let _ = parse_rotation("minutely");
        let _ = parse_rotation("never");
        let _ = parse_rotation("invalid"); // defaults to daily
    }

    #[test]
    fn test_resolve_log_dir_relative() {
        let project_root = Path::new("/home/user/project");
        let relative_dir = Path::new(".coderag/logs");

        let resolved = resolve_log_dir(relative_dir, project_root);
        assert_eq!(resolved, Path::new("/home/user/project/.coderag/logs"));
    }

    #[test]
    fn test_resolve_log_dir_absolute() {
        let project_root = Path::new("/home/user/project");
        let absolute_dir = Path::new("/var/log/coderag");

        let resolved = resolve_log_dir(absolute_dir, project_root);
        assert_eq!(resolved, Path::new("/var/log/coderag"));
    }
}
