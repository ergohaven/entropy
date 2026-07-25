use super::*;

#[cfg(not(target_arch = "wasm32"))]
const BATTERY_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);
#[cfg(not(target_arch = "wasm32"))]
const BATTERY_REFRESH_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
#[cfg(not(target_arch = "wasm32"))]
const INITIAL_BATTERY_REFRESH_DELAY: std::time::Duration = std::time::Duration::from_secs(2);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub(super) enum VialHidOperation {
    UnlockStart,
    UnlockPoll,
    Lock,
    Matrix {
        rows: usize,
        cols: usize,
        rmk_byte_order: bool,
        remember_ever_pressed: bool,
    },
    BatteryRefresh,
    KeyWrite {
        layer: usize,
        key_index: usize,
        row: u8,
        col: u8,
        old_keycode: u16,
        keycode: u16,
        is_undo: bool,
    },
    EncoderWrite {
        layer: usize,
        encoder_visual_index: usize,
        encoder_index: u8,
        direction: u8,
        old_keycode: u16,
        keycode: u16,
        is_undo: bool,
    },
    Deferred(super::device_deferred_load::DeferredLoadRequest),
}

#[cfg(not(target_arch = "wasm32"))]
enum VialHidOutcome {
    UnlockStarted {
        unlocked: bool,
        keys: Vec<(u8, u8)>,
    },
    UnlockPolled {
        unlocked: bool,
        in_progress: bool,
        counter: u8,
    },
    Locked,
    Matrix(Vec<bool>),
    Battery(Option<crate::hid::BatteryHalves>),
    KeyWritten,
    EncoderWritten,
    Deferred(super::device_deferred_load::DeferredLoadPayload),
}

