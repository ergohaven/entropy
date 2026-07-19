use super::*;

const EMOJI_ASSIGNMENT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Clone, Debug)]
pub(super) enum EmojiAssignmentTarget {
    Key {
        layer: usize,
        key_idx: usize,
        row: u8,
        col: u8,
        old_keycode: u16,
    },
    Encoder {
        layer: usize,
        encoder_visual_idx: usize,
        encoder_idx: u8,
        direction: u8,
        old_keycode: u16,
    },
}

pub(super) struct EmojiAssignmentTask {
    receiver: std::sync::mpsc::Receiver<EmojiAssignmentResult>,
    target: EmojiAssignmentTarget,
    assignment: crate::keycode_picker::EmojiAssignment,
    connection_generation: u64,
    started_at: std::time::Instant,
}

pub(super) struct EmojiAssignmentReclaimTask {
    receiver: std::sync::mpsc::Receiver<EmojiAssignmentResult>,
}

impl EmojiAssignmentTask {
    pub(super) fn into_reclaim(self) -> EmojiAssignmentReclaimTask {
        EmojiAssignmentReclaimTask {
            receiver: self.receiver,
        }
    }
}

struct EmojiAssignmentResult {
    hid_device: Option<crate::hid::HidDevice>,
    result: Result<(), String>,
}

fn macro_buffer_matches(
    hid_device: &crate::hid::HidDevice,
    macros: &[Vec<u8>],
    buffer_size: u16,
) -> anyhow::Result<()> {
    let expected = crate::hid::HidDevice::encode_macros(macros, buffer_size);
    hid_device.set_macro_buffer(&expected)?;
    let macro_count = u8::try_from(macros.len()).map_err(|_| anyhow::anyhow!("too many macros"))?;
    let actual = crate::hid::HidDevice::parse_macros(
        &hid_device.get_macro_buffer(buffer_size, macro_count)?,
        macro_count,
    );
    if actual != macros {
        anyhow::bail!("macro buffer readback did not match requested assignment");
    }
    Ok(())
}

fn write_target(
    hid_device: &crate::hid::HidDevice,
    target: &EmojiAssignmentTarget,
    keycode: u16,
) -> anyhow::Result<()> {
    match target {
        EmojiAssignmentTarget::Key {
            layer, row, col, ..
        } => hid_device.set_keycode(*layer as u8, *row, *col, keycode),
        EmojiAssignmentTarget::Encoder {
            layer,
            encoder_idx,
            direction,
            ..
        } => {
            hid_device.set_encoder(*layer as u8, *encoder_idx, *direction, keycode)?;
            let (clockwise, counter_clockwise) =
                hid_device.get_encoder(*layer as u8, *encoder_idx)?;
            let actual = if *direction == 0 {
                clockwise
            } else {
                counter_clockwise
            };
            if actual != keycode {
                anyhow::bail!("encoder readback did not match requested assignment");
            }
            Ok(())
        }
    }
}

fn target_old_keycode(target: &EmojiAssignmentTarget) -> u16 {
    match target {
        EmojiAssignmentTarget::Key { old_keycode, .. }
        | EmojiAssignmentTarget::Encoder { old_keycode, .. } => *old_keycode,
    }
}

fn rollback_assignment(
    hid_device: &crate::hid::HidDevice,
    target: &EmojiAssignmentTarget,
    macros: &[Vec<u8>],
    buffer_size: u16,
) -> String {
    let target_result = write_target(hid_device, target, target_old_keycode(target));
    let macros_result = macro_buffer_matches(hid_device, macros, buffer_size);
    match (target_result, macros_result) {
        (Ok(()), Ok(())) => "rollback completed".into(),
        (target, macros) => format!("rollback failed (target: {target:?}, macros: {macros:?})"),
    }
}

