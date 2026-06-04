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
                    if let Some(path) = canonicalize_if_possible(&event.path) {
                        if watched_files.contains(&path) {
                            changed.insert(path);
                        }
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
#[path = "tests/file_watcher.rs"]
mod tests;