#[cfg(not(target_arch = "wasm32"))]
struct VialHidTaskResult {
    hid_device: Option<crate::hid::HidDevice>,
    operation: VialHidOperation,
    outcome: Result<VialHidOutcome, String>,
    disconnected: bool,
    generation: u64,
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct VialHidTask {
    receiver: std::sync::mpsc::Receiver<VialHidTaskResult>,
    operation: VialHidOperation,
    generation: u64,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VialHidTaskStart {
    Started,
    Busy,
    NoDevice,
}

#[cfg(not(target_arch = "wasm32"))]
fn run_vial_hid_operation(
    hid: &crate::hid::HidDevice,
    operation: VialHidOperation,
) -> anyhow::Result<VialHidOutcome> {
    match operation {
        VialHidOperation::UnlockStart => {
            let (unlocked, keys) = hid.get_unlock_status()?;
            if !unlocked {
                hid.unlock_start()?;
            }
            Ok(VialHidOutcome::UnlockStarted { unlocked, keys })
        }
        VialHidOperation::UnlockPoll => {
            let (unlocked, in_progress, counter) = hid.unlock_poll()?;
            Ok(VialHidOutcome::UnlockPolled {
                unlocked,
                in_progress,
                counter,
            })
        }
        VialHidOperation::Lock => {
            hid.lock()?;
            Ok(VialHidOutcome::Locked)
        }
        VialHidOperation::Matrix {
            rows,
            cols,
            rmk_byte_order,
            ..
        } => hid
            .get_switch_matrix_with_rmk_byte_order(rows, cols, rmk_byte_order)
            .map(VialHidOutcome::Matrix),
        VialHidOperation::BatteryRefresh => hid.get_battery_halves().map(VialHidOutcome::Battery),
        VialHidOperation::KeyWrite {
            layer,
            row,
            col,
            keycode,
            ..
        } => hid
            .set_keycode(layer as u8, row, col, keycode)
            .map(|()| VialHidOutcome::KeyWritten),
        VialHidOperation::EncoderWrite {
            layer,
            encoder_index,
            direction,
            keycode,
            ..
        } => hid
            .set_encoder(layer as u8, encoder_index, direction, keycode)
            .map(|()| VialHidOutcome::EncoderWritten),
        VialHidOperation::Deferred(request) => {
            super::device_deferred_load::run_deferred_load(hid, &request)
                .map(VialHidOutcome::Deferred)
        }
    }
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn schedule_initial_battery_refresh(&mut self) {
        self.next_battery_refresh_at = self
            .device_about_info
            .as_ref()
            .filter(|info| info.supports_battery_halves)
            .map(|_| std::time::Instant::now() + INITIAL_BATTERY_REFRESH_DELAY);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn schedule_next_battery_refresh(&mut self) {
        self.next_battery_refresh_at = self
            .device_about_info
            .as_ref()
            .filter(|info| info.supports_battery_halves)
            .map(|_| std::time::Instant::now() + BATTERY_REFRESH_INTERVAL);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn maybe_start_periodic_battery_refresh(
        &mut self,
        ctx: &egui::Context,
        main_window_hidden_to_tray: bool,
    ) {
        if main_window_hidden_to_tray
            || self.bluetooth_reconnect_active()
            || !self
                .device_about_info
                .as_ref()
                .map(|info| info.supports_battery_halves)
                .unwrap_or(false)
        {
            return;
        }
        let Some(next_refresh_at) = self.next_battery_refresh_at else {
            return;
        };
        let now = std::time::Instant::now();
        if now < next_refresh_at {
            ctx.request_repaint_after(next_refresh_at.saturating_duration_since(now));
            return;
        }

        match self.start_vial_hid_operation(ctx, VialHidOperation::BatteryRefresh) {
            VialHidTaskStart::Started => {
                self.next_battery_refresh_at = None;
            }
            VialHidTaskStart::Busy => {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
            VialHidTaskStart::NoDevice => {
                self.next_battery_refresh_at = Some(now + BATTERY_REFRESH_RETRY_INTERVAL);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn vial_hid_task_active(&self) -> bool {
        self.vial_hid_task.is_some()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn vial_hid_background_layer_active(&self) -> bool {
        self.vial_hid_task.as_ref().is_some_and(|task| {
            matches!(
                &task.operation,
                VialHidOperation::Deferred(request) if request.is_background_layer()
            )
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn vial_hid_task_blocks_user_action(&self) -> bool {
        self.vial_hid_task.is_some() && !self.vial_hid_background_layer_active()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn another_hid_owner_or_write_is_pending(&self) -> bool {
        self.layer_write_task.is_some()
            || self.combo_write_task.is_some()
            || self.settings_write_task.is_some()
            || self.qmk_settings_write_busy()
            || self.qmk_settings_write_pending()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn start_vial_hid_operation(
        &mut self,
        ctx: &egui::Context,
        operation: VialHidOperation,
    ) -> VialHidTaskStart {
        if self.vial_hid_task.is_some() || self.another_hid_owner_or_write_is_pending() {
            return VialHidTaskStart::Busy;
        }
        let Some(hid_device) = self.hid_device.take() else {
            return VialHidTaskStart::NoDevice;
        };

        let generation = self.connection_generation;
        let (sender, receiver) = std::sync::mpsc::channel();
        let repaint_ctx = ctx.clone();
        let task_operation = operation.clone();
        std::thread::spawn(move || {
            #[cfg(target_os = "macos")]
            let _hid_lock = crate::hid::macos_hid_operation_lock();

            let outcome = run_vial_hid_operation(&hid_device, operation.clone());
            let disconnected = outcome
                .as_ref()
                .err()
                .map(crate::hid::is_disconnect_error)
                .unwrap_or(false);
            let hid_device = (!disconnected).then_some(hid_device);
            let outcome = outcome.map_err(|error| format!("{error:#}"));
            let _ = sender.send(VialHidTaskResult {
                hid_device,
                operation,
                outcome,
                disconnected,
                generation,
            });
            repaint_ctx.request_repaint();
        });
        self.vial_hid_task = Some(VialHidTask {
            receiver,
            operation: task_operation,
            generation,
        });
        VialHidTaskStart::Started
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn start_vial_unlock(&mut self, ctx: &egui::Context) -> VialHidTaskStart {
        self.start_vial_hid_operation(ctx, VialHidOperation::UnlockStart)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn start_vial_unlock_poll(&mut self, ctx: &egui::Context) -> VialHidTaskStart {
        self.start_vial_hid_operation(ctx, VialHidOperation::UnlockPoll)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn start_vial_lock(&mut self, ctx: &egui::Context) -> VialHidTaskStart {
        self.start_vial_hid_operation(ctx, VialHidOperation::Lock)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn start_vial_matrix_poll(
        &mut self,
        ctx: &egui::Context,
        rows: usize,
        cols: usize,
        remember_ever_pressed: bool,
    ) -> VialHidTaskStart {
        self.start_vial_hid_operation(
            ctx,
            VialHidOperation::Matrix {
                rows,
                cols,
                rmk_byte_order: self.matrix_tester_rmk_byte_order,
                remember_ever_pressed,
            },
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_vial_hid_task(&mut self, ctx: &egui::Context) {
        let received = match self.vial_hid_task.as_ref() {
            Some(task) => task.receiver.try_recv(),
            None => return,
        };

        let result = match received {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let task = self
                    .vial_hid_task
                    .take()
                    .expect("Vial HID task checked above");
                if task.generation == self.connection_generation {
                    self.hid_device = None;
                    self.finish_vial_hid_error(
                        task.operation,
                        "Vial HID worker stopped".to_owned(),
                        true,
                    );
                }
                self.resume_pending_device_connect();
                return;
            }
        };

        self.vial_hid_task = None;
        if result.generation != self.connection_generation {
            return;
        }

        self.hid_device = result.hid_device;
        match result.outcome {
            Ok(VialHidOutcome::UnlockStarted { unlocked, keys }) => {
                self.finish_vial_unlock_start(unlocked, keys);
            }
            Ok(VialHidOutcome::UnlockPolled {
                unlocked,
                in_progress,
                counter,
            }) => {
                self.finish_vial_unlock_poll(unlocked, in_progress, counter);
            }
            Ok(VialHidOutcome::Locked) => {
                self.finish_vial_lock();
            }
            Ok(VialHidOutcome::Matrix(pressed)) => {
                let remember_ever_pressed = match &result.operation {
                    VialHidOperation::Matrix {
                        remember_ever_pressed,
                        ..
                    } => *remember_ever_pressed,
                    _ => false,
                };
                self.finish_matrix_tester_poll(pressed, remember_ever_pressed);
            }
            Ok(VialHidOutcome::Battery(battery)) => {
                if let Some(info) = self.device_about_info.as_mut() {
                    info.battery_halves = battery;
                }
                self.schedule_next_battery_refresh();
            }
            Ok(VialHidOutcome::KeyWritten) => {
                if let VialHidOperation::KeyWrite {
                    layer,
                    key_index,
                    old_keycode,
                    keycode,
                    is_undo,
                    ..
                } = result.operation
                {
                    self.finish_keycode_write(layer, key_index, old_keycode, keycode, is_undo);
                }
            }
            Ok(VialHidOutcome::EncoderWritten) => {
                if let VialHidOperation::EncoderWrite {
                    layer,
                    encoder_visual_index,
                    encoder_index,
                    direction,
                    old_keycode,
                    keycode,
                    is_undo,
                } = result.operation
                {
                    self.finish_encoder_write(
                        layer,
                        encoder_visual_index,
                        encoder_index,
                        direction,
                        old_keycode,
                        keycode,
                        is_undo,
                    );
                }
            }
            Ok(VialHidOutcome::Deferred(payload)) => {
                if matches!(
                    &result.operation,
                    VialHidOperation::Deferred(request) if request.is_background_layer()
                ) {
                    self.deferred_device_load.mark_background_layer_finished();
                }
                self.finish_deferred_device_load(payload);
            }
            Err(error) => {
                self.finish_vial_hid_error(result.operation, error, result.disconnected);
            }
        }

        self.continue_pending_settings_writes(ctx);
        self.resume_pending_device_connect();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_keycode_write(
        &mut self,
        layer: usize,
        key_index: usize,
        old_keycode: u16,
        keycode: u16,
        is_undo: bool,
    ) {
        if let Some(layout) = self.layout.as_mut() {
            layout.set_keycode(layer, key_index, keycode);
        }
        if !is_undo {
            self.undo_stack.push(UndoAction::Key {
                layer,
                key_idx: key_index,
                old_kc: old_keycode,
            });
        }
        self.refresh_layer_picker_content_flags();
        self.status_msg = "✓ Saved".into();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    fn finish_encoder_write(
        &mut self,
        layer: usize,
        encoder_visual_index: usize,
        encoder_index: u8,
        direction: u8,
        old_keycode: u16,
        keycode: u16,
        is_undo: bool,
    ) {
        if let Some(layout) = self.layout.as_mut() {
            layout.set_encoder_keycode(layer, encoder_visual_index, keycode);
        }
        if !is_undo {
            self.undo_stack.push(UndoAction::Encoder {
                layer,
                encoder_visual_idx: encoder_visual_index,
                old_kc: old_keycode,
            });
        }
        self.status_msg = format!(
            "Assigned encoder {encoder_index} direction {direction} on layer {}",
            layer + 1
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn rollback_single_write(&mut self, operation: &VialHidOperation) {
        match operation {
            VialHidOperation::KeyWrite {
                layer,
                key_index,
                old_keycode,
                keycode,
                is_undo,
                ..
            } => {
                if let Some(layout) = self.layout.as_mut() {
                    if layout.get_keycode(*layer, *key_index) == *keycode {
                        layout.set_keycode(*layer, *key_index, *old_keycode);
                    }
                }
                if *is_undo {
                    self.undo_stack.push(UndoAction::Key {
                        layer: *layer,
                        key_idx: *key_index,
                        old_kc: *keycode,
                    });
                }
                self.refresh_layer_picker_content_flags();
            }
            VialHidOperation::EncoderWrite {
                layer,
                encoder_visual_index,
                old_keycode,
                keycode,
                is_undo,
                ..
            } => {
                if let Some(layout) = self.layout.as_mut() {
                    if layout.get_encoder_keycode(*layer, *encoder_visual_index) == *keycode {
                        layout.set_encoder_keycode(*layer, *encoder_visual_index, *old_keycode);
                    }
                }
                if *is_undo {
                    self.undo_stack.push(UndoAction::Encoder {
                        layer: *layer,
                        encoder_visual_idx: *encoder_visual_index,
                        old_kc: *keycode,
                    });
                }
            }
            _ => {}
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_vial_lock(&mut self) {
        self.vial_unlocked = Some(false);
        self.matrix_tester_pressed.clear();
        self.matrix_tester_unlock_prompted = false;
        self.matrix_tester_lock_checked = false;
        if self.app_settings.sticky_layout_window {
            self.app_settings.sticky_layout_window = false;
            self.pending_layout_indicator_open_after_unlock = false;
            self.sticky_layout_last_size = None;
            save_app_settings(&self.app_settings);
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "ui.sticky_layout_closed_due_to_lock",
            )
            .into();
        } else {
            self.status_msg =
                crate::i18n::tr_catalog(self.app_settings.language, "dynamic_status.device_locked")
                    .into();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_vial_hid_error(
        &mut self,
        operation: VialHidOperation,
        error: String,
        disconnected: bool,
    ) {
        if matches!(
            &operation,
            VialHidOperation::KeyWrite { .. } | VialHidOperation::EncoderWrite { .. }
        ) {
            self.rollback_single_write(&operation);
            self.status_msg = match operation {
                VialHidOperation::EncoderWrite { .. } => {
                    format!("Set encoder failed: {error}")
                }
                _ => format!("Write error: {error}"),
            };
            if disconnected && !self.begin_bluetooth_reconnect(error.clone()) {
                self.clear_connected_keyboard_state(error);
            }
            return;
        }

        if disconnected {
            if !self.begin_bluetooth_reconnect(error.clone()) {
                self.clear_connected_keyboard_state(error);
            }
            return;
        }

        match operation {
            VialHidOperation::UnlockStart => self.fail_vial_unlock_start(error),
            VialHidOperation::UnlockPoll => self.fail_vial_unlock_poll(error),
            VialHidOperation::Lock => {
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "dynamic_status.lock_failed",
                    &[("error", &error)],
                );
            }
            VialHidOperation::Matrix { .. } => self.fail_matrix_tester_poll(error),
            VialHidOperation::BatteryRefresh => {
                log::warn!("Battery refresh failed: {error}");
                self.next_battery_refresh_at =
                    Some(std::time::Instant::now() + BATTERY_REFRESH_RETRY_INTERVAL);
            }
            VialHidOperation::KeyWrite { .. } | VialHidOperation::EncoderWrite { .. } => {
                unreachable!("single writes are handled before disconnect processing")
            }
            VialHidOperation::Deferred(request) => {
                log::warn!("Deferred Bluetooth device load failed: {error}");
                if request.is_background_layer() {
                    self.deferred_device_load.mark_background_layer_finished();
                }
                self.fail_deferred_device_load(&request, error);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn deferred_vial_hid_task_blocks_keyboard(&self) -> bool {
        self.vial_hid_task
            .as_ref()
            .map(|task| match &task.operation {
                VialHidOperation::Deferred(request) => request.blocks_keyboard(),
                _ => false,
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_key_layout(keycode: u16) -> KeyboardLayout {
        KeyboardLayout {
            name: "Async key write".into(),
            rows: 1,
            cols: 1,
            keys: vec![PhysicalKey {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
                row: 0,
                col: 0,
                label: String::new(),
                rotation: 0.0,
                rotation_x: 0.0,
                rotation_y: 0.0,
                layout_condition: None,
            }],
            encoders: vec![],
            layers: vec![vec![keycode]],
            encoder_layers: vec![vec![]],
            layer_names: vec!["Layer 0".into()],
            custom_keycodes: vec![],
            layout_options: vec![],
            live_features: Default::default(),
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
        }
    }

    fn background_layer_context() -> std::sync::Arc<DeferredDeviceLoadContext> {
        std::sync::Arc::new(DeferredDeviceLoadContext {
            json: std::sync::Arc::new(serde_json::json!({})),
            supported_qmk_settings: std::sync::Arc::new(Vec::new()),
            definition_fingerprint: 1,
            layer_count: 2,
            rows: 1,
            cols: 15,
            encoder_count: 0,
            macro_count: 0,
            macro_memory_bytes: None,
            tap_dance_count: 0,
            combo_count: 0,
            key_override_count: 0,
            alt_repeat_count: 0,
            modules_supported: false,
            touchpad_supported: false,
            bluetooth_supported: false,
            layer_leds_supported: false,
            lighting_mode: None,
        })
    }

    fn poll_until_vial_hid_idle(app: &mut EntropyApp, ctx: &egui::Context) {
        for _ in 0..100 {
            app.poll_vial_hid_task(ctx);
            if !app.vial_hid_task_active() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("Vial HID task did not finish");
    }

    #[test]
    fn unlock_start_uses_status_then_start_commands() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();

        let outcome = run_vial_hid_operation(&hid, VialHidOperation::UnlockStart).unwrap();

        assert!(matches!(
            outcome,
            VialHidOutcome::UnlockStarted {
                unlocked: false,
                ..
            }
        ));
        let requests = recorder.requests();
        assert_eq!(&requests[0][..2], &[0xFE, 0x05]);
        assert_eq!(&requests[1][..2], &[0xFE, 0x06]);
    }

    #[test]
    fn matrix_poll_uses_via_switch_matrix_command() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();

        let outcome = run_vial_hid_operation(
            &hid,
            VialHidOperation::Matrix {
                rows: 2,
                cols: 3,
                rmk_byte_order: true,
                remember_ever_pressed: true,
            },
        )
        .unwrap();

        assert!(matches!(
            outcome,
            VialHidOutcome::Matrix(pressed) if pressed == vec![false; 6]
        ));
        let requests = recorder.requests();
        assert_eq!(&requests[0][..2], &[0x02, 0x03]);
    }

    #[test]
    fn unlock_poll_and_lock_use_vial_commands() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();

        let _ = run_vial_hid_operation(&hid, VialHidOperation::UnlockPoll).unwrap();
        let _ = run_vial_hid_operation(&hid, VialHidOperation::Lock).unwrap();

        let requests = recorder.requests();
        assert_eq!(&requests[0][..2], &[0xFE, 0x07]);
        assert_eq!(&requests[1][..2], &[0xFE, 0x08]);
    }

    #[test]
    fn battery_refresh_uses_the_existing_vial_transport() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();

        let outcome = run_vial_hid_operation(&hid, VialHidOperation::BatteryRefresh).unwrap();

        assert!(matches!(outcome, VialHidOutcome::Battery(_)));
        let requests = recorder.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(&requests[0][..3], &[0x08, 0xE8, 0x01]);
        assert_eq!(&requests[1][..3], &[0x08, 0xE8, 0x01]);
    }

    #[test]
    fn key_and_encoder_writes_use_the_serialized_vial_transport() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();

        let key_outcome = run_vial_hid_operation(
            &hid,
            VialHidOperation::KeyWrite {
                layer: 0,
                key_index: 0,
                row: 2,
                col: 3,
                old_keycode: 0x0004,
                keycode: 0,
                is_undo: false,
            },
        )
        .unwrap();
        let encoder_outcome = run_vial_hid_operation(
            &hid,
            VialHidOperation::EncoderWrite {
                layer: 1,
                encoder_visual_index: 0,
                encoder_index: 2,
                direction: 1,
                old_keycode: 0,
                keycode: 0x0005,
                is_undo: false,
            },
        )
        .unwrap();

        assert!(matches!(key_outcome, VialHidOutcome::KeyWritten));
        assert!(matches!(encoder_outcome, VialHidOutcome::EncoderWritten));
        let requests = recorder.requests();
        assert_eq!(&requests[0][..6], &[0x05, 0, 2, 3, 0, 0]);
        assert_eq!(&requests[1][..4], &[0x04, 0, 2, 3]);
        assert_eq!(&requests[2][..7], &[0xFE, 0x04, 1, 2, 1, 0, 5]);
    }

    #[test]
    fn assigning_a_key_updates_ui_before_the_hid_round_trip_finishes() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid);
        app.layout = Some(single_key_layout(0x0004));

        assert!(app.assign_keycode(&ctx, 0, 0, 0));
        assert!(app.vial_hid_task_active());
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 0);
        assert!(app.undo_stack.is_empty());

        poll_until_vial_hid_idle(&mut app, &ctx);

        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 0);
        assert!(matches!(
            app.undo_stack.last(),
            Some(UndoAction::Key {
                layer: 0,
                key_idx: 0,
                old_kc: 0x0004,
            })
        ));
        assert_eq!(recorder.requests().len(), 2);
    }

    #[test]
    fn background_layer_does_not_disable_user_actions_and_queued_undo_runs_next() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let context = background_layer_context();
        let mut layout = single_key_layout(0x0004);
        layout.layers.push(vec![0]);
        layout.layer_names.push("Layer 1".into());
        layout.encoder_layers.push(Vec::new());
        app.layout = Some(layout);
        app.deferred_device_load = DeferredDeviceLoadState::staged((*context).clone());
        app.undo_stack.push(UndoAction::Key {
            layer: 0,
            key_idx: 0,
            old_kc: 0,
        });
        app.hid_device = Some(hid);

        assert_eq!(
            app.start_vial_hid_operation(
                &ctx,
                VialHidOperation::Deferred(
                    super::device_deferred_load::DeferredLoadRequest::BackgroundLayerStep {
                        layer: 1,
                        step: BackgroundLayerStep::Keymap { local_offset: 0 },
                        context,
                    },
                ),
            ),
            VialHidTaskStart::Started
        );
        assert!(app.vial_hid_background_layer_active());
        assert!(!app.hid_user_action_busy());

        app.undo(&ctx);

        assert!(app.pending_layout_undo);
        assert_eq!(app.undo_stack.len(), 1);

        for _ in 0..100 {
            app.poll_vial_hid_task(&ctx);
            app.maybe_start_pending_layout_undo(&ctx);
            if !app.vial_hid_task_active() && !app.pending_layout_undo {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        assert!(!app.pending_layout_undo);
        assert!(!app.vial_hid_task_active());
        assert!(app.undo_stack.is_empty());
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 0);
        assert_eq!(
            app.deferred_device_load.layer_status(1),
            DeferredLoadStatus::NotLoaded
        );
        let requests = recorder.requests();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0][0], 0x12);
        assert_eq!(requests[1][0], 0x05);
        assert_eq!(requests[2][0], 0x04);
    }

    #[test]
    fn failed_background_key_write_rolls_back_the_optimistic_value() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, _recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid);
        app.layout = Some(single_key_layout(0x0004));

        assert!(app.assign_keycode(&ctx, 0, 0, 0x0005));
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 0x0005);

        poll_until_vial_hid_idle(&mut app, &ctx);

        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 0x0004);
        assert!(app.undo_stack.is_empty());
        assert!(app.status_msg.starts_with("Write error:"));
        assert!(app.hid_device.is_some());
    }
}
