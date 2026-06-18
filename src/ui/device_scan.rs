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

        self.selected_device = Some(0);
        self.start_connect(0);
    }
}
