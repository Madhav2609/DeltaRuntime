use tracing::info;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use tracing_appender::rolling;
use anyhow::{Result, Context};
use std::path::PathBuf;
use dirs::config_dir;

/// Initialize the logging system with file rotation and console output
///
/// This function sets up tracing with:
/// - Console output for debug builds
/// - File logging with daily rotation
/// - Configurable log levels via environment variables
/// - Logs stored in the application config directory
///
/// # Returns
/// `Ok(())` if logging was initialized successfully
pub fn init_logging() -> Result<()> {
    // Create logs directory
    let logs_dir = get_logs_dir()?;
    std::fs::create_dir_all(&logs_dir)
        .with_context(|| format!("Failed to create logs directory: {}", logs_dir.display()))?;

    // Create file appender with daily rotation
    let file_appender = rolling::daily(&logs_dir, "gta-mod-launcher.log");

    // Create environment filter
    // Default to INFO level, but allow override via RUST_LOG environment variable
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            if cfg!(debug_assertions) {
                EnvFilter::new("debug")
            } else {
                EnvFilter::new("info")
            }
        });

    // Build the subscriber
    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_writer(file_appender)
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
        );

    // Add console output for debug builds
    #[cfg(debug_assertions)]
    let subscriber = subscriber.with(
        fmt::layer()
            .with_writer(std::io::stdout)
            .with_ansi(true)
            .with_target(false)
            .with_thread_ids(false)
            .with_file(false)
            .with_line_number(false)
    );

    // Initialize the global subscriber
    subscriber.init();

    info!("Logging system initialized");
    info!("Logs directory: {}", logs_dir.display());
    
    Ok(())
}

/// Get the directory where log files should be stored
///
/// # Returns
/// Path to the logs directory in the application config folder
fn get_logs_dir() -> Result<PathBuf> {
    let config_dir = config_dir()
        .context("Failed to get config directory")?;
    
    Ok(config_dir.join("GTA Mod Launcher").join("logs"))
}

/// Get the current log level as a string
///
/// # Returns
/// The current log level (e.g., "INFO", "DEBUG", "ERROR")
pub fn get_log_level() -> String {
    // This is a simplified version - in a real implementation you'd want to
    // track the actual configured level
    if cfg!(debug_assertions) {
        "DEBUG".to_string()
    } else {
        "INFO".to_string()
    }
}

/// Clean up old log files (older than specified days)
///
/// This function removes log files that are older than the specified number of days
/// to prevent unlimited disk usage growth.
///
/// # Arguments
/// * `days_to_keep` - Number of days of logs to keep
///
/// # Returns
/// Number of files deleted
pub fn cleanup_old_logs(days_to_keep: u64) -> Result<usize> {
    let logs_dir = get_logs_dir()?;
    
    if !logs_dir.exists() {
        return Ok(0);
    }

    let cutoff_time = std::time::SystemTime::now() - std::time::Duration::from_secs(days_to_keep * 24 * 60 * 60);
    let mut deleted_count = 0;

    for entry in std::fs::read_dir(&logs_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() {
            if let Some(extension) = path.extension() {
                if extension == "log" {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified_time) = metadata.modified() {
                            if modified_time < cutoff_time {
                                if std::fs::remove_file(&path).is_ok() {
                                    deleted_count += 1;
                                    info!("Deleted old log file: {}", path.display());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(deleted_count)
}

/// Get information about the current logs directory
///
/// # Returns
/// Tuple of (logs_directory_path, total_size_bytes, file_count)
pub fn get_logs_info() -> Result<(PathBuf, u64, usize)> {
    let logs_dir = get_logs_dir()?;
    
    if !logs_dir.exists() {
        return Ok((logs_dir, 0, 0));
    }

    let mut total_size = 0u64;
    let mut file_count = 0usize;

    for entry in std::fs::read_dir(&logs_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() {
            if let Some(extension) = path.extension() {
                if extension == "log" {
                    if let Ok(metadata) = entry.metadata() {
                        total_size += metadata.len();
                        file_count += 1;
                    }
                }
            }
        }
    }

    Ok((logs_dir, total_size, file_count))
}

/// Log a startup message with system information
pub fn log_startup_info() {
    info!("=== GTA:SA Mod Launcher Starting ===");
    info!("Version: {}", env!("CARGO_PKG_VERSION"));
    info!("Build target: {}", std::env::consts::ARCH);
    info!("Operating system: {}", std::env::consts::OS);
    
    if let Ok(current_dir) = std::env::current_dir() {
        info!("Working directory: {}", current_dir.display());
    }
    
    if let Ok(exe_path) = std::env::current_exe() {
        info!("Executable path: {}", exe_path.display());
    }
    
    info!("Log level: {}", get_log_level());
}

/// Log a shutdown message
pub fn log_shutdown_info() {
    info!("=== GTA:SA Mod Launcher Shutting Down ===");
}

/// Custom macro for logging errors with context
#[macro_export]
macro_rules! log_error {
    ($error:expr) => {
        tracing::error!("Error: {:#}", $error);
    };
    ($error:expr, $context:expr) => {
        tracing::error!("Error in {}: {:#}", $context, $error);
    };
}

/// Custom macro for logging warnings
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        tracing::warn!($($arg)*);
    };
}

/// Custom macro for logging info messages
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        tracing::info!($($arg)*);
    };
}

/// Custom macro for logging debug messages
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        tracing::debug!($($arg)*);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_logs_dir() {
        let logs_dir = get_logs_dir().unwrap();
        assert!(logs_dir.to_string_lossy().contains("GTA Mod Launcher"));
        assert!(logs_dir.to_string_lossy().contains("logs"));
    }

    #[test]
    fn test_get_log_level() {
        let level = get_log_level();
        assert!(level == "DEBUG" || level == "INFO");
    }
}