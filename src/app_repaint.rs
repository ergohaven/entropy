#[cfg(not(target_arch = "wasm32"))]
const VISIBLE_REPAINT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
#[cfg(not(target_arch = "wasm32"))]
const HIDDEN_TO_TRAY_REPAINT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
#[cfg(not(target_arch = "wasm32"))]
const BLUETOOTH_VISIBLE_REPAINT_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(16);
#[cfg(not(target_arch = "wasm32"))]
pub(super) const CONNECT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
#[cfg(not(target_arch = "wasm32"))]
pub(super) const UPDATE_CHECK_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(100);

#[cfg(not(target_arch = "wasm32"))]
fn should_use_high_frequency_bluetooth_repaint_for_target(
    selected_device_is_bluetooth: bool,
    target_is_macos: bool,
) -> bool {
    // Linux and Windows receive immediate input-driven repaints from eframe.
    // Rendering a heavy keyboard layout every 16 ms while a Bluetooth device
    // is selected only consumes the UI thread and makes pointer input lag.
    // Keep the existing macOS cadence unchanged; this fix only removes the
    // unnecessary Linux timer.
    selected_device_is_bluetooth && target_is_macos
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn should_use_high_frequency_bluetooth_repaint(
    selected_device_is_bluetooth: bool,
) -> bool {
    should_use_high_frequency_bluetooth_repaint_for_target(
        selected_device_is_bluetooth,
        cfg!(target_os = "macos"),
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn native_repaint_interval(
    hidden_to_tray: bool,
    high_frequency_bluetooth: bool,
    connect_pending: bool,
    update_check_pending: bool,
) -> std::time::Duration {
    let baseline = if hidden_to_tray {
        HIDDEN_TO_TRAY_REPAINT_INTERVAL
    } else if high_frequency_bluetooth {
        BLUETOOTH_VISIBLE_REPAINT_INTERVAL
    } else {
        VISIBLE_REPAINT_INTERVAL
    };

    let connect_interval = connect_pending.then_some(CONNECT_POLL_INTERVAL);
    let update_check_interval = update_check_pending.then_some(UPDATE_CHECK_POLL_INTERVAL);

    [Some(baseline), connect_interval, update_check_interval]
        .into_iter()
        .flatten()
        .min()
        .expect("baseline repaint interval is always present")
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn native_repaint_cadence_uses_shortest_pending_interval() {
        assert_eq!(
            native_repaint_interval(false, false, false, false),
            std::time::Duration::from_millis(250)
        );
        assert_eq!(
            native_repaint_interval(false, true, false, false),
            BLUETOOTH_VISIBLE_REPAINT_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(true, true, false, false),
            std::time::Duration::from_secs(5)
        );
        assert_eq!(
            native_repaint_interval(true, false, true, false),
            CONNECT_POLL_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(true, false, false, true),
            UPDATE_CHECK_POLL_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(true, false, true, true),
            UPDATE_CHECK_POLL_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(false, false, true, false),
            CONNECT_POLL_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(false, false, false, true),
            UPDATE_CHECK_POLL_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(false, false, true, true),
            UPDATE_CHECK_POLL_INTERVAL
        );
    }

    #[test]
    fn non_macos_bluetooth_uses_normal_visible_idle_cadence() {
        assert_eq!(
            native_repaint_interval(
                false,
                should_use_high_frequency_bluetooth_repaint_for_target(true, false),
                false,
                false,
            ),
            VISIBLE_REPAINT_INTERVAL
        );
    }

    #[test]
    fn macos_bluetooth_keeps_continuous_visible_cadence() {
        assert_eq!(
            native_repaint_interval(
                false,
                should_use_high_frequency_bluetooth_repaint_for_target(true, true),
                false,
                false,
            ),
            BLUETOOTH_VISIBLE_REPAINT_INTERVAL
        );
    }
}