fn write_emoji_assignment(
    hid_device: &crate::hid::HidDevice,
    target: &EmojiAssignmentTarget,
    previous_macros: &[Vec<u8>],
    desired_macros: &[Vec<u8>],
    keycode: u16,
) -> anyhow::Result<()> {
    let buffer_size = hid_device.get_macro_buffer_size()?;
    if let Err(error) = macro_buffer_matches(hid_device, desired_macros, buffer_size) {
        let rollback = rollback_assignment(hid_device, target, previous_macros, buffer_size);
        anyhow::bail!("macro-buffer write failed: {error}; {rollback}");
    }
    if let Err(error) = write_target(hid_device, target, keycode) {
        let rollback = rollback_assignment(hid_device, target, previous_macros, buffer_size);
        anyhow::bail!("target write failed: {error}; {rollback}");
    }
    Ok(())
}

impl EntropyApp {
    pub(super) fn start_emoji_assignment(
        &mut self,
        ctx: &egui::Context,
        target: EmojiAssignmentTarget,
        assignment: crate::keycode_picker::EmojiAssignment,
    ) {
        // A failed write remains staged, but retry must be an explicit picker
        // choice rather than an automatic frame-by-frame resend.
        self.keycode_picker.result = None;
        let Some(hid_device) = self.hid_device.take() else {
            self.keycode_picker.emoji_assignment = Some(assignment);
            self.keycode_picker.emoji_assignment_error =
                Some(crate::keycode_picker::EmojiAssignmentError::DeviceUnavailable);
            self.keycode_picker.result = None;
            self.keycode_picker.open = true;
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "status_messages.emoji_assignment_device_unavailable",
            )
            .into();
            return;
        };

