use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, PartialEq, Eq)]
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
    }
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn vial_hid_task_active(&self) -> bool {
        self.vial_hid_task.is_some()
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
    fn start_vial_hid_operation(
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
        std::thread::spawn(move || {
            #[cfg(target_os = "macos")]
            let _hid_lock = crate::hid::macos_hid_operation_lock();

            let outcome = run_vial_hid_operation(&hid_device, operation);
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
            operation,
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
                let remember_ever_pressed = match result.operation {
                    VialHidOperation::Matrix {
                        remember_ever_pressed,
                        ..
                    } => remember_ever_pressed,
                    _ => false,
                };
                self.finish_matrix_tester_poll(pressed, remember_ever_pressed);
            }
            Err(error) => {
                self.finish_vial_hid_error(result.operation, error, result.disconnected);
            }
        }

        self.continue_pending_settings_writes(ctx);
        self.resume_pending_device_connect();
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
        if disconnected {
            self.clear_connected_keyboard_state(error);
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(outcome, VialHidOutcome::Matrix(vec![false; 6]));
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
}
