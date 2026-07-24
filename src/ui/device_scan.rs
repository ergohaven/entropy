use super::*;

fn should_wait_for_manual_device_selection(status_msg: &str) -> bool {
    status_msg.starts_with("Open failed:") || status_msg.starts_with("Connect timeout")
}

fn should_auto_connect_only_device(device_count: usize) -> bool {
    device_count == 1
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
            ConnectState::Idle | ConnectState::Loading { .. } => None,
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
        if let ConnectState::Reconnecting(reconnect) = &self.connect_state {
            let reconnect = reconnect.clone();
            let reconnect_device_index =
                unique_reconnect_device_index(&devices, &reconnect.identity);
            self.device_manager.replace_devices(devices);
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

        let previous_device_key = self
            .selected_device
            .and_then(|idx| self.device_manager.devices().get(idx))
            .map(Device::display_name_cache_key);
        let was_loading = matches!(self.connect_state, ConnectState::Loading { .. });

        self.device_manager.replace_devices(devices);
        let connected_display_name_keys: std::collections::HashSet<String> = self
            .device_manager
            .devices()
            .iter()
            .map(Device::display_name_cache_key)
            .collect();
        self.device_display_names
            .retain(|key, _| connected_display_name_keys.contains(key));

        if self.device_manager.devices().is_empty() {
            if self.selected_device.is_some() || self.layout.is_some() || was_loading {
                self.selected_device = None;
                self.clear_connected_keyboard_state("No device detected");
            } else {
                self.qmk_hid_hosts.clear();
            }
            return;
        }

        if self.selected_device.is_none()
            && self.layout.is_none()
            && !was_loading
            && should_wait_for_manual_device_selection(&self.status_msg)
        {
            self.qmk_hid_hosts.clear();
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
                if self.layout.is_none() && !was_loading {
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
                self.qmk_hid_hosts.clear();
            }
            return;
        }

        self.selected_device = Some(0);
        self.start_connect(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn reconnect_selects_only_the_same_bluetooth_identity() {
        let mut expected = Device {
            name: "K:04".to_owned(),
            vendor_id: 0xE126,
            product_id: 0x0074,
            manufacturer: "Ergohaven".to_owned(),
            serial_number: "AA:BB:CC:DD:EE:FF".to_owned(),
            bus_type: "Bluetooth".to_owned(),
            path: "/dev/hidraw4".to_owned(),
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
