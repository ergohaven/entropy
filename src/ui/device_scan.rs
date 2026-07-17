use super::*;

fn should_wait_for_manual_device_selection(status_msg: &str) -> bool {
    status_msg.starts_with("Open failed:") || status_msg.starts_with("Connect timeout")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeviceScanResultAction {
    Wait,
    Apply,
}

fn device_scan_result_action(refresh_blocked: bool) -> DeviceScanResultAction {
    if refresh_blocked {
        DeviceScanResultAction::Wait
    } else {
        DeviceScanResultAction::Apply
    }
}

fn same_keyboard_after_reenumeration(previous: &Device, candidate: &Device) -> bool {
    previous.vendor_id == candidate.vendor_id
        && previous.product_id == candidate.product_id
        && !previous.serial_number.is_empty()
        && previous.serial_number == candidate.serial_number
}

impl EntropyApp {
    pub(super) fn start_device_scan(&mut self) {
        self.start_device_scan_with_trigger(DeviceScanTrigger::Automatic);
    }

    pub(super) fn start_manual_device_scan(&mut self) -> bool {
        self.start_device_scan_with_trigger(DeviceScanTrigger::Manual)
    }

    fn start_device_scan_with_trigger(&mut self, trigger: DeviceScanTrigger) -> bool {
        if !matches!(self.device_scan_state, DeviceScanState::Idle) {
            return false;
        }
        if self.device_refresh_blocked() {
            return false;
        }

        #[cfg(target_os = "macos")]
        if crate::hid::macos_hid_scan_disabled_for_rosetta() {
            if self.status_msg.is_empty() {
                self.status_msg = crate::hid::macos_rosetta_hid_status_message().into();
            }
            return false;
        }

        let (tx, rx) = mpsc::channel();
        self.device_scan_state = DeviceScanState::Scanning { rx, trigger };
        std::thread::spawn(move || {
            let _ = tx.send(DeviceManager::scan_devices());
        });
        true
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn device_refresh_blocked(&self) -> bool {
        self.hid_write_task_active()
            || matches!(self.connect_state, ConnectState::Loading { .. })
            || self.layer_write_task.is_some()
            || !self.pending_tap_hold_numeric_writes.is_empty()
            || self.tap_hold_numeric_write_due.is_some()
            || self.keycode_picker.macros_dirty
            || self.keycode_picker.tap_dance_dirty
            || self.combo_dirty
            || self.combo_term_dirty
            || self.key_override_dirty
            || self.pending_entlayout_import_path.is_some()
            || self.pending_entsettings_import_path.is_some()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_device_scan(&mut self, ctx: &egui::Context) {
        let trigger = match &self.device_scan_state {
            DeviceScanState::Idle => return,
            DeviceScanState::Scanning { trigger, .. } => *trigger,
        };
        let action = device_scan_result_action(self.device_refresh_blocked());
        match action {
            DeviceScanResultAction::Wait => {
                ctx.request_repaint_after(std::time::Duration::from_millis(25));
                return;
            }
            DeviceScanResultAction::Apply => {}
        }

        let devices = match &self.device_scan_state {
            DeviceScanState::Idle => return,
            DeviceScanState::Scanning { rx, .. } => match rx.try_recv() {
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
            self.apply_device_scan_result(devices, trigger);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn apply_device_scan_result(&mut self, devices: Vec<Device>, trigger: DeviceScanTrigger) {
        let previous_device = self
            .selected_device
            .and_then(|idx| self.device_manager.devices().get(idx))
            .cloned();
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
                self.clear_connected_keyboard_state(crate::i18n::tr(
                    self.app_settings.language,
                    TrKey::NoDevicesFound,
                ));
            } else {
                self.qmk_hid_hosts.clear();
                if trigger == DeviceScanTrigger::Manual {
                    self.status_msg =
                        crate::i18n::tr(self.app_settings.language, TrKey::NoDevicesFound).into();
                }
            }
            return;
        }

        if trigger == DeviceScanTrigger::Manual {
            self.status_msg = match self.app_settings.language {
                crate::i18n::Language::Russian => "Список устройств обновлён",
                crate::i18n::Language::English => "Device list refreshed",
            }
            .into();
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
        {
            self.qmk_hid_hosts.clear();
            return;
        }

        if let Some(previous_device) = previous_device {
            if let Some(idx) = self.device_manager.devices().iter().position(|candidate| {
                candidate.display_name_cache_key() == previous_device.display_name_cache_key()
                    || same_keyboard_after_reenumeration(&previous_device, candidate)
            }) {
                let reenumerated = self.device_manager.devices()[idx].display_name_cache_key()
                    != previous_device.display_name_cache_key();
                self.selected_device = Some(idx);
                if (trigger == DeviceScanTrigger::Manual || reenumerated || self.layout.is_none())
                    && !was_loading
                {
                    self.start_connect(idx);
                } else {
                    self.sync_qmk_hid_host_bridges();
                }
                return;
            }
            if trigger == DeviceScanTrigger::Automatic {
                self.selected_device = None;
                self.clear_connected_keyboard_state(crate::i18n::tr(
                    self.app_settings.language,
                    TrKey::NoDevicesFound,
                ));
                return;
            }
        }

        self.selected_device = Some(0);
        self.start_connect(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> EntropyApp {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        EntropyApp::new(&creation_context)
    }

    fn test_device(path: &str) -> Device {
        Device {
            name: "Test Keyboard".to_owned(),
            vendor_id: 0x1209,
            product_id: 0x2327,
            manufacturer: "Entropy".to_owned(),
            serial_number: "serial".to_owned(),
            bus_type: "USB".to_owned(),
            path: path.to_owned(),
            firmware: FirmwareProtocol::Vial,
        }
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
    fn pending_settings_work_defers_scan_results() {
        assert_eq!(
            device_scan_result_action(true),
            DeviceScanResultAction::Wait
        );
    }

    #[test]
    fn automatic_scan_applies_when_connected_to_detect_disconnects() {
        assert_eq!(
            device_scan_result_action(false),
            DeviceScanResultAction::Apply
        );
    }

    #[test]
    fn automatic_empty_scan_clears_connected_keyboard_state() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        let device = test_device("/test/keyboard");
        app.device_manager.replace_devices(vec![device]);
        app.selected_device = Some(0);
        app.hid_device = Some(crate::hid::HidDevice::test_device().0);
        let (tx, rx) = mpsc::channel();
        tx.send(Vec::new()).unwrap();
        app.device_scan_state = DeviceScanState::Scanning {
            rx,
            trigger: DeviceScanTrigger::Automatic,
        };

        app.poll_device_scan(&ctx);

        assert!(app.hid_device.is_none());
        assert!(app.selected_device.is_none());
        assert!(matches!(app.connect_state, ConnectState::Idle));
    }

    #[test]
    fn manual_same_device_scan_reopens_stale_hid() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        let device = test_device("/test/keyboard");
        app.device_manager.replace_devices(vec![device.clone()]);
        app.selected_device = Some(0);
        app.hid_device = Some(crate::hid::HidDevice::test_device().0);
        let (tx, rx) = mpsc::channel();
        tx.send(vec![device]).unwrap();
        app.device_scan_state = DeviceScanState::Scanning {
            rx,
            trigger: DeviceScanTrigger::Manual,
        };

        app.poll_device_scan(&ctx);

        assert!(app.hid_device.is_none());
        assert!(matches!(app.connect_state, ConnectState::Loading { .. }));
    }

    #[test]
    fn refresh_data_with_pending_combo_keeps_connected_session() {
        let mut app = test_app();
        app.device_manager
            .replace_devices(vec![test_device("/test/keyboard")]);
        app.selected_device = Some(0);
        app.hid_device = Some(crate::hid::HidDevice::test_device().0);
        app.combo_dirty = true;

        app.refresh_current_device_data();

        assert!(app.hid_device.is_some());
        assert!(matches!(app.connect_state, ConnectState::Idle));
    }
}
