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
            Ok(VialHidOutcome::Deferred(payload)) => {
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
            VialHidOperation::Deferred(request) => {
                log::warn!("Deferred Bluetooth device load failed: {error}");
                self.fail_deferred_device_load(&request, error);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn deferred_vial_hid_task_active(&self) -> bool {
        self.vial_hid_task
            .as_ref()
            .map(|task| matches!(&task.operation, VialHidOperation::Deferred(_)))
            .unwrap_or(false)
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
}
