use std::{
    backtrace::Backtrace,
    panic,
    sync::{Once, OnceLock},
};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};

use crate::service_paths::resolve_shared_logs_dir;

/// Number of daily-rotated log files to keep per context before the oldest are
/// pruned. Two weeks is enough history to investigate an issue without letting
/// the always-on daemon's logs grow unbounded.
const MAX_RETAINED_LOG_FILES: usize = 14;

static LOGGING_INIT: Once = Once::new();
static PANIC_HOOK_INIT: Once = Once::new();
static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

#[derive(Clone, Copy)]
enum LogContext {
    Desktop,
    Daemon,
    ServiceManager,
}

impl LogContext {
    fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Daemon => "daemon",
            Self::ServiceManager => "service-manager",
        }
    }

    /// Filename stem for this context's rotating log files. The daily date and a
    /// `.log` suffix are appended by the rolling appender, e.g. `daemon.2026-05-21.log`.
    fn file_prefix(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Daemon => "daemon",
            Self::ServiceManager => "service-manager",
        }
    }
}

pub fn init_desktop_logging() {
    init_logging(LogContext::Desktop);
}

pub fn init_daemon_logging() {
    init_logging(LogContext::Daemon);
}

pub fn init_process_logging_from_args() {
    if std::env::args_os().any(|arg| arg == "--service") {
        init_logging(LogContext::Daemon);
        return;
    }

    let has_service_management_args = std::env::args_os().any(|arg| {
        matches!(
            arg.to_string_lossy().as_ref(),
            "--windows-service-action"
                | "--linux-service-action"
                | "--windows-service-result-file"
                | "--linux-service-result-file"
        )
    });

    if has_service_management_args {
        init_logging(LogContext::ServiceManager);
        return;
    }

    init_logging(LogContext::Desktop);
}

fn init_logging(context: LogContext) {
    LOGGING_INIT.call_once(|| {
        if let Err(error) = init_file_logging(context) {
            let _ = tracing_subscriber::fmt()
                .with_target(true)
                .with_file(true)
                .with_line_number(true)
                .with_thread_ids(true)
                .with_max_level(tracing::Level::INFO)
                .try_init();
            eprintln!(
                "Couldn't initialize file logging for {}: {}",
                context.as_str(),
                error
            );
        }
    });

    install_panic_hook();
}

fn init_file_logging(context: LogContext) -> Result<(), String> {
    let logs_dir = resolve_shared_logs_dir()?;
    // The macOS LaunchDaemon redirects the daemon's raw stdout/stderr to
    // `daemon.stdout.log`/`daemon.stderr.log` in `logs_dir`. The rolling
    // appender prunes any file in its directory whose name shares the configured
    // prefix and suffix (it skips date validation once both are set), which would
    // match — and eventually delete — those launchd-owned files. Keep our rotated
    // logs in a dedicated subdirectory so pruning can only ever touch our own.
    let rolling_dir = logs_dir.join("rolling");
    std::fs::create_dir_all(&rolling_dir).map_err(|err| err.to_string())?;

    // Roll daily and retain a bounded number of files. The daemon is an
    // always-on service, so an unrotated log would grow without limit and
    // eventually exhaust disk on long-lived installs.
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(context.file_prefix())
        .filename_suffix("log")
        .max_log_files(MAX_RETAINED_LOG_FILES)
        .build(&rolling_dir)
        .map_err(|err| err.to_string())?;
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let _ = LOG_GUARD.set(guard);

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_max_level(tracing::Level::INFO)
        .with_writer(non_blocking)
        .try_init()
        .map_err(|err| err.to_string())?;

    tracing::info!(
        process = context.as_str(),
        logs_dir = %rolling_dir.display(),
        retained_files = MAX_RETAINED_LOG_FILES,
        "persistent file logging initialized with daily rotation"
    );

    Ok(())
}

fn install_panic_hook() {
    PANIC_HOOK_INIT.call_once(|| {
        let default_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic_info| {
            let location = panic_info
                .location()
                .map(|location| format!("{}:{}", location.file(), location.line()))
                .unwrap_or_else(|| "unknown".to_string());

            let message = if let Some(message) = panic_info.payload().downcast_ref::<&str>() {
                (*message).to_string()
            } else if let Some(message) = panic_info.payload().downcast_ref::<String>() {
                message.clone()
            } else {
                "panic payload is not a string".to_string()
            };

            let backtrace = Backtrace::force_capture();
            tracing::error!(
                panic.message = %message,
                panic.location = %location,
                panic.backtrace = %backtrace,
                "application panicked"
            );

            default_hook(panic_info);
        }));
    });
}