        let previous_macros = self.keycode_picker.macro_texts.clone();
        let mut desired_macros = previous_macros.clone();
        let Some(slot) = desired_macros.get_mut(assignment.slot) else {
            self.hid_device = Some(hid_device);
            self.keycode_picker.emoji_assignment = Some(assignment);
            self.keycode_picker.emoji_assignment_error =
                Some(crate::keycode_picker::EmojiAssignmentError::TargetUnavailable);
            self.keycode_picker.result = None;
            self.keycode_picker.open = true;
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "status_messages.emoji_assignment_target_unavailable",
            )
            .into();
            return;
        };
        *slot = assignment.text.clone();
        let keycode = 0x7700u16.saturating_add(assignment.slot as u16);
        let (sender, receiver) = std::sync::mpsc::channel();
        let task_target = target.clone();
        std::thread::spawn(move || {
            #[cfg(target_os = "macos")]
            let _hid_lock = crate::hid::macos_hid_operation_lock();

            let write_result = write_emoji_assignment(
                &hid_device,
                &task_target,
                &previous_macros,
                &desired_macros,
                keycode,
            );
            let disconnected = write_result
                .as_ref()
                .err()
                .map(crate::hid::is_disconnect_error)
                .unwrap_or(false);
            let result = write_result.map_err(|error| error.to_string());
            let hid_device = (!disconnected).then_some(hid_device);
            let _ = sender.send(EmojiAssignmentResult { hid_device, result });
        });
        self.emoji_assignment_task = Some(EmojiAssignmentTask {
            receiver,
            target,
            assignment,
            connection_generation: self.hid_connection_generation,
            started_at: std::time::Instant::now(),
        });
        self.status_msg = crate::i18n::tr_catalog(
            self.app_settings.language,
            "status_messages.emoji_assignment_saving",
        )
        .into();
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }

    pub(super) fn poll_emoji_assignment(&mut self, ctx: &egui::Context) {
        self.poll_abandoned_emoji_assignment(ctx);
        if self
            .emoji_assignment_task
            .as_ref()
            .is_some_and(|task| task.started_at.elapsed() >= EMOJI_ASSIGNMENT_TIMEOUT)
        {
            let task = self
                .emoji_assignment_task
                .take()
                .expect("emoji task checked above");
            let assignment = task.assignment.clone();
            self.emoji_assignment_reclaim_task = Some(task.into_reclaim());
            // The worker still owns the HID handle. Invalidate this connection
            // before allowing another scan so a late write cannot leave a live
            // layout paired with no handle.
            self.handoff_hid_worker_disconnect(crate::i18n::tr_catalog(
                self.app_settings.language,
                "key_picker.emoji_worker_stopped",
            ));
            self.restore_emoji_assignment(
                assignment,
                crate::keycode_picker::EmojiAssignmentError::WorkerStopped,
                None,
            );
            return;
        }
        let result = match self.emoji_assignment_task.as_ref() {
            Some(task) => task.receiver.try_recv(),
            None => return,
        };
        match result {
            Ok(result) => {
                let task = self
                    .emoji_assignment_task
                    .take()
                    .expect("emoji task checked above");
                if task.connection_generation != self.hid_connection_generation {
                    return;
                }
                match result.hid_device {
                    Some(hid_device) => {
                        self.hid_device = Some(hid_device);
                        match result.result {
                            Ok(()) => self.finish_emoji_assignment(task),
                            Err(error) => self.restore_emoji_assignment(
                                task.assignment,
                                crate::keycode_picker::EmojiAssignmentError::SaveFailed,
                                Some(error),
                            ),
                        }
                    }
                    None => {
                        let error = result.result.err().unwrap_or_else(|| {
                            "emoji assignment worker returned no HID handle".into()
                        });
                        self.handoff_hid_worker_disconnect(crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "key_picker.emoji_save_failed",
                        ));
                        self.restore_emoji_assignment(
                            task.assignment,
                            crate::keycode_picker::EmojiAssignmentError::SaveFailed,
                            Some(error),
                        );
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let task = self
                    .emoji_assignment_task
                    .take()
                    .expect("emoji task checked above");
                self.handoff_hid_worker_disconnect(crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "key_picker.emoji_worker_stopped",
                ));
                self.restore_emoji_assignment(
                    task.assignment,
                    crate::keycode_picker::EmojiAssignmentError::WorkerStopped,
                    None,
                );
            }
        }
    }

    pub(super) fn abandon_emoji_assignment(&mut self) {
        let Some(task) = self.emoji_assignment_task.take() else {
            return;
        };
        self.hid_connection_generation = self.hid_connection_generation.wrapping_add(1);
        let assignment = task.assignment.clone();
        self.emoji_assignment_reclaim_task = Some(task.into_reclaim());
        self.restore_emoji_assignment(
            assignment,
            crate::keycode_picker::EmojiAssignmentError::WorkerStopped,
            None,
        );
    }

    fn poll_abandoned_emoji_assignment(&mut self, ctx: &egui::Context) {
        let result = match self.emoji_assignment_reclaim_task.as_ref() {
            Some(task) => task.receiver.try_recv(),
            None => return,
        };
        match result {
            Ok(_) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // The session was invalidated before this worker completed. Drop
                // its returned HID handle without restoring UI or firmware state.
                self.emoji_assignment_reclaim_task = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
    }

    fn finish_emoji_assignment(&mut self, task: EmojiAssignmentTask) {
        let assignment = task.assignment;
        self.keycode_picker.macro_actions[assignment.slot] = assignment.actions;
        self.keycode_picker.macro_texts[assignment.slot] = assignment.text;
        self.keycode_picker.macros_dirty = false;
        let keycode = 0x7700u16.saturating_add(assignment.slot as u16);
        match task.target {
            EmojiAssignmentTarget::Key {
                layer,
                key_idx,
                old_keycode,
                ..
            } => {
                self.undo_stack.push(UndoAction::Key {
                    layer,
                    key_idx,
                    old_kc: old_keycode,
                });
                if let Some(layout) = &mut self.layout {
                    layout.set_keycode(layer, key_idx, keycode);
                }
                self.refresh_layer_picker_content_flags();
            }
            EmojiAssignmentTarget::Encoder {
                layer,
                encoder_visual_idx,
                old_keycode,
                ..
            } => {
                self.undo_stack.push(UndoAction::Encoder {
                    layer,
                    encoder_visual_idx,
                    old_kc: old_keycode,
                });
                if let Some(layout) = &mut self.layout {
                    layout.set_encoder_keycode(layer, encoder_visual_idx, keycode);
                }
            }
        }
        self.keycode_picker.emoji_assignment = None;
        self.keycode_picker.emoji_assignment_error = None;
        self.keycode_picker.result = None;
        self.keycode_picker.open = false;
        self.selected_key = None;
        self.selected_encoder = None;
        self.status_msg = crate::i18n::tr_catalog(
            self.app_settings.language,
            "status_messages.emoji_assignment_saved",
        )
        .into();
    }

    fn restore_emoji_assignment(
        &mut self,
        assignment: crate::keycode_picker::EmojiAssignment,
        reason: crate::keycode_picker::EmojiAssignmentError,
        error: Option<String>,
    ) {
        self.keycode_picker.emoji_assignment = Some(assignment);
        self.keycode_picker.emoji_assignment_error = Some(reason);
        self.keycode_picker.result = None;
        self.keycode_picker.open = true;
        if let Some(error) = error {
            log::warn!("emoji assignment failed: {error}");
        }
        self.status_msg = crate::i18n::tr_catalog(
            self.app_settings.language,
            emoji_assignment_error_key(reason),
        )
        .into();
    }
}

