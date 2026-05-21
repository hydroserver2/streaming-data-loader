use super::{connect_strategy, ConnectStrategy};

// Platforms with an ad-hoc daemon mode (macOS, Linux): windows_requires_service
// is false.
const AD_HOC: bool = false;
// Windows: the managed service is mandatory.
const WINDOWS: bool = true;

#[test]
fn installed_and_running_service_is_awaited_not_raced() {
    // The key fix: an installed, running service must be awaited rather than
    // racing it with a GUI-spawned daemon — on every platform.
    assert_eq!(
        connect_strategy(true, true, true, AD_HOC),
        ConnectStrategy::AwaitService
    );
    assert_eq!(
        connect_strategy(true, true, true, WINDOWS),
        ConnectStrategy::AwaitService
    );
}

#[test]
fn installed_but_stopped_service_is_never_raced() {
    // Previously macOS/Linux spawned a competing daemon here, which could wedge
    // the service into a restart loop. Now we ask the user to restart it.
    assert_eq!(
        connect_strategy(true, true, false, AD_HOC),
        ConnectStrategy::ServiceStopped
    );
    assert_eq!(
        connect_strategy(true, true, false, WINDOWS),
        ConnectStrategy::ServiceStopped
    );
}

#[test]
fn no_installed_service_runs_ad_hoc_on_macos_and_linux() {
    assert_eq!(
        connect_strategy(true, false, false, AD_HOC),
        ConnectStrategy::SpawnAdHoc
    );
}

#[test]
fn no_installed_service_is_a_hard_stop_on_windows() {
    assert_eq!(
        connect_strategy(true, false, false, WINDOWS),
        ConnectStrategy::ServiceRequired
    );
}

#[test]
fn unsupported_platform_always_runs_ad_hoc() {
    // `supported == false` means there is no managed service to defer to; the
    // windows flag is irrelevant because that path is gated on `supported`.
    assert_eq!(
        connect_strategy(false, false, false, AD_HOC),
        ConnectStrategy::SpawnAdHoc
    );
    assert_eq!(
        connect_strategy(false, false, false, WINDOWS),
        ConnectStrategy::SpawnAdHoc
    );
}
