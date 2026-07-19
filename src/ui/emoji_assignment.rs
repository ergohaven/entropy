use super::*;

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
            let (clockwise, counter_clockwise) = hid_device.get_encoder(*layer as u8, *encoder_idx)?;
            let actual = if *direction == 0 { clockwise } else { counter_clockwise };
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
            self.keycode_picker.emoji_assignment_error = true;
            self.keycode_picker.result = None;
            self.keycode_picker.open = true;
            self.status_msg = "Emoji assignment needs a connected device".into();
            return;
        };

        let previous_macros = self.keycode_picker.macro_texts.clone();
        let mut desired_macros = previous_macros.clone();
        let Some(slot) = desired_macros.get_mut(assignment.slot) else {
            self.hid_device = Some(hid_device);
            self.keycode_picker.emoji_assignment = Some(assignment);
            self.keycode_picker.emoji_assignment_error = true;
            self.keycode_picker.result = None;
            self.keycode_picker.open = true;
            self.status_msg = "Emoji assignment selected an unavailable macro slot".into();
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
        });
        self.status_msg = "Saving emoji assignment…".into();
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }

    pub(super) fn poll_emoji_assignment(&mut self, ctx: &egui::Context) {
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
                self.hid_device = result.hid_device;
                match result.result {
                    Ok(()) => self.finish_emoji_assignment(task),
                    Err(error) => self.restore_emoji_assignment(task, error),
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
                self.hid_device = None;
                self.restore_emoji_assignment(task, "emoji assignment worker stopped".into());
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
        self.keycode_picker.emoji_assignment_error = false;
        self.keycode_picker.result = None;
        self.keycode_picker.open = false;
        self.selected_key = None;
        self.selected_encoder = None;
        self.status_msg = "Emoji assignment saved".into();
    }

    fn restore_emoji_assignment(&mut self, task: EmojiAssignmentTask, error: String) {
        self.keycode_picker.emoji_assignment = Some(task.assignment);
        self.keycode_picker.emoji_assignment_error = true;
        self.keycode_picker.result = None;
        self.keycode_picker.open = true;
        self.status_msg = format!("Emoji assignment was not saved: {error}");
    }
}
