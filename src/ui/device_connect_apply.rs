use super::*;

fn is_default_layer_name(index: usize, name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.is_empty()
        || trimmed == index.to_string()
        || (index == 0 && trimmed.eq_ignore_ascii_case("main"))
        || trimmed.eq_ignore_ascii_case(&format!("layer {index}"))
}

/// Merge firmware-read layer names with locally saved ones.
///
/// A name the firmware actually stored (`from_firmware[i]`) is authoritative,
/// even when it looks like a default such as "Main". A locally saved name only
/// fills a layer the firmware left as a generated descriptor placeholder, so
/// provenance — not the string — decides. One real firmware name therefore no
/// longer suppresses saved names for the other layers.
fn resolve_layer_names(
    firmware_names: &[String],
    from_firmware: &[bool],
    local_names: Option<&[String]>,
    layer_count: usize,
) -> Vec<String> {
    let mut names: Vec<String> = firmware_names.to_vec();
    if names.len() < layer_count {
        let start = names.len();
        names.extend((start..layer_count).map(|layer| layer.to_string()));
    }
    names.truncate(layer_count);
    if let Some(local) = local_names {
        for (idx, name) in local.iter().enumerate().take(layer_count) {
            let from_firmware = from_firmware.get(idx).copied().unwrap_or(false);
            if !name.trim().is_empty() && !from_firmware && is_default_layer_name(idx, &names[idx])
            {
                names[idx] = name.clone();
            }
        }
    }
    names
}

fn connect_apply_start_log(
    device_name: &str,
    layer_count: usize,
    firmware: FirmwareProtocol,
) -> String {
    format!("Applying keyboard layout for {device_name} ({layer_count} layers, {firmware:?})")
}

fn connect_apply_error_log(error: &str) -> String {
    format!("Connect failed before applying keyboard layout: {error}")
}

fn is_hid_open_failure(error: &str) -> bool {
    error.starts_with("Open failed:")
}

#[cfg(not(target_arch = "wasm32"))]
enum ConnectTaskChannelState {
    Empty,
    Progress(String),
    Done(Box<Result<ConnectResult, String>>),
    Disconnected,
}

