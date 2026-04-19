use std::{
    backtrace::Backtrace,
    panic,
    sync::{Once, OnceLock},
};

use tracing_appender::non_blocking::WorkerGuard;

use crate::service_paths::resolve_shared_logs_dir;

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

    fn file_name(self) -> &'static str {
        match self {
            Self::Desktop => "desktop.log",
            Self::Daemon => "daemon.log",
            Self::ServiceManager => "service-manager.log",
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
    let log_path = logs_dir.join(context.file_name());
    let file_appender = tracing_appender::rolling::never(&logs_dir, context.file_name());
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
        log_file = %log_path.display(),
        "persistent file logging initialized"
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
