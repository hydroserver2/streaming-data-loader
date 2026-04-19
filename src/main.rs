// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    streaming_data_loader_lib::init_process_logging_from_args();

    if let Some(exit_code) = streaming_data_loader_lib::maybe_handle_service_management_cli() {
        std::process::exit(exit_code);
    }

    if std::env::args().any(|arg| arg == "--service") {
        if let Err(error) = streaming_data_loader_lib::run_daemon() {
            tracing::error!(error = %error, "daemon exited with an error");
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }

    streaming_data_loader_lib::run()
}