#[cfg(not(target_arch = "wasm32"))]
fn drain_connect_task_messages(rx: &mpsc::Receiver<ConnectTaskMessage>) -> ConnectTaskChannelState {
    let mut latest_progress = None;
    loop {
        match rx.try_recv() {
            Ok(ConnectTaskMessage::Progress(message)) => latest_progress = Some(message),
            Ok(ConnectTaskMessage::Done(result)) => {
                return ConnectTaskChannelState::Done(result);
            }
            Err(mpsc::TryRecvError::Empty) => {
                return latest_progress
                    .map(ConnectTaskChannelState::Progress)
                    .unwrap_or(ConnectTaskChannelState::Empty);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                return ConnectTaskChannelState::Disconnected;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests drive the real start/poll/clear paths. The per-app launch seam
    // replaces only transport execution, so no HID open or discovery is needed.
    fn connection_app() -> (
        EntropyApp,
        egui::Context,
        mpsc::Receiver<(Device, mpsc::Sender<ConnectTaskMessage>)>,
    ) {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&cc);
        let (tx, requests) = mpsc::channel();
        app.test_connect_requests = Some(tx);
        app.device_manager
            .replace_devices(vec![connection_device("A"), connection_device("B")]);
        (app, ctx, requests)
    }

    fn connection_device(name: &str) -> Device {
        Device {
            name: name.to_owned(),
            vendor_id: 0xFFFF,
            product_id: 0xFFFF,
            manufacturer: "Test".to_owned(),
            serial_number: name.to_owned(),
            bus_type: "Usb".to_owned(),
            path: format!("/nonexistent/entropy-test-{name}"),
            instance_token: name.to_owned(),
            firmware: FirmwareProtocol::Vial,
        }
    }

    fn connection_result(name: &str) -> ConnectResult {
        ConnectResult {
            device_name: name.to_owned(),
            keyboard_id: 0xDEAD165,
            vial_unlock_status: Default::default(),
            hid_device: Some(crate::hid::HidDevice::test_device().0),
            layout: KeyboardLayout::from_vial_json(&serde_json::json!({
                "name": name, "matrix": {"rows": 1, "cols": 1},
                "layouts": {"keymap": [["0,0"]]}
            }))
            .unwrap(),
            layer_count: 1,
            about_info: Default::default(),
            macro_texts: Default::default(),
            supports_macro_ext_keycodes: Default::default(),
            supports_rmk_native_key_actions: Default::default(),
            supports_universal_symbols: Default::default(),
            supports_universal_russian_letters: Default::default(),
            supports_rmk_native_combo_output: Default::default(),
            supports_rmk_native_tap_dance_actions: Default::default(),
            supports_rmk_combo_layers: Default::default(),
            macro_ext_keycodes_disabled_reason: Default::default(),
            tap_dance_entries: Default::default(),
            combo_entries: Default::default(),
            combo_term: Default::default(),
            auto_shift_options: Default::default(),
            auto_shift_timeout: Default::default(),
            mouse_keys_settings: Default::default(),
            touchpad_settings: Default::default(),
            bluetooth_settings: Default::default(),
            module_settings: Default::default(),
            tap_hold_settings: Default::default(),
            magic_settings: Default::default(),
            one_shot_settings: Default::default(),
            grave_escape_settings: Default::default(),
            layer_led_settings: Default::default(),
            rgb_settings: Default::default(),
            display_settings: Default::default(),
            layout_options_value: Default::default(),
            key_override_entries: Default::default(),
            alt_repeat_entries: Default::default(),
            vial_features: Default::default(),
            layer_names_from_firmware: Default::default(),
            supported_qmk_settings: Default::default(),
            deferred_load: Default::default(),
        }
    }

    fn expire_connection(app: &mut EntropyApp, idle: bool) {
        let ConnectState::Loading {
            started_at,
            last_progress_at,
            ..
        } = &mut app.connect_state
        else {
            panic!("expected loading worker");
        };
        let now = std::time::Instant::now();
        *started_at = now - std::time::Duration::from_secs(if idle { 46 } else { 241 });
        *last_progress_at = if idle { *started_at } else { now };
    }

    fn assert_no_applied_connection(app: &EntropyApp) {
        assert!(app.layout.is_none());
        assert!(app.hid_device.is_none());
        assert!(app.shared_hid_output.is_none());
        assert!(app.current_device_name.is_empty());
        assert!(app.current_keyboard_id.is_none());
        assert!(app.device_display_names.is_empty());
    }

    fn replacement_discards_result(
        result: Result<ConnectResult, String>,
        queued_before_selection: bool,
    ) {
        let (mut app, ctx, requests) = connection_app();
        app.start_connect(0);
        let (_, old_tx) = requests.try_recv().unwrap();
        let generation = app.connection_generation;
        let message = ConnectTaskMessage::Done(Box::new(result));
        if queued_before_selection {
            old_tx.send(message).unwrap();
            app.start_connect(1);
        } else {
            app.start_connect(1);
            old_tx.send(message).unwrap();
        }
        assert!(requests.try_recv().is_err(), "B started before A retired");
        app.poll_connect(&ctx);
        let (target, new_tx) = requests.try_recv().unwrap();
        assert_eq!(target.serial_number, "B");
        assert_eq!(app.selected_device, Some(1));
        assert!(app.pending_device_connect.is_none());
        assert_eq!(
            app.connection_generation, generation,
            "obsolete result was applied"
        );
        assert_no_applied_connection(&app);
        assert_eq!(app.status_msg, "Connecting to B (USB)…");
        assert!(matches!(app.connect_state, ConnectState::Loading { .. }));
        // Verify the current receiver remains authoritative after the replacement.
        new_tx
            .send(ConnectTaskMessage::Progress("B progress".to_owned()))
            .unwrap();
        app.poll_connect(&ctx);
        assert_eq!(app.status_msg, "B progress");
    }

    #[test]
    fn replacement_rejects_success_already_queued_before_selection() {
        replacement_discards_result(Ok(connection_result("Obsolete Keyboard A")), true);
    }

    #[test]
    fn replacement_rejects_success_after_last_worker_cancellation_checkpoint() {
        replacement_discards_result(Ok(connection_result("Obsolete Keyboard A")), false);
    }

    #[test]
    fn replacement_survives_old_disconnect_error() {
        replacement_discards_result(
            Err("VIA protocol read failed: HID timeout — device did not respond".to_owned()),
            true,
        );
    }

    #[test]
    fn replacement_survives_old_wrapped_cancellation_error() {
        replacement_discards_result(
            Err("Layout read failed: Connect cancelled".to_owned()),
            true,
        );
    }

    #[test]
    fn replacement_survives_other_obsolete_error() {
        replacement_discards_result(Err("Layout parse failed: invalid matrix".to_owned()), false);
    }

    #[test]
    fn replacement_survives_worker_channel_disconnect() {
        let (mut app, ctx, requests) = connection_app();
        app.start_connect(0);
        let (_, tx) = requests.try_recv().unwrap();
        app.start_connect(1);
        drop(tx);
        app.poll_connect(&ctx);
        assert_eq!(requests.try_recv().unwrap().0.serial_number, "B");
        assert_eq!(app.selected_device, Some(1));
        assert_no_applied_connection(&app);
    }

    #[test]
    fn obsolete_progress_neither_overwrites_status_nor_extends_deadline() {
        let (mut app, ctx, requests) = connection_app();
        app.start_connect(0);
        let (_, tx) = requests.try_recv().unwrap();
        app.start_connect(1);
        let before = match &app.connect_state {
            ConnectState::Loading {
                last_progress_at, ..
            } => *last_progress_at,
            _ => panic!("expected loading"),
        };
        let status = app.status_msg.clone();
        tx.send(ConnectTaskMessage::Progress(
            "obsolete A progress".to_owned(),
        ))
        .unwrap();
        app.poll_connect(&ctx);
        assert_eq!(app.status_msg, status);
        assert!(
            matches!(app.connect_state, ConnectState::Loading { last_progress_at, .. } if last_progress_at == before)
        );
        assert!(requests.try_recv().is_err());
    }

    fn timeout_releases_owner(idle: bool, queued_progress: bool) {
        let (mut app, ctx, requests) = connection_app();
        app.start_connect(0);
        let (_, tx) = requests.try_recv().unwrap();
        let cancel = match &app.connect_state {
            ConnectState::Loading { cancel, .. } => cancel.clone(),
            _ => panic!("expected loading"),
        };
        expire_connection(&mut app, idle);
        if queued_progress {
            tx.send(ConnectTaskMessage::Progress("still working".to_owned()))
                .unwrap();
        }
        let generation = app.connection_generation;
        app.poll_connect(&ctx);
        assert!(matches!(app.connect_state, ConnectState::Idle));
        assert!(!app.retiring_connects.is_empty());
        assert!(cancel.load(std::sync::atomic::Ordering::Relaxed));
        assert_eq!(app.connection_generation, generation.wrapping_add(1));
        assert!(app.status_msg.starts_with("Connect timeout"));
        assert_eq!(app.selected_device, None);
        assert_no_applied_connection(&app);
        // The sender remains alive and silent: UI recovery needs no worker cooperation.
        for _ in 0..3 {
            app.poll_connect(&ctx);
            assert!(!matches!(app.connect_state, ConnectState::Loading { .. }));
            assert!(requests.try_recv().is_err());
        }
        drop(tx);
        app.poll_connect(&ctx);
        assert!(app.retiring_connects.is_empty());
    }

    #[test]
    fn idle_timeout_releases_ui_owner_without_worker_cooperation() {
        timeout_releases_owner(true, false);
    }

    #[test]
    fn total_timeout_releases_ui_owner_despite_fresh_progress() {
        timeout_releases_owner(false, true);
    }

    #[test]
    fn timeout_allows_different_device_to_connect_while_old_worker_remains_unresponsive() {
        let (mut app, ctx, requests) = connection_app();
        app.start_connect(0);
        let (_, a_tx) = requests.try_recv().unwrap();
        expire_connection(&mut app, true);
        app.poll_connect(&ctx);
        assert!(!matches!(app.connect_state, ConnectState::Loading { .. }));

        // A remains alive, silent and unable to observe cancellation. Recovery
        // must actually start and apply B, not only hide A's loading screen.
        app.start_connect(1);
        app.poll_connect(&ctx);
        let (target, b_tx) = requests
            .try_recv()
            .expect("P2: different device B must start after A times out, without A completing");
        assert_eq!(target.serial_number, "B");
        b_tx.send(ConnectTaskMessage::Done(Box::new(Ok(connection_result(
            "Current B",
        )))))
        .unwrap();
        app.poll_connect(&ctx);
        assert_eq!(app.selected_device, Some(1));
        assert_eq!(app.current_device_name, "Current B");
        assert_eq!(app.layout.as_ref().unwrap().name, "Current B");
        assert!(app.hid_device.is_some());

        // Only now may A finish; even a successful late result cannot replace B.
        a_tx.send(ConnectTaskMessage::Done(Box::new(Ok(connection_result(
            "Late A",
        )))))
        .unwrap();
        app.poll_connect(&ctx);
        assert_eq!(app.selected_device, Some(1));
        assert_eq!(app.current_device_name, "Current B");
        assert_eq!(app.layout.as_ref().unwrap().name, "Current B");
        assert!(app.hid_device.is_some());
    }

    #[test]
    fn queued_success_past_timeout_is_never_applied() {
        let (mut app, ctx, requests) = connection_app();
        app.start_connect(0);
        let (_, tx) = requests.try_recv().unwrap();
        expire_connection(&mut app, true);
        tx.send(ConnectTaskMessage::Done(Box::new(Ok(connection_result(
            "Expired A",
        )))))
        .unwrap();
        app.poll_connect(&ctx);
        app.poll_connect(&ctx);
        assert_no_applied_connection(&app);
        assert!(app.retiring_connects.is_empty());
        assert!(app.status_msg.starts_with("Connect timeout"));
    }

    fn late_timeout_result_is_inert(result: Result<ConnectResult, String>) {
        let (mut app, ctx, requests) = connection_app();
        app.start_connect(0);
        let (_, tx) = requests.try_recv().unwrap();
        app.start_connect(1);
        expire_connection(&mut app, true);
        app.poll_connect(&ctx);
        assert_eq!(app.retiring_connects.len(), 1);
        let (target, b_tx) = requests.try_recv().expect("queued B resumes at A timeout");
        assert_eq!(target.serial_number, "B");
        assert!(app.pending_device_connect.is_none());
        b_tx.send(ConnectTaskMessage::Progress("B progress".to_owned()))
            .unwrap();
        for _ in 0..3 {
            app.poll_connect(&ctx);
            assert!(
                requests.try_recv().is_err(),
                "spawned extra connection worker"
            );
            assert!(matches!(app.connect_state, ConnectState::Loading { .. }));
            assert_eq!(app.status_msg, "B progress");
        }
        tx.send(ConnectTaskMessage::Done(Box::new(result))).unwrap();
        app.poll_connect(&ctx);
        assert_eq!(app.selected_device, Some(1));
        assert_eq!(app.status_msg, "B progress");
        assert_no_applied_connection(&app);
        assert!(app.retiring_connects.is_empty());
        assert!(requests.try_recv().is_err());
        assert!(tx
            .send(ConnectTaskMessage::Progress("too late".to_owned()))
            .is_err());
        b_tx.send(ConnectTaskMessage::Done(Box::new(Ok(connection_result(
            "Current B",
        )))))
        .unwrap();
        app.poll_connect(&ctx);
        assert_eq!(app.current_device_name, "Current B");
        assert!(app.hid_device.is_some());
    }

    #[test]
    fn late_success_after_timeout_cannot_replace_the_loading_target() {
        late_timeout_result_is_inert(Ok(connection_result("Late A")));
    }

    #[test]
    fn late_disconnect_after_timeout_cannot_clear_replacement() {
        late_timeout_result_is_inert(Err(
            "VIA protocol read failed: HID timeout — device did not respond".to_owned(),
        ));
    }

    #[test]
    fn clearing_two_workers_keeps_same_endpoint_reserved_until_its_owner_retires() {
        let (mut app, ctx, requests) = connection_app();
        app.start_connect(0);
        let (_, a_tx) = requests.try_recv().unwrap();
        app.clear_connected_keyboard_state("");
        app.start_connect(1);
        let (_, b_tx) = requests.try_recv().unwrap();
        app.clear_connected_keyboard_state("");
        assert_eq!(app.retiring_connects.len(), MAX_CONNECT_WORKERS);
        app.start_connect(1);
        assert!(requests.try_recv().is_err());
        a_tx.send(ConnectTaskMessage::Done(Box::new(Ok(connection_result(
            "Cleared A",
        )))))
        .unwrap();
        app.poll_connect(&ctx);
        assert!(
            requests.try_recv().is_err(),
            "B still owns its endpoint despite free capacity"
        );
        assert_eq!(app.retiring_connects.len(), 1);
        assert_no_applied_connection(&app);
        b_tx.send(ConnectTaskMessage::Done(Box::new(Ok(connection_result(
            "Cleared B",
        )))))
        .unwrap();
        app.poll_connect(&ctx);
        assert_eq!(requests.try_recv().unwrap().0.serial_number, "B");
        assert_no_applied_connection(&app);
    }

    #[test]
    fn repeated_timeouts_and_new_selections_never_exceed_two_connection_workers() {
        let (mut app, ctx, requests) = connection_app();
        app.device_manager.replace_devices(vec![
            connection_device("A"),
            connection_device("B"),
            connection_device("C"),
        ]);
        app.start_connect(0);
        let (_, a_tx) = requests.try_recv().unwrap();
        expire_connection(&mut app, true);
        app.poll_connect(&ctx);
        app.start_connect(1);
        let (_, b_tx) = requests.try_recv().unwrap();
        expire_connection(&mut app, true);
        app.poll_connect(&ctx);
        for _ in 0..10 {
            app.start_connect(2);
            app.poll_connect(&ctx);
            assert_eq!(app.retiring_connects.len(), MAX_CONNECT_WORKERS);
            assert!(!matches!(app.connect_state, ConnectState::Loading { .. }));
            assert!(requests.try_recv().is_err());
        }
        // Reclaim exactly one slot, keeping B blocked and reserved.
        drop(a_tx);
        app.poll_connect(&ctx);
        let (target, _c_tx) = requests.try_recv().unwrap();
        assert_eq!(target.serial_number, "C");
        assert_eq!(app.retiring_connects.len(), 1);
        assert!(matches!(app.connect_state, ConnectState::Loading { .. }));
        drop(b_tx);
    }

    #[test]
    fn timeout_does_not_release_same_device_reservation_after_path_change() {
        let (mut app, ctx, requests) = connection_app();
        app.start_connect(0);
        let (_, a_tx) = requests.try_recv().unwrap();
        expire_connection(&mut app, true);
        app.poll_connect(&ctx);
        let mut reenumerated = connection_device("A");
        reenumerated.path = "/nonexistent/entropy-test-new-path".to_owned();
        reenumerated.instance_token = "new-instance".to_owned();
        app.device_manager.replace_devices(vec![reenumerated]);
        app.start_connect(0);
        app.poll_connect(&ctx);
        assert!(requests.try_recv().is_err());
        assert_eq!(app.retiring_connects.len(), 1);
        drop(a_tx);
        app.poll_connect(&ctx);
        assert_eq!(requests.try_recv().unwrap().0.serial_number, "A");
    }

    #[test]
    fn ambiguous_serial_less_endpoint_stays_reserved_after_timeout() {
        let (mut app, ctx, requests) = connection_app();
        let mut a = connection_device("Same model");
        a.serial_number.clear();
        let mut ambiguous = a.clone();
        ambiguous.path = "/nonexistent/other-endpoint".to_owned();
        ambiguous.instance_token = "other-instance".to_owned();
        app.device_manager.replace_devices(vec![a, ambiguous]);
        app.start_connect(0);
        let (_, _a_tx) = requests.try_recv().unwrap();
        expire_connection(&mut app, true);
        app.poll_connect(&ctx);
        app.start_connect(1);
        app.poll_connect(&ctx);
        assert!(requests.try_recv().is_err());
        assert_eq!(app.retiring_connects.len(), 1);
        assert!(!matches!(app.connect_state, ConnectState::Loading { .. }));
    }

    #[test]
    fn blocked_retired_endpoint_request_does_not_relabel_a_live_different_keyboard() {
        let (mut app, ctx, requests) = connection_app();
        app.start_connect(0);
        let (_, a_tx) = requests.try_recv().unwrap();
        expire_connection(&mut app, true);
        app.poll_connect(&ctx);
        app.start_connect(1);
        let (_, b_tx) = requests.try_recv().unwrap();
        b_tx.send(ConnectTaskMessage::Done(Box::new(Ok(connection_result(
            "Current B",
        )))))
        .unwrap();
        app.poll_connect(&ctx);

        // A is still reserved. Keep B selected while A is merely queued; edits
        // must never go to B under A's identity/name cache or selected index.
        app.start_connect(0);
        assert_eq!(app.selected_device, Some(1));
        assert_eq!(
            app.pending_device_connect,
            Some(connection_device("A").stable_identity())
        );
        assert_eq!(app.current_device_name, "Current B");
        assert_eq!(app.layout.as_ref().unwrap().name, "Current B");
        assert!(app.hid_device.is_some());
        assert!(requests.try_recv().is_err());

        let (scan_tx, scan_rx) = mpsc::channel();
        app.device_scan_state = DeviceScanState::Scanning {
            rx: scan_rx,
            started_at: std::time::Instant::now(),
            generation: app.connection_generation,
            timeout_logged: false,
        };
        scan_tx
            .send(Ok(vec![connection_device("B"), connection_device("A")]))
            .unwrap();
        app.poll_device_scan(&ctx);
        assert_eq!(app.selected_device, Some(0));
        assert_eq!(app.current_device_name, "Current B");
        assert!(app.hid_device.is_some());
        assert!(requests.try_recv().is_err());

        drop(a_tx);
        app.poll_connect(&ctx);
        let (target, _new_a_tx) = requests.try_recv().unwrap();
        assert_eq!(target.serial_number, "A");
        assert_eq!(app.selected_device, Some(1));
        assert!(app.hid_device.is_none());
        assert!(app.layout.is_none());
    }

    #[test]
    fn consumer_rejects_completion_when_selected_identity_no_longer_matches_owner() {
        let (mut app, ctx, requests) = connection_app();
        app.start_connect(0);
        let (_, a_tx) = requests.try_recv().unwrap();
        // Simulate an inconsistent selection independently of cooperative cancel.
        app.selected_device = Some(1);
        let (_scan_tx, scan_rx) = mpsc::channel();
        app.device_scan_state = DeviceScanState::Scanning {
            rx: scan_rx,
            started_at: std::time::Instant::now(),
            generation: app.connection_generation,
            timeout_logged: false,
        };
        a_tx.send(ConnectTaskMessage::Done(Box::new(Ok(connection_result(
            "Wrong A",
        )))))
        .unwrap();
        app.poll_connect(&ctx);
        assert_no_applied_connection(&app);
        assert_eq!(app.selected_device, None);
    }

    #[test]
    fn resumed_identity_updates_selected_index_after_device_reordering() {
        let (mut app, ctx, requests) = connection_app();
        app.start_connect(0);
        let (_, tx) = requests.try_recv().unwrap();
        app.start_connect(1);
        app.device_manager
            .replace_devices(vec![connection_device("B"), connection_device("A")]);
        tx.send(ConnectTaskMessage::Done(Box::new(Err(
            "Connect cancelled".to_owned()
        ))))
        .unwrap();
        app.poll_connect(&ctx);
        assert_eq!(requests.try_recv().unwrap().0.serial_number, "B");
        assert_eq!(app.selected_device, Some(0));
    }

    #[test]
    fn replacement_success_applies_only_the_selected_targets_layout_hid_and_name_cache() {
        let (mut app, ctx, requests) = connection_app();
        app.start_connect(0);
        let (_, a_tx) = requests.try_recv().unwrap();
        a_tx.send(ConnectTaskMessage::Done(Box::new(Ok(connection_result(
            "Obsolete A",
        )))))
        .unwrap();
        app.start_connect(1);
        app.poll_connect(&ctx);
        let (_, b_tx) = requests.try_recv().unwrap();
        b_tx.send(ConnectTaskMessage::Done(Box::new(Ok(connection_result(
            "Current B",
        )))))
        .unwrap();
        app.poll_connect(&ctx);
        assert_eq!(app.selected_device, Some(1));
        assert_eq!(app.current_device_name, "Current B");
        assert_eq!(app.layout.as_ref().unwrap().name, "Current B");
        assert!(app.hid_device.is_some());
        assert_eq!(
            app.device_display_names
                .get(&connection_device("B").display_name_cache_key())
                .map(String::as_str),
            Some("Current B")
        );
        assert!(!app
            .device_display_names
            .contains_key(&connection_device("A").display_name_cache_key()));
        assert!(app.pending_device_connect.is_none());
        assert!(matches!(app.connect_state, ConnectState::Idle));
    }

    #[test]
    fn bluetooth_timeout_preserves_snapshot_and_serializes_retry() {
        let (mut app, ctx, requests) = connection_app();
        let mut device = connection_device("A");
        device.bus_type = "Bluetooth".to_owned();
        let reconnect =
            BluetoothReconnectState::new(device.stable_identity(), "A (Bluetooth)".to_owned());
        app.device_manager.replace_devices(vec![device]);
        app.layout = Some(connection_result("A snapshot").layout);
        app.start_reconnect_connect(0, reconnect.clone());
        let (_, tx) = requests.try_recv().unwrap();
        expire_connection(&mut app, true);
        app.poll_connect(&ctx);
        assert!(matches!(app.connect_state, ConnectState::Reconnecting(_)));
        assert!(!app.retiring_connects.is_empty());
        assert_eq!(app.layout.as_ref().unwrap().name, "A snapshot");
        app.start_reconnect_connect(0, reconnect.clone());
        assert!(requests.try_recv().is_err());
        tx.send(ConnectTaskMessage::Done(Box::new(Ok(connection_result(
            "Late A",
        )))))
        .unwrap();
        app.poll_connect(&ctx);
        assert_eq!(app.layout.as_ref().unwrap().name, "A snapshot");
        assert!(app.hid_device.is_none());
        app.start_reconnect_connect(0, reconnect);
        assert_eq!(requests.try_recv().unwrap().0.serial_number, "A");
    }

    #[test]
    fn manual_selection_during_retired_bluetooth_attempt_supersedes_old_retry() {
        let (mut app, ctx, requests) = connection_app();
        let reconnect = BluetoothReconnectState::new(
            connection_device("A").stable_identity(),
            "A (Bluetooth)".to_owned(),
        );
        app.layout = Some(connection_result("A snapshot").layout);
        app.start_reconnect_connect(0, reconnect);
        let (_, tx) = requests.try_recv().unwrap();
        expire_connection(&mut app, true);
        app.poll_connect(&ctx);
        assert!(matches!(app.connect_state, ConnectState::Reconnecting(_)));
        app.start_connect(1);
        assert!(!app.bluetooth_reconnect_active());
        assert!(app.layout.is_none());
        assert!(app.hid_device.is_none());
        assert_eq!(app.selected_device, Some(1));
        assert!(app.pending_device_connect.is_none());
        let (target, _b_tx) = requests.try_recv().unwrap();
        assert_eq!(target.serial_number, "B");
        drop(tx);
        app.poll_connect(&ctx);
        assert!(requests.try_recv().is_err());
        assert_eq!(app.selected_device, Some(1));
    }

    #[test]
    fn latest_replacement_identity_wins_without_parallel_workers() {
        let (mut app, ctx, requests) = connection_app();
        app.start_connect(0);
        let (_, tx) = requests.try_recv().unwrap();
        app.start_connect(1);
        app.start_connect(0);
        assert!(requests.try_recv().is_err());
        tx.send(ConnectTaskMessage::Done(Box::new(Ok(connection_result(
            "Obsolete first A",
        )))))
        .unwrap();
        app.poll_connect(&ctx);
        assert_eq!(requests.try_recv().unwrap().0.serial_number, "A");
        assert_eq!(app.selected_device, Some(0));
        assert_no_applied_connection(&app);
    }

    #[test]
    fn connect_apply_start_log_includes_device_layer_count_and_firmware() {
        assert_eq!(
            connect_apply_start_log("HPD2", 4, FirmwareProtocol::Vial),
            "Applying keyboard layout for HPD2 (4 layers, Vial)"
        );
    }

    #[test]
    fn connect_apply_error_log_includes_context() {
        assert_eq!(
            connect_apply_error_log("Layout parse failed: missing matrix"),
            "Connect failed before applying keyboard layout: Layout parse failed: missing matrix"
        );
    }

    #[test]
    fn detects_hid_open_failure_status() {
        assert!(is_hid_open_failure(
            "Open failed: Failed to open HID device"
        ));
        assert!(!is_hid_open_failure("Layout parse failed: missing matrix"));
    }

    #[test]
    fn fresh_pairing_timeout_is_classified_as_a_disconnect() {
        let error = "VIA protocol read failed: HID notification probe failed: \
            HID timeout — device did not respond; Get Input Report fallback failed: \
            hidapi error: ioctl (GINPUT): EIO: I/O error";

        assert!(crate::hid::is_disconnect_error_message(error));
    }

    #[test]
    fn empty_connect_poll_is_throttled() {
        assert_eq!(CONNECT_POLL_INTERVAL, std::time::Duration::from_millis(250));
    }

    #[test]
    fn connect_progress_queue_keeps_only_the_latest_message() {
        let (tx, rx) = mpsc::channel();
        tx.send(ConnectTaskMessage::Progress("first".to_owned()))
            .unwrap();
        tx.send(ConnectTaskMessage::Progress("latest".to_owned()))
            .unwrap();

        assert!(matches!(
            drain_connect_task_messages(&rx),
            ConnectTaskChannelState::Progress(message) if message == "latest"
        ));
    }

    #[test]
    fn connect_done_bypasses_queued_progress_messages() {
        let (tx, rx) = mpsc::channel();
        tx.send(ConnectTaskMessage::Progress("first".to_owned()))
            .unwrap();
        tx.send(ConnectTaskMessage::Progress("latest".to_owned()))
            .unwrap();
        tx.send(ConnectTaskMessage::Done(Box::new(Err(
            "finished".to_owned()
        ))))
        .unwrap();

        assert!(matches!(
            drain_connect_task_messages(&rx),
            ConnectTaskChannelState::Done(result)
                if matches!(*result, Err(ref error) if error == "finished")
        ));
    }

    fn s(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn resolve_prefers_firmware_names_even_when_default_looking() {
        // Firmware stored "Main" for layer 0; a stale local name must not win.
        let names = resolve_layer_names(
            &s(&["Main", "1"]),
            &[true, false],
            Some(&s(&["OLD", "LOWER"])),
            2,
        );
        assert_eq!(names, s(&["Main", "LOWER"]));
    }

    #[test]
    fn resolve_fills_placeholder_layers_from_local() {
        // Firmware provided nothing (all placeholders); local names fill them.
        let names = resolve_layer_names(
            &s(&["0", "1", "2"]),
            &[false, false, false],
            Some(&s(&["BASE", "LOWER", ""])),
            3,
        );
        // Empty local name leaves the placeholder untouched.
        assert_eq!(names, s(&["BASE", "LOWER", "2"]));
    }

    #[test]
    fn resolve_mixed_firmware_and_local() {
        // Layer 0 real firmware name kept; layers 1/2 placeholders filled locally.
        let names = resolve_layer_names(
            &s(&["BASE", "1", "2"]),
            &[true, false, false],
            Some(&s(&["X", "RAISE", "ADJUST"])),
            3,
        );
        assert_eq!(names, s(&["BASE", "RAISE", "ADJUST"]));
    }

    #[test]
    fn resolve_extends_and_truncates_to_layer_count() {
        let names = resolve_layer_names(&s(&["BASE"]), &[true], None, 3);
        assert_eq!(names, s(&["BASE", "1", "2"]));
        let names = resolve_layer_names(&s(&["A", "B", "C"]), &[true, true, true], None, 2);
        assert_eq!(names, s(&["A", "B"]));
    }

    use std::cell::RefCell;
    use std::collections::{BTreeSet, HashMap};

    /// In-memory [`LayerNameStore`] standing in for the HID device so the
    /// production sync path can be tested without hardware. Records every read
    /// and write and lets a test inject transport errors per qsid.
    struct MockStore {
        supported: BTreeSet<u16>,
        stored: RefCell<HashMap<u16, String>>,
        read_errors: BTreeSet<u16>,
        write_errors: BTreeSet<u16>,
        reads: RefCell<Vec<u16>>,
        writes: RefCell<Vec<(u16, String)>>,
    }

    impl MockStore {
        fn new(supported: &[u16]) -> Self {
            MockStore {
                supported: supported.iter().copied().collect(),
                stored: RefCell::new(HashMap::new()),
                read_errors: BTreeSet::new(),
                write_errors: BTreeSet::new(),
                reads: RefCell::new(Vec::new()),
                writes: RefCell::new(Vec::new()),
            }
        }

        fn with_stored(mut self, qsid: u16, value: &str) -> Self {
            self.stored.get_mut().insert(qsid, value.to_string());
            self
        }

        fn failing_reads(mut self, qsids: &[u16]) -> Self {
            self.read_errors = qsids.iter().copied().collect();
            self
        }

        fn failing_writes(mut self, qsids: &[u16]) -> Self {
            self.write_errors = qsids.iter().copied().collect();
            self
        }
    }

    impl LayerNameStore for MockStore {
        fn is_supported(&self, qsid: u16) -> bool {
            self.supported.contains(&qsid)
        }

        fn get_string(&self, qsid: u16) -> anyhow::Result<String> {
            self.reads.borrow_mut().push(qsid);
            if self.read_errors.contains(&qsid) {
                anyhow::bail!("read transport error on qsid {qsid}");
            }
            Ok(self.stored.borrow().get(&qsid).cloned().unwrap_or_default())
        }

        fn set_string(&self, qsid: u16, value: &str) -> anyhow::Result<()> {
            self.writes.borrow_mut().push((qsid, value.to_string()));
            if self.write_errors.contains(&qsid) {
                anyhow::bail!("write transport error on qsid {qsid}");
            }
            self.stored.borrow_mut().insert(qsid, value.to_string());
            Ok(())
        }
    }

    #[test]
    fn sync_unsupported_storage_is_not_a_failure_and_never_touches_the_wire() {
        // Firmware advertises no layer-name settings at all.
        let store = MockStore::new(&[]);
        let failed = sync_layer_names_to_store(&store, &s(&["BASE", "LOWER"]), 2);
        assert!(failed.is_empty());
        assert!(store.reads.borrow().is_empty());
        assert!(store.writes.borrow().is_empty());
    }

    #[test]
    fn sync_transient_first_layer_read_failure_is_reported_not_swallowed() {
        // Regression: a read error on layer 0 used to be treated as "storage
        // unsupported", aborting the sync and reporting success. Now the layer
        // is reported failed and later layers still get written.
        let store = MockStore::new(&[200, 201]).failing_reads(&[200]);
        let failed = sync_layer_names_to_store(&store, &s(&["BASE", "LOWER"]), 2);
        assert_eq!(failed, vec![0]);
        // Layer 1 was still read and written back.
        assert_eq!(
            store.writes.borrow().as_slice(),
            &[(201, "LOWER".to_string())]
        );
        assert_eq!(
            store.stored.borrow().get(&201).map(String::as_str),
            Some("LOWER")
        );
    }

    #[test]
    fn sync_middle_layer_read_failure_only_fails_that_layer() {
        let store = MockStore::new(&[200, 201, 202]).failing_reads(&[201]);
        let failed = sync_layer_names_to_store(&store, &s(&["BASE", "LOWER", "RAISE"]), 3);
        assert_eq!(failed, vec![1]);
        assert_eq!(
            store.stored.borrow().get(&200).map(String::as_str),
            Some("BASE")
        );
        assert_eq!(
            store.stored.borrow().get(&202).map(String::as_str),
            Some("RAISE")
        );
    }

    #[test]
    fn sync_set_failure_reports_layer_but_continues() {
        let store = MockStore::new(&[200, 201]).failing_writes(&[200]);
        let failed = sync_layer_names_to_store(&store, &s(&["BASE", "LOWER"]), 2);
        assert_eq!(failed, vec![0]);
        // The later layer still persisted despite the earlier SET error.
        assert_eq!(
            store.stored.borrow().get(&201).map(String::as_str),
            Some("LOWER")
        );
    }

    #[test]
    fn sync_skips_matching_and_empty_names() {
        let store = MockStore::new(&[200, 201]).with_stored(201, "LOWER");
        // Layer 0 empty (skipped, no read); layer 1 already matches (no write).
        let failed = sync_layer_names_to_store(&store, &s(&["", "LOWER"]), 2);
        assert!(failed.is_empty());
        assert_eq!(store.reads.borrow().as_slice(), &[201]);
        assert!(store.writes.borrow().is_empty());
    }

    #[test]
    fn sync_persists_then_is_idempotent_on_reconnect() {
        let names = s(&["BASE", "LOWER"]);
        let store = MockStore::new(&[200, 201]);
        // First sync writes both names.
        let failed = sync_layer_names_to_store(&store, &names, 2);
        assert!(failed.is_empty());
        assert_eq!(store.writes.borrow().len(), 2);
        // A second sync (as after a reconnect) reads them back and writes nothing.
        store.writes.borrow_mut().clear();
        let failed = sync_layer_names_to_store(&store, &names, 2);
        assert!(failed.is_empty());
        assert!(store.writes.borrow().is_empty());
    }
}

impl EntropyApp {
    /// Revoke UI ownership without losing the serialization fence for a worker
    /// that may be unable to observe cancellation inside synchronous HID I/O.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn retire_connect_worker(&mut self) {
        if !matches!(self.connect_state, ConnectState::Loading { .. }) {
            return;
        }
        if let ConnectState::Loading {
            device, rx, cancel, ..
        } = std::mem::replace(&mut self.connect_state, ConnectState::Idle)
        {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            debug_assert!(self.retiring_connects.len() < MAX_CONNECT_WORKERS);
            self.retiring_connects.push(RetiringConnect { device, rx });
        }
    }

    /// Poll background thread for connect result.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_connect(&mut self, ctx: &egui::Context) {
        const CONNECT_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);
        const CONNECT_TOTAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(240);

        // Retired receivers only release endpoint reservations. Poll them without
        // blocking the current owner: B must progress while a distinct A is stuck.
        let retired_before = self.retiring_connects.len();
        self.retiring_connects
            .retain(|retiring| match drain_connect_task_messages(&retiring.rx) {
                ConnectTaskChannelState::Done(result) => {
                    drop(result);
                    false
                }
                ConnectTaskChannelState::Disconnected => false,
                ConnectTaskChannelState::Progress(_) | ConnectTaskChannelState::Empty => true,
            });
        if !self.retiring_connects.is_empty() {
            ctx.request_repaint_after(CONNECT_POLL_INTERVAL);
        }
        if self.retiring_connects.len() != retired_before {
            self.resume_pending_device_connect();
        }

        // Check deadlines before messages: even a stream of progress or a Done
        // already queued after the deadline must not revive expired ownership.
        if let ConnectState::Loading {
            started_at,
            last_progress_at,
            reconnect,
            ..
        } = &self.connect_state
        {
            if last_progress_at.elapsed() > CONNECT_IDLE_TIMEOUT
                || started_at.elapsed() > CONNECT_TOTAL_TIMEOUT
            {
                let stage = if self.status_msg.is_empty() {
                    "unknown stage".to_owned()
                } else {
                    self.status_msg.clone()
                };
                let error = format!(
                    "Connect timeout — RMK/Vial device did not finish loading while: {stage}"
                );
                log::warn!("{error}; retiring worker");
                let reconnect = reconnect.clone();
                let pending = self.pending_device_connect.take();
                if let Some(reconnect) = reconnect.filter(|_| pending.is_none()) {
                    // Keep the existing Bluetooth snapshot/retry path, but revoke
                    // this attempt and fence transport reuse until it retires.
                    self.retire_connect_worker();
                    self.connection_generation = self.connection_generation.wrapping_add(1);
                    self.schedule_bluetooth_reconnect_retry(reconnect, &error);
                } else {
                    self.clear_connected_keyboard_state(error);
                    self.pending_device_connect = pending;
                    if self.pending_device_connect.is_none() {
                        self.selected_device = None;
                    } else {
                        self.resume_pending_device_connect();
                    }
                }
                ctx.request_repaint_after(CONNECT_POLL_INTERVAL);
                return;
            }
        }

        let (result, reconnect, obsolete) = match &mut self.connect_state {
            ConnectState::Loading {
                device,
                rx,
                last_progress_at,
                cancel,
                reconnect,
                ..
            } => {
                // Cancellation is authoritative at the consumer, not just at
                // cooperative worker checkpoints. Done can predate selection B.
                let selected_matches_owner = self
                    .selected_device
                    .and_then(|index| self.device_manager.devices().get(index))
                    .is_some_and(|selected| device.stable_identity().matches(selected));
                let obsolete = cancel.load(std::sync::atomic::Ordering::Relaxed)
                    || self.pending_device_connect.is_some()
                    || !selected_matches_owner;
                let result = match drain_connect_task_messages(rx) {
                    ConnectTaskChannelState::Progress(message) => {
                        if !obsolete && reconnect.is_none() {
                            self.status_msg = message;
                        }
                        // Obsolete progress must not extend cancellation recovery.
                        if !obsolete {
                            *last_progress_at = std::time::Instant::now();
                        }
                        ctx.request_repaint_after(CONNECT_POLL_INTERVAL);
                        return;
                    }
                    ConnectTaskChannelState::Done(result) => *result,
                    ConnectTaskChannelState::Empty => {
                        #[cfg(not(target_os = "windows"))]
                        ctx.request_repaint_after(CONNECT_POLL_INTERVAL);
                        return;
                    }
                    ConnectTaskChannelState::Disconnected => Err("Connect thread died".to_owned()),
                };
                (result, reconnect.clone(), obsolete)
            }
            ConnectState::Idle | ConnectState::SelectingDevice | ConnectState::Reconnecting(_) => {
                return;
            }
        };

        self.connect_state = ConnectState::Idle;
        ctx.request_repaint();
        if obsolete {
            // Drop A's HID handle before allowing B to start. Never classify an
            // obsolete error (including wrapped cancellation) as B's failure.
            drop(result);
            if self.pending_device_connect.is_some() {
                self.resume_pending_device_connect();
            } else if let Some(reconnect) = reconnect {
                self.schedule_bluetooth_reconnect_retry(reconnect, "connect cancelled");
            } else {
                self.selected_device = None;
                self.start_device_scan();
            }
            return;
        }

        match result {
            Ok(mut r) => {
                if let Some(reconnect) = reconnect.as_ref().filter(|_| self.layout.is_some()) {
                    self.preserve_deferred_snapshot_on_reconnect(&mut r, reconnect);
                }
                let staged_bluetooth_load = r.deferred_load.is_staged();
                self.pending_tap_hold_numeric_writes.clear();
                self.tap_hold_numeric_write_due = None;
                log::info!(
                    "{}",
                    connect_apply_start_log(&r.device_name, r.layer_count, r.layout.firmware)
                );
                // A new device is now active; invalidate any file dialog that was
                // opened against the previous connection.
                self.connection_generation = self.connection_generation.wrapping_add(1);
                self.layer_count = r.layer_count;
                self.firmware = r.layout.firmware;
                self.current_device_name = r.device_name.clone();
                self.current_keyboard_id = Some(r.keyboard_id);
                self.vial_unlock_is_rmk = None;
                self.vial_unlock_rmk_hold_started_at = None;
                match &r.vial_unlock_status {
                    Some((unlocked, keys)) => {
                        self.vial_unlocked = Some(*unlocked);
                        self.vial_unlock_keys = keys.clone();
                    }
                    None => {
                        self.vial_unlocked = None;
                        self.vial_unlock_keys.clear();
                    }
                }
                crate::app::ensure_firmware_update_check(
                    &mut self.firmware_update_check,
                    r.about_info.firmware_update_target.clone(),
                );
                self.device_about_info = Some(r.about_info.clone());
                if staged_bluetooth_load {
                    self.schedule_initial_battery_refresh();
                } else {
                    self.schedule_battery_refresh_for_result(r.about_info.battery_halves);
                }
                self.current_encoder_visibility_id =
                    encoder_visibility_id(&r.device_name, r.keyboard_id);
                if let Some(dev) = self
                    .selected_device
                    .and_then(|idx| self.device_manager.devices().get(idx))
                {
                    self.device_display_names
                        .insert(dev.display_name_cache_key(), r.device_name.clone());
                }
                self.keycode_picker.tap_dance_entries = r.tap_dance_entries.clone();
                self.keycode_picker.tap_dance_synced_entries = r.tap_dance_entries.clone();
                self.combo_entries = r.combo_entries.clone();
                self.combo_synced_entries = r.combo_entries.clone();
                self.combo_dirty = false;
                self.combo_edit_revision = self.combo_edit_revision.wrapping_add(1);
                self.combo_attempted_revision = None;
                self.key_override_entries = r.key_override_entries.clone();
                let mut invalid_modifier_triggers = 0;
                for entry in &mut self.key_override_entries {
                    if matches!(entry.trigger, 0x00E0..=0x00E7) && entry.options.enabled {
                        Self::normalize_key_override_entry(entry);
                        invalid_modifier_triggers += 1;
                    }
                }
                if invalid_modifier_triggers > 0 {
                    log::warn!(
                        "Disabled {invalid_modifier_triggers} invalid Key Override modifier trigger(s)"
                    );
                    self.key_override_dirty = true;
                }
                self.alt_repeat_entries = r.alt_repeat_entries.clone();
                self.alt_repeat_names = load_alt_repeat_names(&self.current_device_name);
                self.alt_repeat_names
                    .resize(self.alt_repeat_entries.len(), String::new());
                self.alt_repeat_undo_stack.clear();
                self.selected_alt_repeat = 0;
                self.alt_repeat_visible_count = if self.alt_repeat_entries.is_empty() {
                    1
                } else {
                    1.min(self.alt_repeat_entries.len())
                };
                self.key_override_names = load_key_override_names(&self.current_device_name);
                self.key_override_names
                    .resize(self.key_override_entries.len(), String::new());
                self.key_override_visible_count = 1;
                self.key_override_undo_stack.clear();
                self.selected_key_override = 0;
                self.combo_names = load_combo_names(&self.current_device_name);
                self.combo_names
                    .resize(self.combo_entries.len(), String::new());
                self.combo_colors = load_combo_colors(&self.current_device_name);
                migrate_legacy_combo_default_colors(&mut self.combo_colors);
                normalize_combo_colors(&mut self.combo_colors, self.combo_entries.len());
                self.combo_term = r.combo_term.or(Some(50));
                self.auto_shift_options = r.auto_shift_options;
                self.auto_shift_timeout = r.auto_shift_timeout;
                self.auto_shift_timeout_text = r
                    .auto_shift_timeout
                    .map(|timeout| timeout.to_string())
                    .unwrap_or_default();
                self.mouse_keys_settings = r.mouse_keys_settings;
                self.touchpad_settings = r.touchpad_settings;
                self.bluetooth_settings = r.bluetooth_settings;
                self.module_settings = r.module_settings;
                self.tap_hold_settings = r.tap_hold_settings;
                self.magic_settings = r.magic_settings;
                self.one_shot_settings = r.one_shot_settings;
                self.grave_escape_settings = r.grave_escape_settings;
                self.layer_led_settings = r.layer_led_settings;
                self.rgb_settings = r.rgb_settings;
                self.display_settings = r.display_settings;
                self.layout_options_value = r.layout_options_value;
                let highest_used_combo = self
                    .combo_entries
                    .iter()
                    .enumerate()
                    .filter(|(i, combo)| {
                        !combo.output.is_no()
                            || combo.keys.iter().any(|&k| k != 0)
                            || combo.layer.is_some()
                            || self
                                .combo_names
                                .get(*i)
                                .map(|n| !n.trim().is_empty())
                                .unwrap_or(false)
                    })
                    .map(|(i, _)| i + 1)
                    .max()
                    .unwrap_or(1);
                self.combo_visible_count = highest_used_combo.min(self.combo_entries.len().max(1));
                self.selected_combo = self
                    .selected_combo
                    .min(self.combo_visible_count.saturating_sub(1));
                self.keycode_picker.macro_count = r.macro_texts.len();
                self.keycode_picker.macro_texts = r.macro_texts.clone();
                let mut macro_metadata = load_macro_metadata(&self.current_device_name);
                macro_metadata.resize(r.macro_texts.len());
                self.keycode_picker.macro_names = macro_metadata.names;
                self.keycode_picker.macro_descriptions = macro_metadata.descriptions;
                self.keycode_picker.macro_metadata_dirty = false;
                self.keycode_picker.supports_macro_ext_keycodes = r.supports_macro_ext_keycodes;
                self.keycode_picker.supports_rmk_native_key_actions =
                    r.supports_rmk_native_key_actions;
                self.keycode_picker.supports_universal_symbols = r.supports_universal_symbols;
                self.keycode_picker.supports_universal_russian_letters =
                    r.supports_universal_russian_letters;
                self.keycode_picker.supports_rmk_native_combo_output =
                    r.supports_rmk_native_combo_output;
                self.keycode_picker.supports_rmk_native_tap_dance_actions =
                    r.supports_rmk_native_tap_dance_actions;
                self.supports_rmk_combo_layers = r.supports_rmk_combo_layers;
                self.keycode_picker.macro_ext_keycodes_disabled_reason =
                    r.macro_ext_keycodes_disabled_reason;
                // Parse macro texts into actions (Vial protocol v2+ bytecode).
                self.keycode_picker.macro_actions = r
                    .macro_texts
                    .iter()
                    .map(|bytes| crate::keycode_picker::decode_macro_actions(bytes))
                    .collect();

                let connected_display_name = self
                    .selected_device
                    .and_then(|idx| self.device_manager.devices().get(idx))
                    .map(|device| device.display_name_with_transport(&r.device_name))
                    .unwrap_or_else(|| r.device_name.clone());
                self.status_msg = format!("Connected: {connected_display_name}");

                // Load per-device layer names.
                let device_name = r.device_name.clone();
                let local_layer_names = load_saved_layer_names(&device_name);
                self.layer_names = resolve_layer_names(
                    &r.layout.layer_names,
                    &r.layer_names_from_firmware,
                    local_layer_names.as_deref(),
                    r.layer_count,
                );

                let encoder_count = r.layout.encoder_count();
                let hide_modular_encoders_by_default =
                    self.hide_modular_encoders_by_default(&r.layout);
                self.encoder_visibility = Self::resolve_initial_encoder_visibility(
                    &r.layout,
                    self.layout_options_value,
                    load_saved_encoder_visibility(
                        &self.current_encoder_visibility_id,
                        encoder_count,
                    ),
                    hide_modular_encoders_by_default,
                );

                // Populate picker
                self.keycode_picker.supports_rgb =
                    r.layout.supports_rgb || self.rgb_settings.supported;
                self.keycode_picker.supports_macro = self.keycode_picker.macro_count > 0;
                self.keycode_picker.supports_tap_dance = !r.tap_dance_entries.is_empty();
                // Mouse keycodes are assignable through the keymap even when a
                // firmware does not expose Vial/QMK mouse-key settings.
                self.keycode_picker.supports_mouse_keys = true;
                self.keycode_picker.supports_combo = !self.combo_entries.is_empty();
                self.keycode_picker.supports_auto_shift = r.supported_qmk_settings.contains(&4);
                self.keycode_picker.supports_caps_word = r.vial_features.caps_word;
                self.keycode_picker.supports_repeat_key = r.vial_features.repeat_key;
                self.keycode_picker.supports_alt_repeat_key = r.vial_features.alt_repeat_key;
                self.keycode_picker.supports_layer_lock = r.vial_features.layer_lock;
                self.keycode_picker.supports_persistent_default_layer =
                    r.vial_features.persistent_default_layer;
                self.keycode_picker.layer_count = r.layout.layers.len().max(1);
                self.keycode_picker.tap_dance_names = load_tap_dance_names(&device_name);
                // Vial GUI maps customKeycodes to USER00.. at QK_KB + index.
                // Protocol v6: QK_KB = 0x7E00. Do not use QK_USER (0x7E40):
                // assigning those values writes the wrong keycodes to firmware.
                const QK_KB: u16 = 0x7E00;
                self.keycode_picker.custom_keycodes = r
                    .layout
                    .custom_keycodes
                    .iter()
                    .enumerate()
                    .map(|(i, custom)| {
                        (
                            custom.name.clone(),
                            custom.label.clone(),
                            custom.title.clone(),
                            QK_KB + i as u16,
                        )
                    })
                    .collect();
                self.keycode_picker.layer_names = self.layer_names.clone();
                self.sticky_layout_prev_pressed.clear();
                self.sticky_layout_pressed_key_layers.clear();
                self.sticky_layout_toggled_layers = vec![false; r.layout.layers.len().max(1)];
                self.sticky_layout_active_combos = vec![false; r.combo_entries.len()];
                self.sticky_layout_tap_dance_states.clear();
                self.sticky_layout_base_layer = 0;
                self.sticky_layout_active_layer = 0;

                self.layout = Some(r.layout);
                self.sync_firmware_managed_layout_options();
                self.refresh_layer_picker_content_flags();

                // Keep the same HID owner that loaded the keyboard, matching vial-gui's
                // open-once/reload/use model. Avoid Entropy-only reopen churn when switching
                // between qmk-vial and RMK devices.
                self.shared_hid_output = r
                    .hid_device
                    .as_ref()
                    .and_then(crate::hid::HidDevice::shared_output);
                self.hid_device = r.hid_device;
                self.supported_qmk_settings = r.supported_qmk_settings;
                self.deferred_device_load = r.deferred_load;

                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.restore_entropy_display_preset_after_connect();
                    self.sync_qmk_hid_host_bridges();
                }

                log::info!(
                    "Connected: {} ({} layers, {:?})",
                    connected_display_name,
                    r.layer_count,
                    self.firmware
                );
            }
            Err(e) => {
                if let Some(reconnect) = reconnect {
                    self.schedule_bluetooth_reconnect_retry(reconnect, &e);
                    return;
                }

                if crate::hid::is_disconnect_error_message(&e)
                    && self.begin_bluetooth_reconnect(e.clone())
                {
                    return;
                }

                if crate::hid::is_disconnect_error_message(&e) {
                    self.selected_device = None;
                    self.clear_connected_keyboard_state(e);
                    self.start_device_scan();
                    return;
                }

                if is_hid_open_failure(&e) {
                    self.selected_device = None;
                    self.clear_connected_keyboard_state(e);
                    return;
                }

                self.status_msg = e;
                log::error!("{}", connect_apply_error_log(&self.status_msg));
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn sync_layer_names_to_firmware(&self) {
        let _ = self.write_layer_names_to_firmware(&self.layer_names);
    }

    /// Write `names` to firmware (qsid 200 + layer), one layer at a time so a
    /// single failing SET does not abort the rest. Skips names that already
    /// match. Returns the indices of layers whose write did not land, so callers
    /// like the .entlayout import can aggregate and report them. An empty result
    /// means every non-empty name was already correct or was written back —
    /// never a masked transport error.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn write_layer_names_to_firmware(&self, names: &[String]) -> Vec<usize> {
        if self.firmware != FirmwareProtocol::Vial {
            return Vec::new();
        }
        let Some(dev) = &self.hid_device else {
            return Vec::new();
        };
        let store = HidLayerNameStore {
            dev,
            supported: &self.supported_qmk_settings,
        };
        sync_layer_names_to_store(&store, names, self.layer_count)
    }
}

/// Storage seam for layer-name persistence, so the sync loop can be exercised
/// without a real HID device. `is_supported` reports whether the firmware
/// exposes storage for the setting id at all — used to keep genuine
/// "unsupported storage" apart from a transient read/write failure.
#[cfg(not(target_arch = "wasm32"))]
pub(super) trait LayerNameStore {
    fn is_supported(&self, qsid: u16) -> bool;
    fn get_string(&self, qsid: u16) -> anyhow::Result<String>;
    fn set_string(&self, qsid: u16, value: &str) -> anyhow::Result<()>;
}

/// Production [`LayerNameStore`] backed by the open HID connection and the
/// setting ids the firmware advertised during connect.
#[cfg(not(target_arch = "wasm32"))]
struct HidLayerNameStore<'a> {
    dev: &'a crate::hid::HidDevice,
    supported: &'a [u16],
}

