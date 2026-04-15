use super::{
    active_app_directory_name, copy_dir_contents, has_runtime_state, move_or_copy_dir_contents,
    APP_DIRECTORY_NAME, DEV_APP_DIRECTORY_NAME,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn copy_dir_contents_copies_nested_runtime_state() {
    let temp_root = unique_temp_dir("runtime-copy");
    let source = temp_root.join("source");
    let target = temp_root.join("target");

    fs::create_dir_all(source.join("workspaces")).expect("create source workspaces");
    fs::write(source.join("config.json"), "{}").expect("write config");
    fs::write(
        source.join("workspaces").join("workspace.json"),
        "{\"datasources\":[]}",
    )
    .expect("write workspace");

    copy_dir_contents(&source, &target).expect("copy runtime state");

    assert!(target.join("config.json").exists());
    assert!(target.join("workspaces").join("workspace.json").exists());

    remove_temp_dir(&temp_root);
}

#[test]
fn copy_dir_contents_does_not_overwrite_existing_files() {
    let temp_root = unique_temp_dir("runtime-preserve");
    let source = temp_root.join("source");
    let target = temp_root.join(APP_DIRECTORY_NAME);

    fs::create_dir_all(&source).expect("create source");
    fs::create_dir_all(&target).expect("create target");
    fs::write(source.join("config.json"), "{\"url\":\"new\"}").expect("write source");
    fs::write(target.join("config.json"), "{\"url\":\"existing\"}").expect("write target");

    copy_dir_contents(&source, &target).expect("copy runtime state");

    let persisted = fs::read_to_string(target.join("config.json")).expect("read target");
    assert_eq!(persisted, "{\"url\":\"existing\"}");

    remove_temp_dir(&temp_root);
}

#[test]
fn move_or_copy_dir_contents_moves_source_when_target_is_missing() {
    let temp_root = unique_temp_dir("runtime-move");
    let source = temp_root.join("source");
    let target = temp_root.join("target");

    fs::create_dir_all(&source).expect("create source");
    fs::write(source.join("config.json"), "{}").expect("write config");

    move_or_copy_dir_contents(&source, &target).expect("move runtime state");

    assert!(!source.exists());
    assert!(target.join("config.json").exists());

    remove_temp_dir(&temp_root);
}

#[test]
fn has_runtime_state_detects_config_or_workspace_dir() {
    let temp_root = unique_temp_dir("runtime-state");
    let config_only = temp_root.join("config-only");
    let workspace_only = temp_root.join("workspace-only");

    fs::create_dir_all(&config_only).expect("create config dir");
    fs::create_dir_all(workspace_only.join("workspaces")).expect("create workspace dir");
    fs::write(config_only.join("config.json"), "{}").expect("write config");

    assert!(has_runtime_state(&config_only));
    assert!(has_runtime_state(&workspace_only));
    assert!(!has_runtime_state(&temp_root.join("empty")));

    remove_temp_dir(&temp_root);
}

#[test]
fn active_app_directory_name_matches_build_mode() {
    let expected = if cfg!(debug_assertions) {
        DEV_APP_DIRECTORY_NAME
    } else {
        APP_DIRECTORY_NAME
    };

    assert_eq!(active_app_directory_name(), expected);
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("sdl-{label}-{nanos}"));
    fs::create_dir_all(&root).expect("create temp root");
    root
}

fn remove_temp_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).expect("remove temp dir");
    }
}
