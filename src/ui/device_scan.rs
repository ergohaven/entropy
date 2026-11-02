use super::*;

const DEVICE_SCAN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

fn should_wait_for_manual_device_selection(status_msg: &str) -> bool {
    status_msg.starts_with("Open failed:")
        || status_msg.starts_with("Connect timeout")
        || status_msg.starts_with("Connect failed:")
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

/// `scanned` in the order the list already shows: listed devices keep their
/// rows and new ones follow in scan order. Scans order the transports
/// differently (a USB-only one keeps the other transports first, a full one
/// lists them last), and a list that moves under the pointer gets the wrong
/// row picked.
#[cfg(not(target_arch = "wasm32"))]
fn in_listed_order(listed: &[Device], mut scanned: Vec<Device>) -> Vec<Device> {
    let mut ordered = Vec::with_capacity(scanned.len());
    for device in listed {
        let identity = device.stable_identity();
        let found = scanned
            .iter()
            .position(|d| d.path == device.path && d.instance_token == device.instance_token)
            // A board re-enumerated between two scans keeps its row.
            .or_else(|| {
                unique_reconnect_device_index(listed, &identity)?;
                unique_reconnect_device_index(&scanned, &identity)
            });
        if let Some(index) = found {
            ordered.push(scanned.remove(index));
        }
    }
    ordered.extend(scanned);
    ordered
}

impl EntropyApp {
    /// Connects the board the user picked, and remembers it to connect it
    /// again whenever it comes back. Only an identity that names this board
    /// alone is remembered: same-model boards without distinct serials cannot
    /// be told apart once one of them is gone.
    pub(super) fn connect_picked_device(&mut self, device_idx: usize) {
        self.remember_picked_device(device_idx);
        self.start_connect(device_idx);
    }

    /// A board picked in the device menu: connected unless it already is,
    /// and remembered either way.
    pub(super) fn pick_device(&mut self, device_idx: usize) {
        if self.selected_device != Some(device_idx) || self.pending_device_connect.is_some() {
            self.connect_picked_device(device_idx);
        } else {
            self.remember_picked_device(device_idx);
        }
    }

    /// Remembers the board the user picked, connected already or not: the
    /// pick replaces any board remembered before, and any takeover queued for
    /// it.
    fn remember_picked_device(&mut self, device_idx: usize) {
        let devices = self.device_manager.devices();
        self.remembered_device = devices
            .get(device_idx)
            .map(Device::stable_identity)
            .filter(|identity| devices.iter().filter(|d| identity.matches(d)).count() == 1);
        self.pending_takeover = None;
    }

    /// Forgets the picked board once a scan lists two boards its identity
    /// names: from then on neither proves to be the one picked, and a
    /// takeover still queued for it lapses. A request the user made stays.
    fn forget_ambiguous_remembered_device(&mut self) {
        let ambiguous = self.remembered_device.as_ref().is_some_and(|identity| {
            let devices = self.device_manager.devices();
            devices.iter().filter(|d| identity.matches(d)).count() > 1
        });
        if ambiguous {
            self.remembered_device = None;
            if self
                .pending_takeover
                .take()
                .is_some_and(|takeover| self.pending_device_connect.as_ref() == Some(&takeover))
            {
                self.pending_device_connect = None;
            }
        }
    }

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

    pub(super) fn bluetooth_discovery_owner_active(&self) -> bool {
        let selected_bluetooth = self.selected_device.and_then(|i| self.device_manager.devices().get(i))
            .is_some_and(Device::is_bluetooth_transport);
        (selected_bluetooth && (self.hid_device.is_some() || self.hid_write_task_owner_active()))
            || matches!(&self.connect_state, ConnectState::Loading { device, .. } if device.is_bluetooth_transport())
            || self.retiring_connects.iter().any(|owner| owner.device.is_bluetooth_transport())
            || self.qmk_hid_hosts.values().any(|bridge| bridge.device().is_bluetooth_transport())
    }

    pub(super) fn start_device_scan(&mut self) {
        // All entrypoints (including reconnect/manual refresh) obey the actual
        // transport-owner fence, not just the periodic scheduler.
        let usb_only = self.bluetooth_discovery_owner_active();
        if usb_only && !cfg!(any(target_os="linux",target_os="windows",target_os="macos")) { return; }
        self.start_device_scan_kind(usb_only);
    }

    pub(super) fn start_usb_device_scan(&mut self) {
        self.start_device_scan_kind(true);
    }

    fn start_device_scan_kind(&mut self, usb_only: bool) {
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
        usb_only,
        };
        #[cfg(test)]
        if let Some(requests) = &self.test_device_scan_requests {
            let _ = requests.send((usb_only, tx));
            return;
        }
        std::thread::spawn(move || {
            let result = if usb_only { DeviceManager::scan_usb_devices() }
                else { DeviceManager::scan_devices() };
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
                usb_only,
            } => match rx.try_recv() {
                Ok(result) => Some((*generation, *usb_only, result)),
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
                    *usb_only,
                    Err("HID device scan worker stopped".to_owned()),
                )),
            },
        };

        self.device_scan_state = DeviceScanState::Idle;
        if let Some((generation, usb_only, result)) = completed {
            if generation != self.connection_generation {
                log::debug!(
                    "Ignoring HID scan from connection generation {generation}; current generation is {}",
                    self.connection_generation
                );
                return;
            }
            match result {
                Ok(devices) => {
                    if usb_only {
                        // Only the USB scope is authoritative for removals. Retain
                        // every other transport, including unknown/future HID buses.
                        // All USB metadata backends publish the canonical "Usb" bus.
                        let mut merged: Vec<_> = self.device_manager.devices().iter()
                            .filter(|d| d.bus_type != "Usb").cloned().collect();
                        merged.extend(devices.into_iter().filter(|d| d.bus_type == "Usb"));
                        self.apply_device_scan_result_inner(merged, false);
                    } else { self.apply_device_scan_result(devices); }
                },
                Err(error) => log::warn!("{error}; preserving the previous device list"),
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn apply_device_scan_result(&mut self, devices: Vec<Device>) {
        self.apply_device_scan_result_inner(devices, true);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn apply_device_scan_result_inner(&mut self, devices: Vec<Device>, allow_discovery_connect: bool) {
        let devices = in_listed_order(self.device_manager.devices(), devices);
        if !allow_discovery_connect {
            // Inventory is not permission to retry/open any transport. Keep the
            // chosen endpoint by identity; only retire an actually removed or
            // re-enumerated selected USB owner. Bluetooth absence cannot be
            // inferred from a partial USB snapshot.
            let previous = self.selected_device.and_then(|i| self.device_manager.devices().get(i)).cloned();
            // Stable identity need not be unique (missing or duplicated serials).
            // Prefer the actual retained enumeration, never the first same-model
            // endpoint. Only an unambiguous old AND new identity may fall back.
            let selected = previous.as_ref().and_then(|previous| {
                let identity = previous.stable_identity();
                devices.iter().position(|d| {
                    d.path == previous.path && d.instance_token == previous.instance_token
                        && d.stable_identity() == identity
                }).or_else(|| {
                    let old_unique = self.device_manager.devices().iter()
                        .filter(|d| d.stable_identity() == identity).count() == 1;
                    if !old_unique { return None; }
                    let mut matches = devices.iter().enumerate()
                        .filter_map(|(i, d)| (d.stable_identity() == identity).then_some(i));
                    let first = matches.next()?;
                    matches.next().is_none().then_some(first)
                })
            });
            self.device_manager.replace_devices(devices);
            self.forget_ambiguous_remembered_device();
            if let Some(previous) = previous {
                let changed = selected.is_some_and(|i| {
                    let current = &self.device_manager.devices()[i];
                    previous.path != current.path || usb_endpoint_was_reenumerated(&previous, current)
                });
                if !previous.is_bluetooth_transport() && (selected.is_none() || changed) {
                    // Normal unplug/re-enumeration is not an open failure.
                    // Return to the neutral picker; real connection errors keep
                    // their own status and existing error presentation.
                    // The USB-only merge may have moved a Bluetooth device
                    // into the old USB index. Do not let disconnect cleanup
                    // interpret that stale index as the owner being retired.
                    self.selected_device = None;
                    self.clear_connected_keyboard_state("");
                }
                self.selected_device = selected;
            }
            self.retain_connected_qmk_hid_host_bridges();
            return;
        }
        if let ConnectState::Reconnecting(reconnect) = &self.connect_state {
            let reconnect = reconnect.clone();
            let reconnect_device_index =
                unique_reconnect_device_index(&devices, &reconnect.identity);
            self.device_manager.replace_devices(devices);
            self.forget_ambiguous_remembered_device();
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
        self.forget_ambiguous_remembered_device();
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

        // The board picked last comes back on its own, however many others
        // are listed, and takes over from a board connected without a pick,
        // such as the one left alone while it was gone: picking that board
        // would have replaced the memory. It has been opened before, so no
        // udev gate applies.
        if !was_loading {
            let idle = self.selected_device.is_none() && self.layout.is_none();
            let remembered = self.remembered_device.as_ref().and_then(|identity| {
                let index = unique_reconnect_device_index(self.device_manager.devices(), identity)?;
                let other_connected = previous_device
                    .as_ref()
                    .is_some_and(|device| !identity.matches(device));
                (idle || other_connected).then_some(index)
            });
            if let Some(index) = remembered {
                // The previous board's index is from the last scan; the
                // handoff in start_connect needs it in this one.
                self.selected_device = previous_device_key.as_ref().and_then(|key| {
                    self.device_manager
                        .devices()
                        .iter()
                        .position(|dev| dev.display_name_cache_key() == *key)
                });
                self.start_connect(index);
                // Queued behind a write, it is still the remembered board's.
                self.pending_takeover = self.pending_device_connect.clone();
                return;
            }
        }

        #[cfg(target_os = "linux")]
        if self.selected_device.is_none()
            && self.layout.is_none()
            && !was_loading
            && !super::app_settings_ui::linux_device_udev_rules_installed()
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
                } else if (self.layout.is_none()
                    || (self.hid_device.is_none() && !self.hid_write_task_owner_active()))
                    && !was_loading
                {
                    // A failed USB settings write can drop the HID owner while
                    // discovery still reports the same path/instance. Do not
                    // wait for another OS re-enumeration to reopen it.
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
    fn successful_scan_clears_unplugged_selected_usb_workspace_before_device_switch() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        let macropad = scan_test_device("Macropad");
        let k04 = scan_test_device("K04");
        app.device_manager
            .replace_devices(vec![macropad.clone(), k04.clone()]);
        app.selected_device = Some(0);
        app.current_device_name = macropad.name.clone();
        app.layout = Some(KeyboardLayout {
            name: macropad.name.clone(),
            rows: 0,
            cols: 0,
            keys: Vec::new(),
            encoders: Vec::new(),
            layers: vec![Vec::new()],
            encoder_layers: vec![Vec::new()],
            layer_names: vec!["Base".to_owned()],
            custom_keycodes: Vec::new(),
            layout_options: Vec::new(),
            live_features: Default::default(),
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
        });
        let (hid, _recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid);
        app.device_display_names
            .insert(macropad.display_name_cache_key(), macropad.name.clone());
        let old_generation = app.connection_generation;
        let (launch_tx, requests) = mpsc::channel();
        app.test_connect_requests = Some(launch_tx);
        let (scan_tx, scan_rx) = mpsc::channel();
        scan_tx.send(Ok(vec![k04.clone()])).unwrap();
        app.device_scan_state = DeviceScanState::Scanning {
            rx: scan_rx,
            started_at: std::time::Instant::now(),
            generation: old_generation,
            timeout_logged: false,
        usb_only: false,
        };

        // Exercise the real completed-scan lifecycle, not a manual device switch.
        app.poll_device_scan(&ctx);

        assert!(
            app.layout.is_none(),
            "the unplugged USB workspace is retired"
        );
        assert!(app.hid_device.is_none());
        assert!(matches!(app.connect_state, ConnectState::Loading { .. }));
        assert!(app.status_msg.starts_with("Connecting to K04"));
        assert!(!app
            .device_display_names
            .contains_key(&macropad.display_name_cache_key()));
        assert_eq!(app.device_manager.devices().len(), 1);
        assert_eq!(
            requests.try_recv().unwrap().0.stable_identity(),
            k04.stable_identity()
        );
    }

    #[test]
    fn same_usb_endpoint_reopens_after_settings_write_loses_hid_owner() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&cc);
        let (launch_tx, requests) = mpsc::channel();
        app.test_connect_requests = Some(launch_tx);
        let device = scan_test_device("SM62");
        app.device_manager.replace_devices(vec![device.clone()]);
        app.selected_device = Some(0);
        app.layout = Some(KeyboardLayout {
            name: "SM62".to_owned(), rows: 0, cols: 0,
            keys: vec![], encoders: vec![], layers: vec![vec![]],
            encoder_layers: vec![vec![]], layer_names: vec!["Base".to_owned()],
            custom_keycodes: vec![], layout_options: vec![],
            live_features: Default::default(), supports_rgb: false,
            lighting_mode: None, firmware: FirmwareProtocol::Vial,
        });
        assert!(app.hid_device.is_none());

        app.apply_device_scan_result(vec![device.clone()]);

        assert_eq!(requests.try_recv().unwrap().0.path, device.path);
        assert!(matches!(app.connect_state, ConnectState::Loading { .. }));
    }

    #[test]
    fn the_board_picked_last_is_connected_again_among_several() {
        let mut app = EntropyApp::new_inert_for_test();
        let (launch_tx, requests) = mpsc::channel();
        app.test_connect_requests = Some(launch_tx);
        let phenom = scan_test_device("Phenom");
        let qube = scan_test_device("Qube");
        let mut mini = scan_test_device("Mini");
        mini.bus_type = "Bluetooth".to_owned();
        app.device_manager
            .replace_devices(vec![phenom.clone(), qube.clone(), mini.clone()]);
        // A connect that ended with its board gone.
        let unplugged = |app: &mut EntropyApp| {
            app.connect_state = ConnectState::Idle;
            app.selected_device = None;
        };

        app.connect_picked_device(1);
        assert_eq!(requests.try_recv().unwrap().0.name, "Qube");
        unplugged(&mut app);
        app.apply_device_scan_result(vec![phenom.clone(), mini.clone()]);
        assert!(requests.try_recv().is_err(), "another board was connected");

        let mut replugged = qube.clone();
        replugged.path = format!("{}-replugged", qube.path);
        replugged.instance_token = "replugged-instance".to_owned();
        app.apply_device_scan_result(vec![phenom.clone(), replugged.clone(), mini.clone()]);
        let (connected, _) = requests
            .try_recv()
            .expect("the picked board was not connected again");
        assert_eq!(connected.path, replugged.path);

        // Picking another board replaces the one remembered.
        unplugged(&mut app);
        app.connect_picked_device(0);
        assert_eq!(requests.try_recv().unwrap().0.name, "Phenom");
        unplugged(&mut app);
        app.apply_device_scan_result(vec![phenom, replugged, mini]);
        assert_eq!(requests.try_recv().unwrap().0.name, "Phenom");
    }

    /// The same model without a serial: nothing tells two of them apart.
    fn serial_less_twin(path: &str) -> Device {
        let mut twin = scan_test_device("Twin");
        twin.serial_number.clear();
        twin.path = format!("/nonexistent/entropy-test-{path}");
        twin.instance_token = format!("{path}-instance");
        twin
    }

    /// The picked board, opened and loaded.
    fn connected_to(app: &mut EntropyApp, index: usize) {
        let device = app.device_manager.devices()[index].clone();
        app.connect_state = ConnectState::Idle;
        app.selected_device = Some(index);
        app.layout = Some(KeyboardLayout {
            name: device.name.clone(),
            rows: 0,
            cols: 0,
            keys: Vec::new(),
            encoders: Vec::new(),
            layers: vec![Vec::new()],
            encoder_layers: vec![Vec::new()],
            layer_names: vec!["Base".to_owned()],
            custom_keycodes: Vec::new(),
            layout_options: Vec::new(),
            live_features: Default::default(),
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
        });
        app.hid_device = Some(crate::hid::HidDevice::test_device().0);
    }

    #[test]
    fn a_board_its_twin_could_be_is_not_connected_again() {
        let mut app = EntropyApp::new_inert_for_test();
        let (launch_tx, requests) = mpsc::channel();
        app.test_connect_requests = Some(launch_tx);
        let a = serial_less_twin("a");
        let b = serial_less_twin("b");
        let other = scan_test_device("Other");
        // Both twins gone and the connection with them.
        let gone = |app: &mut EntropyApp| {
            app.connect_state = ConnectState::Idle;
            app.selected_device = None;
            app.layout = None;
            app.hid_device = None;
        };

        // Picked while its twin was listed: the pick names either.
        app.device_manager
            .replace_devices(vec![a.clone(), b.clone(), other.clone()]);
        app.connect_picked_device(0);
        assert_eq!(requests.try_recv().unwrap().0.path, a.path);
        assert!(app.remembered_device.is_none());
        gone(&mut app);
        app.apply_device_scan_result(vec![b.clone(), other.clone()]);
        assert!(
            requests.try_recv().is_err(),
            "the twin was opened as the pick"
        );

        // Picked alone, then listed beside its twin: neither proves to be it.
        app.device_manager
            .replace_devices(vec![a.clone(), other.clone()]);
        app.connect_picked_device(0);
        assert_eq!(requests.try_recv().unwrap().0.path, a.path);
        assert!(app.remembered_device.is_some());
        connected_to(&mut app, 0);
        app.apply_device_scan_result(vec![a, b.clone(), other.clone()]);
        assert!(app.remembered_device.is_none());
        gone(&mut app);
        app.apply_device_scan_result(vec![b, other]);
        assert!(
            requests.try_recv().is_err(),
            "the twin was opened as the pick"
        );
    }

    #[test]
    fn the_picked_board_takes_over_from_the_one_left_alone() {
        let mut app = EntropyApp::new_inert_for_test();
        let (launch_tx, requests) = mpsc::channel();
        app.test_connect_requests = Some(launch_tx);
        let picked = scan_test_device("Picked");
        let other = scan_test_device("Other");
        app.device_manager
            .replace_devices(vec![picked.clone(), other.clone()]);
        app.connect_picked_device(0);
        assert_eq!(requests.try_recv().unwrap().0.name, "Picked");
        connected_to(&mut app, 0);

        // Unplugged: the board left alone is connected on its own.
        app.apply_device_scan_result(vec![other.clone()]);
        assert_eq!(requests.try_recv().unwrap().0.name, "Other");
        connected_to(&mut app, 0);
        app.apply_device_scan_result(vec![other.clone()]);
        assert!(requests.try_recv().is_err());

        // Back: the picked board takes over, and keeps the connection after.
        app.apply_device_scan_result(vec![other.clone(), picked.clone()]);
        assert_eq!(requests.try_recv().unwrap().0.name, "Picked");
        connected_to(&mut app, 1);
        app.apply_device_scan_result(vec![other.clone(), picked.clone()]);
        assert!(requests.try_recv().is_err(), "the connection moved again");

        // Picking the other board makes it the one remembered.
        app.connect_picked_device(0);
        assert_eq!(requests.try_recv().unwrap().0.name, "Other");
        connected_to(&mut app, 0);
        app.apply_device_scan_result(vec![other, picked]);
        assert!(
            requests.try_recv().is_err(),
            "the manual choice was overridden"
        );
    }

    #[test]
    fn picking_the_connected_board_makes_it_the_one_remembered() {
        let mut app = EntropyApp::new_inert_for_test();
        let (launch_tx, requests) = mpsc::channel();
        app.test_connect_requests = Some(launch_tx);
        let a = scan_test_device("A");
        let b = scan_test_device("B");
        app.device_manager
            .replace_devices(vec![a.clone(), b.clone()]);
        app.pick_device(0);
        assert_eq!(requests.try_recv().unwrap().0.name, "A");
        connected_to(&mut app, 0);
        app.apply_device_scan_result(vec![b.clone()]);
        assert_eq!(requests.try_recv().unwrap().0.name, "B");
        connected_to(&mut app, 0);

        // Picked as it is: nothing to open, but it is the choice now.
        app.pick_device(0);
        assert!(
            requests.try_recv().is_err(),
            "the connected board was reopened"
        );
        app.apply_device_scan_result(vec![b, a]);

        assert!(requests.try_recv().is_err(), "the earlier pick took over");
        assert_eq!(app.selected_device, Some(0));
    }

    /// The same model with the same serial: as indistinct as no serial.
    fn duplicate_serial_twin(path: &str) -> Device {
        let mut twin = scan_test_device("Twin");
        twin.path = format!("/nonexistent/entropy-test-{path}");
        twin.instance_token = format!("{path}-instance");
        twin
    }

    /// Holds the connection the way a combo write does, until it is dropped.
    fn busy_with_a_write(app: &mut EntropyApp) {
        let (_, receiver) = mpsc::channel();
        app.combo_write_task = Some(ComboWriteTask {
            receiver,
            revision: 0,
        });
    }

    #[test]
    fn a_queued_takeover_lapses_once_its_board_has_a_twin() {
        for twin in [serial_less_twin, duplicate_serial_twin] {
            let mut app = EntropyApp::new_inert_for_test();
            let (launch_tx, requests) = mpsc::channel();
            app.test_connect_requests = Some(launch_tx);
            let a = twin("a");
            let b = twin("b");
            let other = scan_test_device("Other");
            app.device_manager
                .replace_devices(vec![a.clone(), other.clone()]);
            app.pick_device(0);
            assert_eq!(requests.try_recv().unwrap().0.path, a.path);
            connected_to(&mut app, 0);
            app.apply_device_scan_result(vec![other.clone()]);
            assert_eq!(requests.try_recv().unwrap().0.name, "Other");
            connected_to(&mut app, 0);

            // A comes back while a write holds Other: its takeover waits.
            busy_with_a_write(&mut app);
            app.apply_device_scan_result(vec![other.clone(), a.clone()]);
            assert_eq!(app.pending_device_connect, Some(a.stable_identity()));
            app.apply_device_scan_result(vec![other.clone(), a, b.clone()]);
            app.apply_device_scan_result(vec![other.clone()]);
            app.combo_write_task = None;
            app.resume_pending_device_connect();
            app.apply_device_scan_result(vec![other, b]);

            assert!(
                requests.try_recv().is_err(),
                "the twin was opened as the pick"
            );
            assert!(app.pending_device_connect.is_none());
        }
    }

    #[test]
    fn a_queued_pick_outlives_its_board_getting_a_twin() {
        let mut app = EntropyApp::new_inert_for_test();
        let (launch_tx, requests) = mpsc::channel();
        app.test_connect_requests = Some(launch_tx);
        let a = serial_less_twin("a");
        let b = serial_less_twin("b");
        let other = scan_test_device("Other");
        app.device_manager
            .replace_devices(vec![other.clone(), a.clone()]);
        connected_to(&mut app, 0);
        busy_with_a_write(&mut app);

        app.pick_device(1);
        app.apply_device_scan_result(vec![other, a.clone(), b]);

        assert!(requests.try_recv().is_err());
        assert!(app.remembered_device.is_none());
        assert_eq!(
            app.pending_device_connect,
            Some(a.stable_identity()),
            "the user's request was dropped"
        );
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
            "Connect timeout — device did not finish loading"
        ));
        assert!(should_wait_for_manual_device_selection(
            "Connect failed: HID timeout — device did not respond"
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
        usb_only: false,
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
        usb_only: false,
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
        usb_only: false,
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

    fn device_names(app: &EntropyApp) -> Vec<String> {
        app.device_manager
            .devices()
            .iter()
            .map(|device| device.name.clone())
            .collect()
    }

    #[test]
    fn scans_keep_the_rows_of_the_listed_devices() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        let (launch_tx, _requests) = mpsc::channel();
        app.test_connect_requests = Some(launch_tx);
        let phenom = scan_test_device("Phenom");
        let qube = scan_test_device("Qube");
        let mut mini = scan_test_device("Mini");
        mini.bus_type = "Bluetooth".to_owned();
        app.device_manager
            .replace_devices(vec![phenom.clone(), qube.clone(), mini.clone()]);

        // A USB-only scan keeps the other transports and adds its own after
        // them, in its own order.
        let (scan_tx, scan_rx) = mpsc::channel();
        scan_tx
            .send(Ok(vec![qube.clone(), phenom.clone()]))
            .unwrap();
        app.device_scan_state = DeviceScanState::Scanning {
            rx: scan_rx,
            started_at: std::time::Instant::now(),
            generation: app.connection_generation,
            timeout_logged: false,
            usb_only: true,
        };
        app.poll_device_scan(&ctx);
        assert_eq!(device_names(&app), ["Phenom", "Qube", "Mini"]);

        // A full scan lists Bluetooth first here.
        app.apply_device_scan_result(vec![mini.clone(), qube.clone(), phenom.clone()]);
        assert_eq!(device_names(&app), ["Phenom", "Qube", "Mini"]);

        // A board that turns up goes last; one re-enumerated between two
        // scans keeps its row.
        let mut replugged = qube.clone();
        replugged.path = format!("{}-replugged", qube.path);
        replugged.instance_token = "replugged-instance".to_owned();
        let k03 = scan_test_device("K03");
        app.apply_device_scan_result(vec![k03, mini, replugged, phenom]);
        assert_eq!(device_names(&app), ["Phenom", "Qube", "Mini", "K03"]);
    }
}
