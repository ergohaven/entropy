use super::*;

const DEVICE_SCAN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

fn should_wait_for_manual_device_selection(status_msg: &str) -> bool {
    status_msg.starts_with("Open failed:") || status_msg.starts_with("Connect timeout")
}

fn should_auto_connect_only_device(device_count: usize) -> bool {
    device_count == 1
}

fn usb_endpoint_was_reenumerated(previous: &Device, current: &Device) -> bool {
    !previous.is_bluetooth_transport()
        && !previous.instance_token.is_empty()
        && !current.instance_token.is_empty()
        && previous.instance_token != current.instance_token
}

#[cfg(not(target_arch = "wasm32"))]
fn unique_reconnect_device_index(devices: &[Device], identity: &DeviceIdentity) -> Option<usize> {
    let mut matches = devices
        .iter()
        .enumerate()
        .filter_map(|(index, device)| identity.matches(device).then_some(index));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn maybe_start_bluetooth_reconnect_scan(&mut self, ctx: &egui::Context) {
        let Some(next_attempt_at) = (match &self.connect_state {
            ConnectState::Reconnecting(state) => Some(state.next_attempt_at),
            ConnectState::Idle | ConnectState::SelectingDevice | ConnectState::Loading { .. } => {
                None
            }
        }) else {
            return;
        };

        let now = std::time::Instant::now();
        if now >= next_attempt_at {
            self.start_device_scan();
        } else {
            ctx.request_repaint_after(next_attempt_at.saturating_duration_since(now));
        }
    }

    pub(super) fn start_device_scan(&mut self) {
        if !matches!(self.device_scan_state, DeviceScanState::Idle) {
            return;
        }

        #[cfg(target_os = "macos")]
        if crate::hid::macos_hid_scan_disabled_for_rosetta() {
            if self.status_msg.is_empty() {
                self.status_msg = crate::hid::macos_rosetta_hid_status_message().into();
            }
            return;
        }

        let (tx, rx) = mpsc::channel();
        self.device_scan_state = DeviceScanState::Scanning {
            rx,
            started_at: std::time::Instant::now(),
            generation: self.connection_generation,
            timeout_logged: false,
        };
        std::thread::spawn(move || {
            let result = DeviceManager::scan_devices();
            if let Ok(devices) = &result {
                log::debug!(
                    "HID scan completed with {} Vial endpoint(s): {}",
                    devices.len(),
                    devices
                        .iter()
                        .map(|device| format!(
                            "{:04X}:{:04X}@{} [{}]",
                            device.vendor_id, device.product_id, device.path, device.instance_token
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            let _ = tx.send(result);
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_device_scan(&mut self, ctx: &egui::Context) {
        let completed = match &mut self.device_scan_state {
            DeviceScanState::Idle => return,
            DeviceScanState::Scanning {
                rx,
                started_at,
                generation,
                timeout_logged,
            } => match rx.try_recv() {
                Ok(result) => Some((*generation, result)),
                Err(mpsc::TryRecvError::Empty) => {
                    if started_at.elapsed() >= DEVICE_SCAN_TIMEOUT && !*timeout_logged {
                        log::warn!(
                            "HID device scan exceeded {:?}; keeping the single worker alive",
                            DEVICE_SCAN_TIMEOUT
                        );
                        *timeout_logged = true;
                    }
                    ctx.request_repaint_after(std::time::Duration::from_millis(25));
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => Some((
                    *generation,
                    Err("HID device scan worker stopped".to_owned()),
                )),
            },
        };

        self.device_scan_state = DeviceScanState::Idle;
        if let Some((generation, result)) = completed {
            if generation != self.connection_generation {
                log::debug!(
                    "Ignoring HID scan from connection generation {generation}; current generation is {}",
                    self.connection_generation
                );
                return;
            }
            match result {
                Ok(devices) => self.apply_device_scan_result(devices),
                Err(error) => log::warn!("{error}; preserving the previous device list"),
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn apply_device_scan_result(&mut self, devices: Vec<Device>) {
        if let ConnectState::Reconnecting(reconnect) = &self.connect_state {
            let reconnect = reconnect.clone();
            let reconnect_device_index =
                unique_reconnect_device_index(&devices, &reconnect.identity);
            self.device_manager.replace_devices(devices);
            self.retain_connected_qmk_hid_host_bridges();
            let connected_display_name_keys: std::collections::HashSet<String> = self
                .device_manager
                .devices()
                .iter()
                .map(Device::display_name_cache_key)
                .collect();
            self.device_display_names
                .retain(|key, _| connected_display_name_keys.contains(key));

            if let Some(device_index) = reconnect_device_index {
                self.selected_device = Some(device_index);
                self.start_reconnect_connect(device_index, reconnect);
            } else {
                self.selected_device = None;
                self.schedule_bluetooth_reconnect_retry(reconnect, "device not found");
            }
            return;
        }

        let previous_device = self
            .selected_device
            .and_then(|idx| self.device_manager.devices().get(idx))
            .cloned();
        let previous_device_key = previous_device.as_ref().map(Device::display_name_cache_key);
        let was_loading = matches!(self.connect_state, ConnectState::Loading { .. });
        let selecting_device = matches!(self.connect_state, ConnectState::SelectingDevice);

        self.device_manager.replace_devices(devices);
        self.retain_connected_qmk_hid_host_bridges();
        let connected_display_name_keys: std::collections::HashSet<String> = self
            .device_manager
            .devices()
            .iter()
            .map(Device::display_name_cache_key)
            .collect();
        self.device_display_names
            .retain(|key, _| connected_display_name_keys.contains(key));

        // A user-requested identity takes precedence over discovery's automatic
        // choice, including an empty scan while its cancelled predecessor retires.
        if let Some(identity) = &self.pending_device_connect {
            let pending_index =
                unique_reconnect_device_index(self.device_manager.devices(), identity);
            if self.layout.is_some() && !was_loading {
                // A blocked replacement must not relabel the currently live HID.
                self.selected_device = previous_device.as_ref().and_then(|device| {
                    unique_reconnect_device_index(
                        self.device_manager.devices(),
                        &device.stable_identity(),
                    )
                });
                if self.selected_device.is_none() {
                    let pending = self.pending_device_connect.take();
                    self.clear_connected_keyboard_state("No device detected");
                    self.pending_device_connect = pending;
                }
            } else {
                self.selected_device = pending_index;
            }
            if pending_index.is_some() {
                self.resume_pending_device_connect();
            }
            return;
        }

        if self.device_manager.devices().is_empty() {
            if selecting_device {
                self.selected_device = None;
                self.retain_connected_qmk_hid_host_bridges();
                return;
            }
            if was_loading {
                // Release the UI, but keep the cancelled transport serialized
                // until it finishes. clear_connected_keyboard_state retires it
                // and makes every late result inert.
                self.selected_device = None;
                self.clear_connected_keyboard_state("No device detected");
                return;
            }
            if self.selected_device.is_some() || self.layout.is_some() || was_loading {
                self.selected_device = None;
                self.clear_connected_keyboard_state("No device detected");
            } else {
                self.retain_connected_qmk_hid_host_bridges();
            }
            return;
        }

        if selecting_device {
            self.selected_device = None;
            self.status_msg.clear();
            self.retain_connected_qmk_hid_host_bridges();
            return;
        }

        if self.selected_device.is_none()
            && self.layout.is_none()
            && !was_loading
            && should_wait_for_manual_device_selection(&self.status_msg)
        {
            self.retain_connected_qmk_hid_host_bridges();
            return;
        }

        #[cfg(target_os = "linux")]
        if self.selected_device.is_none()
            && self.layout.is_none()
            && !was_loading
            && !super::app_settings_ui::linux_vial_udev_rules_installed()
            && !self
                .device_manager
                .devices()
                .iter()
                .any(Device::uses_bluez_gatt_transport)
        {
            self.retain_connected_qmk_hid_host_bridges();
            return;
        }

        if let Some(device_key) = previous_device_key {
            if let Some(idx) = self
                .device_manager
                .devices()
                .iter()
                .position(|dev| dev.display_name_cache_key() == device_key)
            {
                self.selected_device = Some(idx);
                let endpoint_was_reenumerated = previous_device.as_ref().is_some_and(|previous| {
                    usb_endpoint_was_reenumerated(previous, &self.device_manager.devices()[idx])
                });
                if endpoint_was_reenumerated {
                    log::info!("USB HID endpoint was re-enumerated; reopening the device");
                    self.start_connect(idx);
                } else if self.layout.is_none() && !was_loading {
                    self.start_connect(idx);
                } else {
                    self.sync_qmk_hid_host_bridges();
                }
                return;
            }
        }

        if !should_auto_connect_only_device(self.device_manager.devices().len()) {
            if self.selected_device.is_some() || self.layout.is_some() || was_loading {
                self.selected_device = None;
                self.clear_connected_keyboard_state("");
            } else {
                self.status_msg.clear();
                self.retain_connected_qmk_hid_host_bridges();
            }
            return;
        }

        self.start_connect(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_test_device(name: &str) -> Device {
        Device {
            name: name.to_owned(),
            vendor_id: 0xFFFF,
            product_id: 0xFFFF,
            manufacturer: "Test".to_owned(),
            serial_number: name.to_owned(),
            bus_type: "Usb".to_owned(),
            path: format!("/nonexistent/entropy-test-{name}"),
            instance_token: "original-instance".to_owned(),
            firmware: FirmwareProtocol::Vial,
        }
    }

    #[test]
    fn scans_preserve_queued_identity_through_absence_and_reordering() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&cc);
        let (launch_tx, requests) = mpsc::channel();
        app.test_connect_requests = Some(launch_tx);
        let a = scan_test_device("A");
        let b = scan_test_device("B");
        app.device_manager
            .replace_devices(vec![a.clone(), b.clone()]);
        app.start_connect(0);
        let (_, old_tx) = requests.try_recv().unwrap();
        app.start_connect(1);
        app.apply_device_scan_result(vec![a.clone()]);
        assert_eq!(app.pending_device_connect, Some(b.stable_identity()));
        assert_eq!(app.selected_device, None);
        assert!(requests.try_recv().is_err());
        assert!(matches!(app.device_scan_state, DeviceScanState::Idle));
        app.apply_device_scan_result(Vec::new());
        assert_eq!(app.pending_device_connect, Some(b.stable_identity()));
        app.apply_device_scan_result(vec![b.clone(), a]);
        assert_eq!(app.selected_device, Some(0));
        assert!(requests.try_recv().is_err());
        old_tx
            .send(ConnectTaskMessage::Done(Box::new(Err(
                "Layout read failed: Connect cancelled".to_owned(),
            ))))
            .unwrap();
        app.poll_connect(&ctx);
        assert_eq!(
            requests.try_recv().unwrap().0.stable_identity(),
            b.stable_identity()
        );
        assert_eq!(app.selected_device, Some(0));
    }

    #[test]
    fn reused_hidraw_instance_cancels_loading_owner_before_reopening() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&cc);
        let (launch_tx, requests) = mpsc::channel();
        app.test_connect_requests = Some(launch_tx);
        let mut device = scan_test_device("A");
        app.device_manager.replace_devices(vec![device.clone()]);
        app.start_connect(0);
        let (_, old_tx) = requests.try_recv().unwrap();
        device.instance_token = "replacement-instance".to_owned();
        app.apply_device_scan_result(vec![device]);
        assert!(
            matches!(&app.connect_state, ConnectState::Loading { cancel, .. } if cancel.load(std::sync::atomic::Ordering::Relaxed))
        );
        assert!(requests.try_recv().is_err());
        old_tx
            .send(ConnectTaskMessage::Done(Box::new(Err(
                "old device disconnected".to_owned(),
            ))))
            .unwrap();
        app.poll_connect(&ctx);
        assert_eq!(
            requests.try_recv().unwrap().0.instance_token,
            "replacement-instance"
        );
        assert_eq!(app.selected_device, Some(0));
    }

    #[test]
    fn unplug_and_reselection_do_not_overlap_a_blocked_transport_worker() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&cc);
        let (launch_tx, requests) = mpsc::channel();
        app.test_connect_requests = Some(launch_tx);
        let device = scan_test_device("A");
        app.device_manager.replace_devices(vec![device.clone()]);
        app.start_connect(0);
        let (_, old_tx) = requests.try_recv().unwrap();
        app.apply_device_scan_result(Vec::new());
        assert!(matches!(app.connect_state, ConnectState::Idle));
        assert!(!app.retiring_connects.is_empty());
        app.device_manager.replace_devices(vec![device]);
        app.start_connect(0);
        app.poll_connect(&ctx);
        assert!(requests.try_recv().is_err());
        drop(old_tx);
        app.poll_connect(&ctx);
        assert_eq!(requests.try_recv().unwrap().0.serial_number, "A");
        assert!(app.retiring_connects.is_empty());
    }

    #[test]
    fn manual_device_selection_waits_after_open_failure() {
        assert!(should_wait_for_manual_device_selection(
            "Open failed: Failed to open HID device"
        ));
        assert!(should_wait_for_manual_device_selection(
            "Connect timeout — RMK/Vial device did not finish loading"
        ));
        assert!(!should_wait_for_manual_device_selection(""));
    }

    #[test]
    fn startup_auto_connects_only_when_exactly_one_device_exists() {
        assert!(!should_auto_connect_only_device(0));
        assert!(should_auto_connect_only_device(1));
        assert!(!should_auto_connect_only_device(2));
        assert!(!should_auto_connect_only_device(8));
    }

    #[test]
    fn reused_usb_path_with_a_new_instance_requires_reopen() {
        let mut previous = Device {
            name: "M4CR0Pad v3".to_owned(),
            vendor_id: 0xE126,
            product_id: 0x0042,
            manufacturer: "Ergohaven".to_owned(),
            serial_number: String::new(),
            bus_type: "Usb".to_owned(),
            path: "/dev/hidraw4".to_owned(),
            instance_token: "1:100:60932".to_owned(),
            firmware: FirmwareProtocol::Vial,
        };
        let mut current = previous.clone();
        current.instance_token = "1:104:60932".to_owned();

        assert!(usb_endpoint_was_reenumerated(&previous, &current));
        previous.bus_type = "Bluetooth".to_owned();
        current.bus_type = "Bluetooth".to_owned();
        assert!(!usb_endpoint_was_reenumerated(&previous, &current));
    }

    #[test]
    fn unplug_while_loading_releases_the_connect_owner_for_the_next_scan() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let now = std::time::Instant::now();
        app.connect_state = ConnectState::Loading {
            device: scan_test_device("Loading"),
            rx: std::sync::mpsc::channel().1,
            started_at: now,
            last_progress_at: now,
            cancel: cancel.clone(),
            reconnect: None,
        };

        app.apply_device_scan_result(Vec::new());

        assert!(cancel.load(std::sync::atomic::Ordering::Relaxed));
        assert!(matches!(app.connect_state, ConnectState::Idle));
        assert_eq!(app.status_msg, "No device detected");
    }

    #[test]
    fn timed_out_scan_keeps_its_single_worker_and_receiver() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (_tx, rx) = std::sync::mpsc::channel();
        app.device_scan_state = DeviceScanState::Scanning {
            rx,
            started_at: std::time::Instant::now() - DEVICE_SCAN_TIMEOUT,
            generation: app.connection_generation,
            timeout_logged: false,
        };

        app.poll_device_scan(&ctx);
        app.start_device_scan();

        assert!(matches!(
            app.device_scan_state,
            DeviceScanState::Scanning {
                timeout_logged: true,
                ..
            }
        ));
    }

    #[test]
    fn stale_scan_generation_cannot_restore_an_old_usb_endpoint() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        app.device_manager.replace_devices(Vec::new());
        let old_generation = app.connection_generation;
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Ok(vec![Device {
            name: "M4CR0Pad v3".to_owned(),
            vendor_id: 0xE126,
            product_id: 0x0042,
            manufacturer: "Ergohaven".to_owned(),
            serial_number: String::new(),
            bus_type: "Usb".to_owned(),
            path: "/dev/hidraw4".to_owned(),
            instance_token: "sysfs:.0007".to_owned(),
            firmware: FirmwareProtocol::Vial,
        }]))
        .unwrap();
        app.device_scan_state = DeviceScanState::Scanning {
            rx,
            started_at: std::time::Instant::now(),
            generation: old_generation,
            timeout_logged: false,
        };
        app.connection_generation = app.connection_generation.wrapping_add(1);

        app.poll_device_scan(&ctx);

        assert!(app.device_manager.devices().is_empty());
        assert!(matches!(app.device_scan_state, DeviceScanState::Idle));
    }

    #[test]
    fn explicit_device_selection_does_not_auto_connect_only_scan_result() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        let device = Device {
            name: "K:04".to_owned(),
            vendor_id: 0xE126,
            product_id: 0x0074,
            manufacturer: "Ergohaven".to_owned(),
            serial_number: "AA:BB:CC:DD:EE:FF".to_owned(),
            bus_type: "Bluetooth".to_owned(),
            path: "/dev/hidraw4".to_owned(),
            instance_token: "/dev/hidraw4".to_owned(),
            firmware: FirmwareProtocol::Vial,
        };
        app.connect_state = ConnectState::SelectingDevice;

        app.apply_device_scan_result(vec![device]);

        assert!(app.selected_device.is_none());
        assert!(matches!(app.connect_state, ConnectState::SelectingDevice));
        assert_eq!(app.device_manager.devices().len(), 1);
    }

    #[test]
    fn unplugged_wired_keyboard_returns_to_the_device_menu_after_replug() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        // The device menu must update without opening a real HID handle.
        app.connect_state = ConnectState::SelectingDevice;
        let mut keyboard = Device {
            name: "Wired Keyboard".to_owned(),
            vendor_id: 0xE126,
            product_id: 0x0042,
            manufacturer: "Ergohaven".to_owned(),
            serial_number: "replug-test".to_owned(),
            bus_type: "Usb".to_owned(),
            path: "/dev/hidraw4".to_owned(),
            instance_token: "sysfs:.0007".to_owned(),
            firmware: FirmwareProtocol::Vial,
        };
        app.apply_device_scan_result(vec![keyboard.clone()]);
        app.apply_device_scan_result(Vec::new());
        assert!(app.device_manager.devices().is_empty());
        keyboard.instance_token = "sysfs:.0008".to_owned();
        app.apply_device_scan_result(vec![keyboard.clone()]);
        assert_eq!(app.device_manager.devices().len(), 1);
        assert_eq!(
            app.device_manager.devices()[0].instance_token,
            keyboard.instance_token
        );
        assert!(app.selected_device.is_none());

        // Enumeration errors are not successful empty scans (unplugs).
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Err("temporary enumeration failure".to_owned()))
            .unwrap();
        app.device_scan_state = DeviceScanState::Scanning {
            rx,
            started_at: std::time::Instant::now(),
            generation: app.connection_generation,
            timeout_logged: false,
        };
        app.poll_device_scan(&ctx);
        assert_eq!(app.device_manager.devices().len(), 1);
        assert!(matches!(app.device_scan_state, DeviceScanState::Idle));
    }

    #[test]
    fn reconnect_selects_only_the_same_bluetooth_identity() {
        let mut expected = Device {
            name: "K:04".to_owned(),
            vendor_id: 0xE126,
            product_id: 0x0074,
            manufacturer: "Ergohaven".to_owned(),
            serial_number: "AA:BB:CC:DD:EE:FF".to_owned(),
            bus_type: "Bluetooth".to_owned(),
            path: "/dev/hidraw4".to_owned(),
            instance_token: "/dev/hidraw4".to_owned(),
            firmware: FirmwareProtocol::Vial,
        };
        let identity = expected.stable_identity();
        expected.path = "/dev/hidraw9".to_owned();
        let mut other = expected.clone();
        other.serial_number = "11:22:33:44:55:66".to_owned();

        assert_eq!(
            unique_reconnect_device_index(&[other, expected], &identity),
            Some(1)
        );
    }

    #[test]
    fn reconnect_refuses_an_ambiguous_serial_less_match() {
        let expected = Device {
            name: "K:04".to_owned(),
            vendor_id: 0xE126,
            product_id: 0x0074,
            manufacturer: "Ergohaven".to_owned(),
            serial_number: String::new(),
            bus_type: "Bluetooth".to_owned(),
            path: "/dev/hidraw4".to_owned(),
            instance_token: "/dev/hidraw4".to_owned(),
            firmware: FirmwareProtocol::Vial,
        };
        let identity = expected.stable_identity();
        let mut duplicate = expected.clone();
        duplicate.path = "/dev/hidraw9".to_owned();

        assert_eq!(
            unique_reconnect_device_index(&[expected, duplicate], &identity),
            None
        );
    }
}