#[cfg(not(target_arch = "wasm32"))]
impl LayerNameStore for HidLayerNameStore<'_> {
    fn is_supported(&self, qsid: u16) -> bool {
        self.supported.contains(&qsid)
    }
    fn get_string(&self, qsid: u16) -> anyhow::Result<String> {
        self.dev.get_qmk_setting_string(qsid)
    }
    fn set_string(&self, qsid: u16, value: &str) -> anyhow::Result<()> {
        self.dev.set_qmk_setting_string(qsid, value)
    }
}

/// Persist `names` to `store`, one layer at a time. A layer is reported failed
/// only when its storage is supported yet a read or write actually errors — an
/// unsupported id is skipped silently (the name lives on in the local per-device
/// store), and a transport error never masquerades as "unsupported".
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn sync_layer_names_to_store<S: LayerNameStore>(
    store: &S,
    names: &[String],
    layer_count: usize,
) -> Vec<usize> {
    let mut failed = Vec::new();
    for (layer, name) in names.iter().enumerate().take(layer_count) {
        let qsid = 200 + layer as u16;
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        if !store.is_supported(qsid) {
            // Firmware exposes no storage for this layer name; nothing to persist.
            continue;
        }
        match store.get_string(qsid) {
            Ok(current) if current == name => {}
            Ok(_) => {
                if let Err(e) = store.set_string(qsid, name) {
                    log::warn!(
                        "Vial set_qmk_setting_string failed while syncing layer {layer}: {e}"
                    );
                    failed.push(layer);
                }
            }
            Err(e) => {
                // The id is advertised as supported, so a read error is a real
                // transport failure, not missing storage — surface it.
                log::warn!("Vial get_qmk_setting_string failed while syncing layer {layer}: {e}");
                failed.push(layer);
            }
        }
    }
    failed
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod qa_followup_reconnect {
    use super::super::vial_hid_task::{VialHidOperation, VialHidTaskStart};
    use super::*;

    fn device(serial: &str) -> crate::device::Device {
        crate::device::Device {
            name: "Draft keyboard".into(),
            vendor_id: 0x1111,
            product_id: 0x2222,
            manufacturer: "fixture".into(),
            serial_number: serial.into(),
            bus_type: "Bluetooth".into(),
            path: format!("fixture-{serial}"),
            instance_token: format!("fixture-instance-{serial}"),
            firmware: FirmwareProtocol::Vial,
        }
    }
    fn layout() -> KeyboardLayout {
        KeyboardLayout::from_vial_json(&serde_json::json!({
            "name":"Draft keyboard", "matrix":{"rows":1,"cols":1},
            "layouts":{"keymap":[["0,0"]]}
        }))
        .unwrap()
    }
    fn result(device: &crate::device::Device, keyboard_id: u64) -> ConnectResult {
        let (hid, _) = crate::hid::HidDevice::test_device();
        ConnectResult {
            device_name: device.name.clone(),
            keyboard_id: keyboard_id,
            vial_unlock_status: Default::default(),
            hid_device: Some(hid),
            layout: layout(),
            layer_count: 1,
            about_info: DeviceAboutInfo {
                vendor_id: device.vendor_id,
                product_id: device.product_id,
                ..Default::default()
            },
            macro_texts: Default::default(),
            supports_macro_ext_keycodes: Default::default(),
            supports_rmk_native_key_actions: Default::default(),
            supports_universal_symbols: Default::default(),
            supports_universal_russian_letters: Default::default(),
            supports_rmk_native_combo_output: Default::default(),
            supports_rmk_native_tap_dance_actions: Default::default(),
            supports_rmk_combo_layers: Default::default(),
            macro_ext_keycodes_disabled_reason: Default::default(),
            tap_dance_entries: Default::default(),
            combo_entries: Default::default(),
            combo_term: Default::default(),
            auto_shift_options: Default::default(),
            auto_shift_timeout: Default::default(),
            mouse_keys_settings: Default::default(),
            touchpad_settings: Default::default(),
            bluetooth_settings: Default::default(),
            module_settings: Default::default(),
            tap_hold_settings: Default::default(),
            magic_settings: Default::default(),
            one_shot_settings: Default::default(),
            grave_escape_settings: Default::default(),
            layer_led_settings: Default::default(),
            rgb_settings: Default::default(),
            display_settings: Default::default(),
            layout_options_value: Default::default(),
            key_override_entries: Default::default(),
            alt_repeat_entries: Default::default(),
            vial_features: Default::default(),
            layer_names_from_firmware: Default::default(),
            supported_qmk_settings: Default::default(),
            deferred_load: Default::default(),
        }
    }
    fn poll_upload(app: &mut EntropyApp, ctx: &egui::Context) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.vial_hid_task.is_some() {
            assert!(std::time::Instant::now() < deadline);
            app.poll_vial_hid_task(ctx);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    fn failed_upload(ctx: &egui::Context) -> (EntropyApp, BluetoothReconnectState) {
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&cc);
        app.device_manager.replace_devices(vec![device("a")]);
        app.selected_device = Some(0);
        app.layout = Some(layout());
        app.current_keyboard_id = Some(7);
        app.current_device_name = "Draft keyboard".into();
        let (hid, _) = crate::hid::HidDevice::test_device_with_fault_after_requests(Some((
            0,
            crate::hid::TestHidFault::Disconnect,
        )));
        app.hid_device = Some(hid);
        let p = &mut app.display_settings.pictograms;
        p.loaded = true;
        p.supported = Some(true);
        p.editor_name = "rejected upload draft".into();
        p.source_levels = vec![42; PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT];
        p.undo = vec![vec![17; PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT]];
        p.library
            .set(PictogramKind::Macro, 0, &vec![0xAA; PICTOGRAM_BYTES]);
        assert!(app.apply_current_pictogram(ctx));
        poll_upload(&mut app, ctx);
        let ConnectState::Reconnecting(reconnect) = &app.connect_state else {
            panic!("did not enter actual automatic reconnect");
        };
        let reconnect = reconnect.clone();
        assert_eq!(
            app.display_settings.pictograms.editor_name,
            "rejected upload draft"
        );
        assert!(!app.display_settings.pictograms.loaded);
        (app, reconnect)
    }
    fn complete(
        app: &mut EntropyApp,
        ctx: &egui::Context,
        result: ConnectResult,
        reconnect: Option<BluetoothReconnectState>,
    ) {
        let (tx, rx) = mpsc::channel();
        let now = std::time::Instant::now();
        app.connect_state = ConnectState::Loading {
            device: app.device_manager.devices()[app.selected_device.unwrap()].clone(),
            cancel: Default::default(),
            rx,
            started_at: now,
            last_progress_at: now,
            reconnect,
        };
        tx.send(ConnectTaskMessage::Done(Box::new(Ok(result))))
            .unwrap();
        app.poll_connect(ctx);
        assert!(matches!(app.connect_state, ConnectState::Idle));
    }

    #[test]
    fn failed_upload_draft_survives_successful_same_device_automatic_reconnect_and_reread() {
        let ctx = egui::Context::default();
        let (mut app, reconnect) = failed_upload(&ctx);
        let draft = app.display_settings.pictograms.clone();
        complete(&mut app, &ctx, result(&device("a"), 7), Some(reconnect));
        let p = &app.display_settings.pictograms;
        assert_eq!(p.editor_name, draft.editor_name);
        assert_eq!(p.source_levels, draft.source_levels);
        assert_eq!(p.undo, draft.undo);
        assert!(!p.loaded);
        assert!(!p.library.has(PictogramKind::Macro, 0));
        assert!(p.preserve_editor_on_load);
        let confirmed = PictogramLibrary::default();
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        recorder.respond_with(test_pictogram_read_responses(&confirmed));
        app.hid_device = Some(hid);
        assert!(matches!(
            app.start_vial_hid_operation(
                &ctx,
                VialHidOperation::PictogramLoad {
                    preserve_editor: app.display_settings.pictograms.preserve_editor_on_load,
                }
            ),
            VialHidTaskStart::Started
        ));
        poll_upload(&mut app, &ctx);
        let p = &app.display_settings.pictograms;
        assert!(p.loaded);
        assert_eq!(p.library, confirmed);
        assert_eq!(p.editor_name, draft.editor_name);
        assert_eq!(p.source_levels, draft.source_levels);
        assert_eq!(p.undo, draft.undo);
        assert!(!p.preserve_editor_on_load);
    }

    #[test]
    fn reconnect_does_not_transfer_draft_to_another_physical_device_or_definition() {
        for (serial, keyboard_id) in [("b", 7), ("a", 8)] {
            let ctx = egui::Context::default();
            let (mut app, reconnect) = failed_upload(&ctx);
            app.device_manager.replace_devices(vec![device(serial)]);
            complete(
                &mut app,
                &ctx,
                result(&device(serial), keyboard_id),
                Some(reconnect),
            );
            let p = &app.display_settings.pictograms;
            assert!(p.editor_name.is_empty());
            assert!(p.undo.is_empty());
            assert!(!p.preserve_editor_on_load);
            assert!(!p.loaded);
            assert!(!p.library.has(PictogramKind::Macro, 0));
        }
    }

    #[test]
    fn explicit_new_connection_does_not_inherit_failed_upload_draft() {
        let ctx = egui::Context::default();
        let (mut app, _) = failed_upload(&ctx);
        complete(&mut app, &ctx, result(&device("a"), 7), None);
        assert!(app.display_settings.pictograms.editor_name.is_empty());
        assert!(app.display_settings.pictograms.undo.is_empty());
        assert!(!app.display_settings.pictograms.preserve_editor_on_load);
    }
}
