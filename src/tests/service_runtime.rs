use super::{acquire_daemon_pid_lock, PID_LOCK_FILENAME};
use std::{fs, path::PathBuf, time::UNIX_EPOCH};

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("sdl-{label}-{nanos}"));
    fs::create_dir_all(&root).expect("create temp root");
    root
}

/// bug_007: a second daemon pointed at the same shared config dir must fail
/// to acquire the pid lock while the first one is running.
#[test]
fn second_daemon_cannot_acquire_pid_lock_while_first_is_held() {
    let dir = unique_temp_dir("pid-lock");
    let first = acquire_daemon_pid_lock(&dir).expect("first lock should succeed");

    let second = acquire_daemon_pid_lock(&dir);
    assert!(
        second.is_err(),
        "second lock acquisition should fail while first is held"
    );
    let message = second.unwrap_err();
    assert!(
        message.contains("already running"),
        "error should explain that another daemon is running, got: {message}"
    );

    drop(first);

    // Once the first lock is released the second daemon can start.
    let after = acquire_daemon_pid_lock(&dir);
    assert!(
        after.is_ok(),
        "dropping the first lock should free the pid file, got: {after:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Lock acquisition writes the current PID to the pid file so operators can
/// see which daemon holds the lock.
#[test]
fn pid_lock_records_current_process_id() {
    let dir = unique_temp_dir("pid-lock-content");
    let _guard = acquire_daemon_pid_lock(&dir).expect("acquire lock");

    let contents = fs::read_to_string(dir.join(PID_LOCK_FILENAME)).expect("read pid file");
    let parsed: u32 = contents
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("pid file should contain a u32, got: {contents:?}"));
    assert_eq!(parsed, std::process::id());

    let _ = fs::remove_dir_all(&dir);
}
