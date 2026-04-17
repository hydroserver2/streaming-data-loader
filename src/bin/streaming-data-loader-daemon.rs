fn main() {
    if let Err(error) = streaming_data_loader_lib::run_daemon() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