fn emoji_assignment_error_key(error: crate::keycode_picker::EmojiAssignmentError) -> &'static str {
    use crate::keycode_picker::EmojiAssignmentError;

    match error {
        EmojiAssignmentError::BackendUnavailable => "key_picker.emoji_backend_unavailable",
        EmojiAssignmentError::NoFreeMacroSlot => "key_picker.emoji_no_free_macro_slot",
        EmojiAssignmentError::DeviceUnavailable => "key_picker.emoji_device_unavailable",
        EmojiAssignmentError::TargetUnavailable => "key_picker.emoji_target_unavailable",
        EmojiAssignmentError::WorkerStopped => "key_picker.emoji_worker_stopped",
        EmojiAssignmentError::SaveFailed => "key_picker.emoji_save_failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assignment() -> crate::keycode_picker::EmojiAssignment {
        crate::keycode_picker::EmojiAssignment {
            slot: 0,
            actions: vec![],
            text: vec![],
        }
    }

    fn target() -> EmojiAssignmentTarget {
        EmojiAssignmentTarget::Key {
            layer: 0,
            key_idx: 0,
            row: 0,
            col: 0,
            old_keycode: 0,
        }
    }

    fn app(ctx: &egui::Context) -> EntropyApp {
        EntropyApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()))
    }

    fn macros_with_emoji() -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let previous = vec![Vec::new(); 16];
        let mut desired = previous.clone();
        desired[0] = vec![0x01, 0x02];
        (previous, desired)
    }

    fn request_count(recorder: &crate::hid::TestHidRecorder, command: u8) -> usize {
        recorder
            .requests()
            .into_iter()
            .filter(|request| request[0] == command)
            .count()
    }

    #[test]
    fn macro_write_failure_rolls_back_without_committing_target() {
        // First request reads the macro-buffer size; the first macro write then
        // disconnects. The rollback must still restore the old target and buffer.
        let (hid_device, recorder) =
            crate::hid::HidDevice::test_device_with_disconnect_after_requests(Some(1));
        let (previous, desired) = macros_with_emoji();

        let error = write_emoji_assignment(&hid_device, &target(), &previous, &desired, 0x7700)
            .expect_err("macro write must fail");

        assert!(error.to_string().contains("macro-buffer write failed"));
        assert_eq!(
            request_count(&recorder, 0x0F),
            2,
            "failed write plus rollback"
        );
        assert_eq!(request_count(&recorder, 0x05), 1, "rollback target write");
        assert_eq!(hid_device.test_macro_entries(16), previous);
        assert_eq!(hid_device.test_keycode(0, 0, 0), 0);
    }

    #[test]
    fn partial_macro_write_failure_rolls_back_full_buffer() {
        // A 56-byte buffer forces two macro-write requests. Disconnect on the
        // second chunk and verify rollback rewrites both old chunks.
        let (hid_device, recorder) =
            crate::hid::HidDevice::test_device_with_macro_buffer_size_and_disconnect_after_requests(
                56,
                Some(2),
            );
        let mut previous = vec![Vec::new(); 16];
        previous[0] = vec![0x01; 40];
        let mut desired = previous.clone();
        desired[0] = vec![0x02; 40];

        let error = write_emoji_assignment(&hid_device, &target(), &previous, &desired, 0x7700)
            .expect_err("second macro chunk must fail");

        assert!(error.to_string().contains("macro-buffer write failed"));
        assert_eq!(
            request_count(&recorder, 0x0F),
            4,
            "two attempted chunks and two rollback chunks"
        );
        assert_eq!(hid_device.test_macro_entries(16), previous);
        assert_eq!(hid_device.test_keycode(0, 0, 0), 0);
    }

    #[test]
    fn target_write_failure_restores_previous_macro_buffer() {
        // Macro size, write and readback succeed; target write disconnects.
        // Rollback must restore both target and the original macro buffer.
        let (hid_device, recorder) =
            crate::hid::HidDevice::test_device_with_disconnect_after_requests(Some(3));
        let (previous, desired) = macros_with_emoji();

        let error = write_emoji_assignment(&hid_device, &target(), &previous, &desired, 0x7700)
            .expect_err("target write must fail");

        assert!(error.to_string().contains("target write failed"));
        assert_eq!(
            request_count(&recorder, 0x0F),
            2,
            "save then rollback macro buffer"
        );
        assert_eq!(
            request_count(&recorder, 0x05),
            2,
            "failed target write then rollback"
        );
        assert_eq!(hid_device.test_macro_entries(16), previous);
        assert_eq!(hid_device.test_keycode(0, 0, 0), 0);
    }

    #[test]
    fn macro_readback_mismatch_rolls_back_original_values() {
        let (hid_device, _recorder) = crate::hid::HidDevice::test_device();
        hid_device.test_mismatch_next_readback(crate::hid::TestHidReadbackMismatch::MacroBuffer);
        let (previous, desired) = macros_with_emoji();

        let error = write_emoji_assignment(&hid_device, &target(), &previous, &desired, 0x7700)
            .expect_err("macro readback mismatch must fail");

        assert!(error
            .to_string()
            .contains("macro buffer readback did not match"));
        assert_eq!(hid_device.test_macro_entries(16), previous);
        assert_eq!(hid_device.test_keycode(0, 0, 0), 0);
    }

    #[test]
    fn target_readback_mismatch_rolls_back_original_values() {
        let (hid_device, _recorder) = crate::hid::HidDevice::test_device();
        hid_device.test_mismatch_next_readback(crate::hid::TestHidReadbackMismatch::Keycode);
        let (previous, desired) = macros_with_emoji();

        let error = write_emoji_assignment(&hid_device, &target(), &previous, &desired, 0x7700)
            .expect_err("target readback mismatch must fail");

        assert!(error.to_string().contains("keycode writeback mismatch"));
        assert_eq!(hid_device.test_macro_entries(16), previous);
        assert_eq!(hid_device.test_keycode(0, 0, 0), 0);
    }

    #[test]
    fn persistent_disconnect_reports_rollback_failure_without_claiming_restore() {
        let (hid_device, _recorder) =
            crate::hid::HidDevice::test_device_with_persistent_disconnect_after_requests(2);
        let mut previous = vec![Vec::new(); 16];
        previous[0] = vec![0x01; 40];
        let mut desired = previous.clone();
        desired[0] = vec![0x02; 40];

        let error = write_emoji_assignment(&hid_device, &target(), &previous, &desired, 0x7700)
            .expect_err("persistent disconnect must fail the transaction");

        assert!(error.to_string().contains("rollback failed"));
        assert_ne!(hid_device.test_macro_entries(16), previous);
    }

    #[test]
    fn successful_assignment_writes_each_transaction_step_once() {
        let (hid_device, recorder) = crate::hid::HidDevice::test_device();
        let (previous, desired) = macros_with_emoji();

        write_emoji_assignment(&hid_device, &target(), &previous, &desired, 0x7700)
            .expect("transaction must succeed");

        assert_eq!(request_count(&recorder, 0x0F), 1);
        assert_eq!(request_count(&recorder, 0x0E), 1);
        assert_eq!(request_count(&recorder, 0x05), 1);
        assert_eq!(request_count(&recorder, 0x04), 1);
    }

    #[test]
    fn stale_emoji_completion_cannot_restore_hid_or_commit_picker_state() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        let (sender, receiver) = std::sync::mpsc::channel();
        sender
            .send(EmojiAssignmentResult {
                hid_device: None,
                result: Ok(()),
            })
            .expect("receiver is live");
        app.emoji_assignment_task = Some(EmojiAssignmentTask {
            receiver,
            target: target(),
            assignment: assignment(),
            connection_generation: 0,
            started_at: std::time::Instant::now(),
        });
        app.hid_connection_generation = 1;

        app.poll_emoji_assignment(&ctx);

        assert!(app.emoji_assignment_task.is_none());
        assert!(app.hid_device.is_none());
        assert!(app.keycode_picker.emoji_assignment.is_none());
        assert_eq!(app.keycode_picker.macro_texts[0], Vec::<u8>::new());
    }

    #[test]
    fn disconnect_clears_emoji_worker_and_invalidates_its_generation() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        let (_sender, receiver) = std::sync::mpsc::channel();
        let generation = app.hid_connection_generation;
        app.emoji_assignment_task = Some(EmojiAssignmentTask {
            receiver,
            target: target(),
            assignment: assignment(),
            connection_generation: generation,
            started_at: std::time::Instant::now(),
        });

        app.clear_connected_keyboard_state("Device disconnected");

        assert!(app.emoji_assignment_task.is_none());
        assert_eq!(app.hid_connection_generation, generation.wrapping_add(1));
        assert!(app.emoji_assignment_reclaim_task.is_some());
        assert!(app.hid_write_task_active());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn emoji_worker_holds_the_settings_write_transport() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        let (_sender, receiver) = std::sync::mpsc::channel();
        app.emoji_assignment_task = Some(EmojiAssignmentTask {
            receiver,
            target: target(),
            assignment: assignment(),
            connection_generation: app.hid_connection_generation,
            started_at: std::time::Instant::now(),
        });

        assert!(app.hid_write_task_owner_active());
    }

    #[test]
    fn hung_emoji_worker_stops_blocking_hid_lifecycle() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        let (sender, receiver) = std::sync::mpsc::channel();
        let generation = app.hid_connection_generation;
        app.current_device_name = "Test Keyboard".into();
        app.emoji_assignment_task = Some(EmojiAssignmentTask {
            receiver,
            target: target(),
            assignment: assignment(),
            connection_generation: app.hid_connection_generation,
            started_at: std::time::Instant::now() - EMOJI_ASSIGNMENT_TIMEOUT,
        });

        app.poll_emoji_assignment(&ctx);

        assert!(app.emoji_assignment_task.is_none());
        assert!(app.emoji_assignment_reclaim_task.is_some());
        assert!(app.hid_write_task_active());
        assert_eq!(app.hid_connection_generation, generation.wrapping_add(1));
        assert!(app.current_device_name.is_empty());
        assert!(app.hid_device.is_none());
        app.device_manager.replace_devices(vec![crate::device::Device {
            name: "Reconnect target".to_owned(),
            vendor_id: 0,
            product_id: 0,
            manufacturer: String::new(),
            serial_number: String::new(),
            bus_type: String::new(),
            path: "test:reconnect".to_owned(),
            firmware: crate::firmware::FirmwareProtocol::Vial,
        }]);
        app.start_connect(0);
        assert!(matches!(app.connect_state, ConnectState::Idle));
        assert!(sender
            .send(EmojiAssignmentResult {
                hid_device: None,
                result: Ok(()),
            })
            .is_ok());
        app.poll_emoji_assignment(&ctx);
        assert!(!app.hid_write_task_active());
        app.start_connect(0);
        assert!(matches!(app.connect_state, ConnectState::Loading { .. }));
        assert_eq!(
            app.keycode_picker.emoji_assignment_error,
            Some(crate::keycode_picker::EmojiAssignmentError::WorkerStopped)
        );
    }

    #[test]
    fn close_abandons_hung_emoji_worker_without_deferring_exit() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        let (_sender, receiver) = std::sync::mpsc::channel();
        app.emoji_assignment_task = Some(EmojiAssignmentTask {
            receiver,
            target: target(),
            assignment: assignment(),
            connection_generation: app.hid_connection_generation,
            started_at: std::time::Instant::now(),
        });

        assert!(!app.defer_exit_until_hid_write_returns(&ctx));
        assert!(app.emoji_assignment_task.is_none());
        assert!(app.emoji_assignment_reclaim_task.is_some());
        assert!(app.hid_write_task_active());
        assert!(!app.exit_after_hid_write);
    }

    #[test]
    fn abandoned_worker_cannot_deliver_a_late_completion() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        let (sender, receiver) = std::sync::mpsc::channel();
        app.emoji_assignment_task = Some(EmojiAssignmentTask {
            receiver,
            target: target(),
            assignment: assignment(),
            connection_generation: app.hid_connection_generation,
            started_at: std::time::Instant::now(),
        });

        app.abandon_emoji_assignment();

        assert!(sender
            .send(EmojiAssignmentResult {
                hid_device: None,
                result: Ok(()),
            })
            .is_ok());
        app.poll_emoji_assignment(&ctx);
        assert!(app.emoji_assignment_task.is_none());
        assert!(app.emoji_assignment_reclaim_task.is_none());
        assert_eq!(app.keycode_picker.macro_texts[0], Vec::<u8>::new());
    }

    #[test]
    fn failed_emoji_completion_preserves_explicit_retry_state() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        let (sender, receiver) = std::sync::mpsc::channel();
        let generation = app.hid_connection_generation;
        app.current_device_name = "Test Keyboard".into();
        sender
            .send(EmojiAssignmentResult {
                hid_device: None,
                result: Err("device disconnected".into()),
            })
            .expect("receiver is live");
        app.emoji_assignment_task = Some(EmojiAssignmentTask {
            receiver,
            target: target(),
            assignment: assignment(),
            connection_generation: app.hid_connection_generation,
            started_at: std::time::Instant::now(),
        });

        app.poll_emoji_assignment(&ctx);

        assert!(app.emoji_assignment_task.is_none());
        assert!(app.keycode_picker.open);
        assert_eq!(app.hid_connection_generation, generation.wrapping_add(1));
        assert!(app.current_device_name.is_empty());
        assert!(app.hid_device.is_none());
        assert_eq!(
            app.keycode_picker
                .emoji_assignment
                .as_ref()
                .map(|assignment| assignment.slot),
            Some(0)
        );
        assert_eq!(
            app.keycode_picker.emoji_assignment_error,
            Some(crate::keycode_picker::EmojiAssignmentError::SaveFailed)
        );
        assert_eq!(
            app.status_msg,
            crate::i18n::tr_catalog(app.app_settings.language, "key_picker.emoji_save_failed")
        );
        assert!(!app.status_msg.contains("device disconnected"));
    }

    #[test]
    fn failed_emoji_completion_with_hid_keeps_connected_session_for_retry() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        let (hid_device, _) = crate::hid::HidDevice::test_device();
        let (sender, receiver) = std::sync::mpsc::channel();
        let generation = app.hid_connection_generation;
        app.current_device_name = "Test Keyboard".into();
        sender
            .send(EmojiAssignmentResult {
                hid_device: Some(hid_device),
                result: Err("write rejected".into()),
            })
            .expect("receiver is live");
        app.emoji_assignment_task = Some(EmojiAssignmentTask {
            receiver,
            target: target(),
            assignment: assignment(),
            connection_generation: generation,
            started_at: std::time::Instant::now(),
        });

        app.poll_emoji_assignment(&ctx);

        assert!(app.emoji_assignment_task.is_none());
        assert!(app.hid_device.is_some());
        assert_eq!(app.hid_connection_generation, generation);
        assert_eq!(app.current_device_name, "Test Keyboard");
        assert!(app.keycode_picker.open);
        assert_eq!(
            app.keycode_picker.emoji_assignment_error,
            Some(crate::keycode_picker::EmojiAssignmentError::SaveFailed)
        );
    }
}
