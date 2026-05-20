use std::{fs, path::Path};

use serde_json::Value;

pub(super) fn write_json_file(path: &Path, value: &Value) -> Result<(), String> {
    let payload = serde_json::to_string_pretty(value).map_err(|err| err.to_string())?;
    fs::write(path, format!("{payload}\n")).map_err(|err| err.to_string())
}
