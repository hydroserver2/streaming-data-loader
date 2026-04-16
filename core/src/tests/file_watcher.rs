use super::*;
use notify_debouncer_mini::DebouncedEvent;

fn make_event(path: PathBuf, kind: DebouncedEventKind) -> DebouncedEvent {
    DebouncedEvent { path, kind }
}

#[test]
fn burst_writes_collapse_into_single_event() {
    let dir = std::env::temp_dir();
    let file_path = dir.join(format!("sdl-debounce-test-{}.csv", std::process::id()));
    std::fs::write(&file_path, "test").expect("create temp file");
    let canonical = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.clone());

    let watched: HashSet<PathBuf> = [canonical.clone()].into_iter().collect();
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Simulate 10 rapid events for the same file — the kind a burst write produces
    let events: Vec<DebouncedEvent> = (0..10)
        .map(|_| make_event(file_path.clone(), DebouncedEventKind::Any))
        .collect();

    handle_debounced_events(Ok(events), &watched, &tx);

    // Only one message should be sent despite 10 events
    let mut count = 0;
    while rx.try_recv().is_ok() {
        count += 1;
    }
    assert_eq!(count, 1, "burst of 10 events should collapse to 1 send");

    let _ = std::fs::remove_file(&file_path);
}

#[test]
fn unwatched_files_are_ignored() {
    let dir = std::env::temp_dir();
    let watched_file = dir.join("sdl-watched.csv");
    let unwatched_file = dir.join("sdl-unwatched.csv");
    std::fs::write(&watched_file, "test").expect("create temp file");
    std::fs::write(&unwatched_file, "test").expect("create temp file");
    let canonical = watched_file
        .canonicalize()
        .unwrap_or_else(|_| watched_file.clone());

    let watched: HashSet<PathBuf> = [canonical].into_iter().collect();
    let (tx, mut rx) = mpsc::unbounded_channel();

    let events = vec![make_event(unwatched_file.clone(), DebouncedEventKind::Any)];
    handle_debounced_events(Ok(events), &watched, &tx);

    assert!(
        rx.try_recv().is_err(),
        "events for unwatched files should be ignored"
    );

    let _ = std::fs::remove_file(&watched_file);
    let _ = std::fs::remove_file(&unwatched_file);
}
