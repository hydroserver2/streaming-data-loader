use std::{
    ffi::OsString,
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use serde::de::DeserializeOwned;
use serde_json::Value;

/// Suffix for the retained previous-good generation of a state file.
const PREV_SUFFIX: &str = ".prev";

/// Serialize `value` and write it to `path` crash-safely (see [`write_atomic`]).
pub(super) fn write_json_file(path: &Path, value: &Value) -> Result<(), String> {
    let payload = serde_json::to_string_pretty(value).map_err(|err| err.to_string())?;
    write_atomic(path, format!("{payload}\n").as_bytes())
}

/// Read and deserialize a JSON state file, transparently recovering from the
/// `<name>.prev` backup when the primary file is missing or unparseable (a
/// write interrupted by a crash/power loss, or a damaged disk block).
///
/// Returns `Ok(None)` only when neither the primary nor the backup exists, so
/// callers can treat a fresh install the same as before. If the primary is
/// corrupt but the backup is intact, the backup is returned and a warning is
/// logged. Only when *both* are unreadable does this surface an error, leaving
/// the documented manual-recovery path (delete the file and restart) intact.
pub(super) fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    let primary_error = match fs::read_to_string(path) {
        Ok(contents) => match serde_json::from_str::<T>(&contents) {
            Ok(value) => return Ok(Some(value)),
            Err(err) => Some(err.to_string()),
        },
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => return Err(err.to_string()),
    };

    let prev_path = sidecar_path(path, PREV_SUFFIX);
    match fs::read_to_string(&prev_path) {
        Ok(contents) => match serde_json::from_str::<T>(&contents) {
            Ok(value) => {
                if let Some(error) = &primary_error {
                    tracing::warn!(
                        path = %path.display(),
                        error = %error,
                        "state file was unreadable; recovered from .prev backup"
                    );
                }
                Ok(Some(value))
            }
            Err(prev_error) => Err(match primary_error {
                Some(error) => format!(
                    "{} is corrupt ({error}) and its .prev backup is also corrupt ({prev_error})",
                    path.display()
                ),
                None => format!("{} is corrupt: {prev_error}", prev_path.display()),
            }),
        },
        // Backup unreadable: if the primary simply didn't exist this is a fresh
        // install; otherwise report the primary's corruption.
        Err(_) => match primary_error {
            Some(error) => Err(format!("{} is corrupt: {error}", path.display())),
            None => Ok(None),
        },
    }
}

/// Crash-safe file replacement. Writes to a sibling temp file and fsyncs it,
/// preserves the current contents as `<name>.prev`, atomically renames the temp
/// over the target, then fsyncs the parent directory so the rename itself is
/// durable. A crash at any step leaves either the prior complete file at `path`
/// or a complete `.prev` to recover from — never a truncated target.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| {
        format!(
            "Cannot write {}: it has no parent directory.",
            path.display()
        )
    })?;

    // Temp lives in the same directory as the target so the rename stays on one
    // filesystem (a cross-device rename is not atomic and would fail).
    let tmp_path = sidecar_path(path, &format!(".tmp-{}", std::process::id()));
    let write_result = (|| {
        let mut tmp = fs::File::create(&tmp_path).map_err(|err| err.to_string())?;
        tmp.write_all(bytes).map_err(|err| err.to_string())?;
        tmp.sync_all().map_err(|err| err.to_string())
    })();
    if let Err(err) = write_result {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }

    // Preserve the current contents as the previous-good generation before we
    // replace them. Best-effort: a hiccup copying `.prev` must never block the
    // primary write, and the reader only consults `.prev` when the primary is
    // unparseable anyway.
    if path.exists() {
        let _ = fs::copy(path, sidecar_path(path, PREV_SUFFIX));
    }

    if let Err(err) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err.to_string());
    }

    // fsync the directory so the rename survives a power loss. Opening a
    // directory as a file isn't supported on every platform (notably Windows),
    // so this is best-effort.
    if let Ok(dir) = fs::File::open(parent) {
        let _ = dir.sync_all();
    }

    Ok(())
}

/// Append `suffix` to a path's filename, e.g. `foo.json` -> `foo.json.prev`.
/// Appends rather than replacing the extension so the backup of `a.json` never
/// collides with a workspace literally named `a`.
fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
#[path = "../tests/json_file.rs"]
mod tests;
