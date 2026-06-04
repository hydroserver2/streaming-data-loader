use std::{fs, path::PathBuf};

use serde_json::{json, Value};

use super::{read_json_file, sidecar_path, write_json_file};

fn unique_temp_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "sdl-json-file-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

#[test]
fn write_then_read_round_trips() {
    let dir = unique_temp_dir("round-trip");
    let path = dir.join("config.json");

    write_json_file(&path, &json!({ "value": 1 })).expect("write");
    let loaded = read_json_file::<Value>(&path).expect("read");

    assert_eq!(loaded, Some(json!({ "value": 1 })));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn reading_a_missing_file_returns_none() {
    let dir = unique_temp_dir("missing");
    let path = dir.join("config.json");

    let loaded = read_json_file::<Value>(&path).expect("read missing");

    assert_eq!(loaded, None);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn second_write_preserves_prior_generation_as_prev() {
    let dir = unique_temp_dir("prev-generation");
    let path = dir.join("workspace.json");

    write_json_file(&path, &json!({ "generation": 1 })).expect("write v1");
    write_json_file(&path, &json!({ "generation": 2 })).expect("write v2");

    // Primary holds the latest; .prev holds the prior good generation.
    assert_eq!(
        read_json_file::<Value>(&path).expect("read primary"),
        Some(json!({ "generation": 2 }))
    );
    let prev_contents =
        fs::read_to_string(sidecar_path(&path, ".prev")).expect("read .prev backup");
    assert_eq!(
        serde_json::from_str::<Value>(&prev_contents).expect("parse .prev"),
        json!({ "generation": 1 })
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn corrupt_primary_recovers_from_prev_backup() {
    let dir = unique_temp_dir("recover-corrupt");
    let path = dir.join("workspace.json");

    write_json_file(&path, &json!({ "generation": 1 })).expect("write v1");
    write_json_file(&path, &json!({ "generation": 2 })).expect("write v2");

    // Simulate a write torn by a crash/power loss before atomic rename existed,
    // or a damaged disk block: the primary is now unparseable.
    fs::write(&path, b"{ this is not valid json").expect("corrupt primary");

    let recovered = read_json_file::<Value>(&path).expect("recover from .prev");
    assert_eq!(recovered, Some(json!({ "generation": 1 })));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_primary_with_intact_prev_recovers() {
    let dir = unique_temp_dir("recover-missing-primary");
    let path = dir.join("workspace.json");

    write_json_file(&path, &json!({ "generation": 1 })).expect("write v1");
    write_json_file(&path, &json!({ "generation": 2 })).expect("write v2");
    fs::remove_file(&path).expect("remove primary");

    let recovered = read_json_file::<Value>(&path).expect("recover from .prev");
    assert_eq!(recovered, Some(json!({ "generation": 1 })));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn both_files_corrupt_surfaces_an_error() {
    let dir = unique_temp_dir("both-corrupt");
    let path = dir.join("workspace.json");

    write_json_file(&path, &json!({ "generation": 1 })).expect("write v1");
    write_json_file(&path, &json!({ "generation": 2 })).expect("write v2");
    fs::write(&path, b"garbage").expect("corrupt primary");
    fs::write(sidecar_path(&path, ".prev"), b"also garbage").expect("corrupt prev");

    assert!(read_json_file::<Value>(&path).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn no_temp_files_remain_after_a_successful_write() {
    let dir = unique_temp_dir("no-temp-leftovers");
    let path = dir.join("config.json");

    write_json_file(&path, &json!({ "value": 1 })).expect("write");

    let leftover_temp = fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().contains(".tmp-"));
    assert!(
        !leftover_temp,
        "atomic write should not leave a temp file behind"
    );
    let _ = fs::remove_dir_all(&dir);
}
