// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|arg| arg == "--service") {
        if let Err(error) = streaming_data_loader_lib::run_daemon() {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }

    streaming_data_loader_lib::run()
}
