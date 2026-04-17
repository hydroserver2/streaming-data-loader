use std::{
    collections::{HashMap, HashSet},
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
        manual_trigger_targets: impl IntoIterator<Item = (PathBuf, PathBuf)>,
        event_tx: mpsc::UnboundedSender<PathBuf>,
    ) -> Result<Option<Self>, String> {
        let watched_files: HashSet<PathBuf> = watched_files.into_iter().collect();
        let manual_trigger_targets: HashMap<PathBuf, PathBuf> =
            manual_trigger_targets.into_iter().collect();
        if watched_files.is_empty() && manual_trigger_targets.is_empty() {
            return Ok(None);
        }

        let watched_dirs = watched_files
            .iter()
            .filter_map(|path| path.parent().map(Path::to_path_buf))
            .chain(
                manual_trigger_targets
                    .keys()
                    .filter_map(|path| path.parent().map(Path::to_path_buf)),
            )
            .collect::<HashSet<_>>();
        let watched_files_for_handler = watched_files.clone();
        let manual_trigger_targets_for_handler = manual_trigger_targets.clone();

        let mut debouncer = new_debouncer(WATCH_DEBOUNCE_WINDOW, move |result| {
            handle_debounced_events(
                result,
                &watched_files_for_handler,
                &manual_trigger_targets_for_handler,
                &event_tx,
            );
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
    manual_trigger_targets: &HashMap<PathBuf, PathBuf>,
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
                            continue;
                        }

                        if let Some(target_path) = manual_trigger_targets.get(&path) {
                            changed.insert(target_path.clone());
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
