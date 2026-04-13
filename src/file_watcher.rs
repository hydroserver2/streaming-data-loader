use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Duration,
};

use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, DebouncedEventKind, Debouncer};
use tokio::sync::mpsc;
use tracing::{error, warn};

const WATCH_DEBOUNCE_WINDOW: Duration = Duration::from_millis(500);

pub struct FilesystemWatcher {
    _debouncer: Debouncer<RecommendedWatcher>,
}

impl FilesystemWatcher {
    pub fn start(
        watched_files: impl IntoIterator<Item = PathBuf>,
        event_tx: mpsc::UnboundedSender<PathBuf>,
    ) -> Result<Option<Self>, String> {
        let watched_files: HashSet<PathBuf> = watched_files.into_iter().collect();
        if watched_files.is_empty() {
            return Ok(None);
        }

        let watched_dirs = watched_files
            .iter()
            .filter_map(|path| path.parent().map(Path::to_path_buf))
            .collect::<HashSet<_>>();
        let watched_files_for_handler = watched_files.clone();

        let mut debouncer = new_debouncer(WATCH_DEBOUNCE_WINDOW, move |result| {
            handle_debounced_events(result, &watched_files_for_handler, &event_tx);
        })
        .map_err(|err| err.to_string())?;

        for dir in watched_dirs {
            if let Err(err) = debouncer.watcher().watch(&dir, RecursiveMode::NonRecursive) {
                warn!(
                    dir = %dir.display(),
                    error = %err,
                    "couldn't watch directory; files in this path won't trigger until it becomes available"
                );
            }
        }

        Ok(Some(Self {
            _debouncer: debouncer,
        }))
    }
}

fn handle_debounced_events(
    result: DebounceEventResult,
    watched_files: &HashSet<PathBuf>,
    event_tx: &mpsc::UnboundedSender<PathBuf>,
) {
    match result {
        Ok(events) => {
            let mut changed = HashSet::new();
            for event in events {
                if matches!(
                    event.kind,
                    DebouncedEventKind::Any | DebouncedEventKind::AnyContinuous
                ) {
                    if let Some(path) = canonicalize_if_possible(&event.path)
                        .filter(|path| watched_files.contains(path))
                    {
                        changed.insert(path);
                    }
                }
            }

            for path in changed {
                if event_tx.send(path).is_err() {
                    warn!("filesystem watcher event dropped because the pipeline is shutting down");
                    break;
                }
            }
        }
        Err(error) => error!(?error, "filesystem watcher reported an error"),
    }
}

fn canonicalize_if_possible(path: &Path) -> Option<PathBuf> {
    path.canonicalize()
        .ok()
        .or_else(|| Some(path.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify_debouncer_mini::DebouncedEvent;

    fn make_event(path: PathBuf, kind: DebouncedEventKind) -> DebouncedEvent {
        DebouncedEvent { path, kind }
    }

    #[test]
    fn burst_writes_collapse_into_single_event() {
        let dir = std::env::temp_dir();
        let file_path = dir.join(format!(
            "sdl-debounce-test-{}.csv",
            std::process::id()
        ));
        std::fs::write(&file_path, "test").expect("create temp file");
        let canonical = file_path.canonicalize().unwrap_or_else(|_| file_path.clone());

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
        let canonical = watched_file.canonicalize().unwrap_or_else(|_| watched_file.clone());

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
}
