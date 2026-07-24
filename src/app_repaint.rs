#[cfg(not(target_arch = "wasm32"))]
const VISIBLE_REPAINT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
#[cfg(not(target_arch = "wasm32"))]
const HIDDEN_TO_TRAY_REPAINT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
#[cfg(not(target_arch = "wasm32"))]
const BLUETOOTH_VISIBLE_REPAINT_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(16);
#[cfg(not(target_arch = "wasm32"))]
const WAYLAND_BLUETOOTH_VISIBLE_REPAINT_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(50);
#[cfg(not(target_arch = "wasm32"))]
pub(super) const CONNECT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
#[cfg(not(target_arch = "wasm32"))]
pub(super) const UPDATE_CHECK_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(100);

#[cfg(not(target_arch = "wasm32"))]
fn bluetooth_visible_repaint_interval_for_target(
    selected_device_is_bluetooth: bool,
    target_is_macos: bool,
    target_is_wayland: bool,
) -> Option<std::time::Duration> {
    if !selected_device_is_bluetooth {
        None
    } else if target_is_macos {
        Some(BLUETOOTH_VISIBLE_REPAINT_INTERVAL)
    } else if target_is_wayland {
        // On the tested Wayland stack, cursor motion does not reliably drive
        // another eframe pass. A 20 FPS heartbeat keeps hover, tooltips, and
        // hover-open menus live without restoring the saturating 60 FPS loop.
        Some(WAYLAND_BLUETOOTH_VISIBLE_REPAINT_INTERVAL)
    } else {
        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn bluetooth_visible_repaint_interval(
    selected_device_is_bluetooth: bool,
    target_is_wayland: bool,
) -> Option<std::time::Duration> {
    bluetooth_visible_repaint_interval_for_target(
        selected_device_is_bluetooth,
        cfg!(target_os = "macos"),
        target_is_wayland,
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn native_repaint_interval(
    hidden_to_tray: bool,
    bluetooth_visible_interval: Option<std::time::Duration>,
    connect_pending: bool,
    update_check_pending: bool,
) -> std::time::Duration {
    let baseline = if hidden_to_tray {
        HIDDEN_TO_TRAY_REPAINT_INTERVAL
    } else {
        bluetooth_visible_interval.unwrap_or(VISIBLE_REPAINT_INTERVAL)
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
            native_repaint_interval(false, None, false, false),
            std::time::Duration::from_millis(250)
        );
        assert_eq!(
            native_repaint_interval(
                false,
                Some(BLUETOOTH_VISIBLE_REPAINT_INTERVAL),
                false,
                false,
            ),
            BLUETOOTH_VISIBLE_REPAINT_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(true, Some(BLUETOOTH_VISIBLE_REPAINT_INTERVAL), false, false,),
            std::time::Duration::from_secs(5)
        );
        assert_eq!(
            native_repaint_interval(true, None, true, false),
            CONNECT_POLL_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(true, None, false, true),
            UPDATE_CHECK_POLL_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(true, None, true, true),
            UPDATE_CHECK_POLL_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(false, None, true, false),
            CONNECT_POLL_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(false, None, false, true),
            UPDATE_CHECK_POLL_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(false, None, true, true),
            UPDATE_CHECK_POLL_INTERVAL
        );
    }

    #[test]
    fn wayland_bluetooth_uses_a_bounded_visible_heartbeat() {
        assert_eq!(
            bluetooth_visible_repaint_interval_for_target(true, false, true),
            Some(WAYLAND_BLUETOOTH_VISIBLE_REPAINT_INTERVAL)
        );
        assert_eq!(
            native_repaint_interval(
                false,
                bluetooth_visible_repaint_interval_for_target(true, false, true),
                false,
                false,
            ),
            WAYLAND_BLUETOOTH_VISIBLE_REPAINT_INTERVAL
        );
    }

    #[test]
    fn event_driven_platforms_keep_the_normal_visible_idle_cadence() {
        assert_eq!(
            bluetooth_visible_repaint_interval_for_target(true, false, false),
            None
        );
        assert_eq!(
            native_repaint_interval(
                false,
                bluetooth_visible_repaint_interval_for_target(true, false, false),
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
                bluetooth_visible_repaint_interval_for_target(true, true, false),
                false,
                false,
            ),
            BLUETOOTH_VISIBLE_REPAINT_INTERVAL
        );
    }
}
