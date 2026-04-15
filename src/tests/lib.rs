use super::{autostart_app_name, has_launch_flag, AUTOSTART_ARG};

#[test]
fn has_launch_flag_detects_autostart_argument() {
    assert!(has_launch_flag(
        ["streaming-data-loader", AUTOSTART_ARG],
        AUTOSTART_ARG
    ));
    assert!(!has_launch_flag(["streaming-data-loader"], AUTOSTART_ARG));
}

#[test]
fn autostart_app_name_matches_build_mode() {
    let expected = if cfg!(debug_assertions) {
        "Streaming Data Loader Dev"
    } else {
        "Streaming Data Loader"
    };

    assert_eq!(autostart_app_name(), expected);
}
