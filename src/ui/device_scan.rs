use super::*;

impl EntropyApp {
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
        self.device_scan_state = DeviceScanState::Scanning(rx);
        std::thread::spawn(move || {
            let _ = tx.send(DeviceManager::scan_devices());
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_device_scan(&mut self, ctx: &egui::Context) {
        let devices = match &self.device_scan_state {
            DeviceScanState::Idle => return,
            DeviceScanState::Scanning(rx) => match rx.try_recv() {
                Ok(devices) => Some(devices),
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(25));
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => Some(Vec::new()),
            },
        };

        self.device_scan_state = DeviceScanState::Idle;
        if let Some(devices) = devices {
            self.apply_device_scan_result(devices);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn apply_device_scan_result(&mut self, devices: Vec<Device>) {
        // Number of consecutive scans the selected device must stay absent
        // before the session is torn down. Scans run ~1s apart, so this debounces
        // brief disappearances (e.g. USB re-enumeration after a large import)
        // without noticeably delaying a genuine unplug.
        const DEVICE_ABSENT_CLEAR_SCANS: u32 = 3;

        let was_loading = matches!(self.connect_state, ConnectState::Loading { .. });

        if devices.is_empty() {
            let had_session =
                self.selected_device.is_some() || self.layout.is_some() || was_loading;
            if had_session {
                self.device_absent_scans = self.device_absent_scans.saturating_add(1);
                if self.device_absent_scans < DEVICE_ABSENT_CLEAR_SCANS {
                    // Keep the device list, selection and layout so the session
                    // (and layout-dependent actions like export) survive a
                    // transient disappearance. Do not touch the device manager.
                    return;
                }
                self.device_manager.replace_devices(devices);
                self.device_display_names.clear();
                self.selected_device = None;
                self.clear_connected_keyboard_state("No device detected");
            } else {
                self.device_manager.replace_devices(devices);
                self.device_display_names.clear();
                self.qmk_hid_hosts.clear();
            }
            return;
        }

        let previous_device_key = self
            .selected_device
            .and_then(|idx| self.device_manager.devices().get(idx))
            .map(Device::display_name_cache_key);
        // If the device just came back from a transient absence, the previously
        // opened HID handle is now stale and must be reopened.
        let recovered_from_absence = self.device_absent_scans > 0;
        self.device_absent_scans = 0;

        self.device_manager.replace_devices(devices);
        let connected_display_name_keys: std::collections::HashSet<String> = self
            .device_manager
            .devices()
            .iter()
            .map(Device::display_name_cache_key)
            .collect();
        self.device_display_names
            .retain(|key, _| connected_display_name_keys.contains(key));

        #[cfg(target_os = "linux")]
        if self.selected_device.is_none()
            && self.layout.is_none()
            && !was_loading
            && !super::app_settings_ui::linux_vial_udev_rules_installed()
        {
            self.qmk_hid_hosts.clear();
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
                if (self.layout.is_none() || recovered_from_absence) && !was_loading {
                    self.start_connect(idx);
                } else {
                    self.sync_qmk_hid_host_bridges();
                }
                return;
            }
        }

        self.selected_device = Some(0);
        self.start_connect(0);
    }
}
