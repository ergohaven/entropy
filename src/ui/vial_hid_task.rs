use super::*;

#[cfg(not(target_arch = "wasm32"))]
const BATTERY_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);
#[cfg(not(target_arch = "wasm32"))]
const BATTERY_REFRESH_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
#[cfg(not(target_arch = "wasm32"))]
const BATTERY_INCOMPLETE_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);
#[cfg(not(target_arch = "wasm32"))]
const INITIAL_BATTERY_REFRESH_DELAY: std::time::Duration = std::time::Duration::from_secs(2);

#[cfg(not(target_arch = "wasm32"))]
fn battery_refresh_delay(battery: Option<crate::hid::BatteryHalves>) -> std::time::Duration {
    if battery.is_some_and(crate::hid::BatteryHalves::is_complete) {
        BATTERY_REFRESH_INTERVAL
    } else {
        BATTERY_INCOMPLETE_REFRESH_INTERVAL
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub(super) enum VialHidOperation {
    UnlockStart,
    UnlockPoll,
    Lock,
    Matrix {
        rows: usize,
        cols: usize,
        remember_ever_pressed: bool,
    },
    BatteryRefresh,
    KeyWrite {
        layer: usize,
        key_index: usize,
        row: u8,
        col: u8,
        old_binding: crate::keyboard::KeyBinding,
        binding: crate::keyboard::KeyBinding,
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
    MacroWrite {
        macros: Vec<Vec<u8>>,
        revision: u64,
    },
    BackgroundUpload {
        path: std::path::PathBuf,
        fallback: [u8; 3],
        scale: crate::app::StandbyBackgroundScale,
    },
    BackgroundClear,
    BackgroundSpeed {
        old_percent: u16,
        percent: u16,
    },
    StartupImageUpload {
        path: std::path::PathBuf,
        fallback: [u8; 3],
    },
    StartupImageClear,
    PictogramLoad {
        preserve_editor: bool,
    },
    PictogramUpload {
        library: crate::app::PictogramLibrary,
    },
    PictogramSlotUpload {
        library: crate::app::PictogramLibrary,
        kind: crate::app::PictogramKind,
        slot: usize,
    },
    Deferred(super::device_deferred_load::DeferredLoadRequest),
}

#[cfg(not(target_arch = "wasm32"))]
impl VialHidOperation {
    fn display_diagnostic_label(&self) -> Option<&'static str> {
        match self {
            Self::UnlockStart => Some("unlock-start"),
            Self::UnlockPoll => Some("unlock-poll"),
            Self::Lock => Some("lock"),
            Self::PictogramLoad { .. } => Some("pictogram-load"),
            Self::PictogramUpload { .. } => Some("pictogram-upload"),
            Self::PictogramSlotUpload { .. } => Some("pictogram-slot-upload"),
            _ => None,
        }
    }
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
    MacrosWritten,
    BackgroundUploaded(crate::app::standby_background::BackgroundUploadResult),
    BackgroundCleared,
    BackgroundCancelled {
        cleared: bool,
    },
    BackgroundSpeedSet,
    StartupImageUploaded(crate::app::standby_background::StartupImageUploadResult),
    StartupImageCleared,
    PictogramsLoaded(crate::app::PictogramLibrary),
    PictogramsUploaded(crate::app::PictogramLibrary),
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
    progress: std::sync::Arc<std::sync::atomic::AtomicU32>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VialHidTaskStart {
    Started,
    Busy,
    NoDevice,
}

#[cfg(not(target_arch = "wasm32"))]
fn run_vial_hid_operation_with_progress(
    hid: &crate::hid::HidDevice,
    operation: VialHidOperation,
    progress: &std::sync::atomic::AtomicU32,
    cancel: &std::sync::atomic::AtomicBool,
) -> anyhow::Result<VialHidOutcome> {
    match operation {
        VialHidOperation::UnlockStart => {
            let (unlocked, in_progress, keys) = hid.get_unlock_status_with_progress()?;
            if !unlocked && !in_progress {
                hid.unlock_start()?;
            }
            Ok(VialHidOutcome::UnlockStarted {
                unlocked: unlocked && !in_progress,
                keys,
            })
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
        VialHidOperation::Matrix { rows, cols, .. } => hid
            .get_switch_matrix(rows, cols)
            .map(VialHidOutcome::Matrix),
        VialHidOperation::BatteryRefresh => hid.get_battery_halves().map(VialHidOutcome::Battery),
        VialHidOperation::KeyWrite {
            layer,
            row,
            col,
            binding,
            ..
        } => match binding {
            crate::keyboard::KeyBinding::Vial(keycode) => hid
                .set_keycode(layer as u8, row, col, keycode)
                .map(|()| VialHidOutcome::KeyWritten),
            crate::keyboard::KeyBinding::Rmk(action) => hid
                .set_rmk_key_action(layer as u8, row, col, action)
                .map(|()| VialHidOutcome::KeyWritten),
        },
        VialHidOperation::EncoderWrite {
            layer,
            encoder_index,
            direction,
            keycode,
            ..
        } => hid
            .set_encoder(layer as u8, encoder_index, direction, keycode)
            .map(|()| VialHidOutcome::EncoderWritten),
        VialHidOperation::MacroWrite { macros, .. } => {
            let size = hid.get_macro_buffer_size()?;
            let buffer = crate::hid::HidDevice::encode_macros(&macros, size);
            hid.set_macro_buffer(&buffer)?;
            Ok(VialHidOutcome::MacrosWritten)
        }
        VialHidOperation::BackgroundUpload {
            path,
            fallback,
            scale,
        } => hid
            .upload_standby_background(&path, fallback, scale, progress, cancel)
            .map(|result| match result {
                Some(upload) => VialHidOutcome::BackgroundUploaded(upload),
                None => VialHidOutcome::BackgroundCancelled {
                    cleared: progress.load(std::sync::atomic::Ordering::Relaxed) >= 150,
                },
            }),
        VialHidOperation::BackgroundClear => {
            hid.clear_standby_background()?;
            Ok(VialHidOutcome::BackgroundCleared)
        }
        VialHidOperation::BackgroundSpeed { percent, .. } => {
            hid.set_standby_background_speed(percent)?;
            Ok(VialHidOutcome::BackgroundSpeedSet)
        }
        VialHidOperation::StartupImageUpload { path, fallback } => hid
            .upload_startup_image(&path, fallback, progress)
            .map(VialHidOutcome::StartupImageUploaded),
        VialHidOperation::StartupImageClear => {
            hid.clear_startup_image()?;
            Ok(VialHidOutcome::StartupImageCleared)
        }
        VialHidOperation::PictogramLoad { .. } => hid
            .load_pictograms(progress)
            .map(VialHidOutcome::PictogramsLoaded),
        VialHidOperation::PictogramUpload { library } => hid
            .upload_pictograms(library, progress)
            .map(VialHidOutcome::PictogramsUploaded),
        VialHidOperation::PictogramSlotUpload {
            library,
            kind,
            slot,
        } => hid
            .upload_pictogram_slot(library, kind, slot, progress)
            .map(VialHidOutcome::PictogramsUploaded),
        VialHidOperation::Deferred(request) => {
            super::device_deferred_load::run_deferred_load(hid, &request)
                .map(VialHidOutcome::Deferred)
        }
    }
}

#[cfg(test)]
fn run_vial_hid_operation(
    hid: &crate::hid::HidDevice,
    operation: VialHidOperation,
) -> anyhow::Result<VialHidOutcome> {
    let progress = std::sync::atomic::AtomicU32::new(0);
    run_vial_hid_operation_with_progress(
        hid,
        operation,
        &progress,
        &std::sync::atomic::AtomicBool::new(false),
    )
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
    pub(super) fn schedule_battery_refresh_for_result(
        &mut self,
        battery: Option<crate::hid::BatteryHalves>,
    ) {
        self.next_battery_refresh_at = self
            .device_about_info
            .as_ref()
            .filter(|info| info.supports_battery_halves)
            .map(|_| std::time::Instant::now() + battery_refresh_delay(battery));
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
    pub(super) fn cancel_background_upload(&self) {
        if let Some(task) = &self.vial_hid_task {
            if matches!(task.operation, VialHidOperation::BackgroundUpload { .. }) {
                task.cancel
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn standby_background_upload_progress(&self) -> Option<f32> {
        self.vial_hid_task.as_ref().and_then(|task| {
            matches!(task.operation, VialHidOperation::BackgroundUpload { .. })
                .then(|| task.progress.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0)
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn startup_image_upload_progress(&self) -> Option<f32> {
        self.vial_hid_task.as_ref().and_then(|task| {
            matches!(task.operation, VialHidOperation::StartupImageUpload { .. })
                .then(|| task.progress.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0)
        })
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
        self.start_vial_hid_operation_with_runner(
            ctx,
            operation,
            run_vial_hid_operation_with_progress,
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start_vial_hid_operation_with_runner(
        &mut self,
        ctx: &egui::Context,
        operation: VialHidOperation,
        run: impl FnOnce(
                &crate::hid::HidDevice,
                VialHidOperation,
                &std::sync::atomic::AtomicU32,
                &std::sync::atomic::AtomicBool,
            ) -> anyhow::Result<VialHidOutcome>
            + Send
            + 'static,
    ) -> VialHidTaskStart {
        if self.vial_hid_task.is_some() || self.another_hid_owner_or_write_is_pending() {
            return VialHidTaskStart::Busy;
        }
        let Some(hid_device) = self.hid_device.take() else {
            return VialHidTaskStart::NoDevice;
        };

        if matches!(operation, VialHidOperation::PictogramLoad { .. }) {
            self.display_settings.pictograms.loading = true;
            self.display_settings.pictograms.load_failure = None;
        }
        let generation = self.connection_generation;
        let (sender, receiver) = std::sync::mpsc::channel();
        let repaint_ctx = ctx.clone();
        let task_operation = operation.clone();
        let progress = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let worker_progress = progress.clone();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let diagnostic_label = operation.display_diagnostic_label();
        if let Some(label) = diagnostic_label {
            let target = self
                .selected_device
                .and_then(|index| self.device_manager.devices().get(index))
                .map(|device| device.path.as_str());
            log::debug!(
                "Display operation started: {label} generation={generation} target={target:?}"
            );
        }
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            #[cfg(target_os = "macos")]
            let _hid_lock = hid_device.macos_hid_operation_lock();

            let outcome = run(
                &hid_device,
                operation.clone(),
                &worker_progress,
                &worker_cancel,
            );
            let disconnected = outcome
                .as_ref()
                .err()
                .map(crate::hid::is_disconnect_error)
                .unwrap_or(false);
            if let Some(label) = diagnostic_label {
                match &outcome {
                    Ok(_) => log::debug!(
                        "Display operation completed: {label} generation={generation} elapsed_ms={}",
                        started.elapsed().as_millis(),
                    ),
                    Err(error) => log::debug!(
                        "Display operation failed: {label} generation={generation} elapsed_ms={} disconnected={disconnected} error={error:#}",
                        started.elapsed().as_millis(),
                    ),
                }
            }
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
            progress,
            cancel,
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
                remember_ever_pressed,
            },
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn maybe_start_macro_write(&mut self, ctx: &egui::Context) {
        if !self.keycode_picker.macros_dirty
            || self.keycode_picker.open
            || self.hid_write_task_active()
            || self.keycode_picker.macro_attempted_revision
                == Some(self.keycode_picker.macro_edit_revision)
        {
            return;
        }
        if self.unlock_open || self.vial_unlock_polling {
            return;
        }
        if self.is_vial_locked() {
            self.unlock_open = true;
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "connection.keyboard_locked_edit_macros",
            )
            .into();
            return;
        }

        let revision = self.keycode_picker.macro_edit_revision;
        let operation = VialHidOperation::MacroWrite {
            macros: self.keycode_picker.macro_texts.clone(),
            revision,
        };
        match self.start_vial_hid_operation(ctx, operation) {
            VialHidTaskStart::Started => {
                // A later edit can set this again while the snapshot is being
                // written; completion must not clear that newer dirty state.
                self.keycode_picker.macros_dirty = false;
                self.keycode_picker.macro_attempted_revision = Some(revision);
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "status_messages.macros_saving",
                )
                .into();
            }
            VialHidTaskStart::Busy => {}
            VialHidTaskStart::NoDevice => {
                self.keycode_picker.macro_attempted_revision = Some(revision);
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "status_messages.macro_write_error",
                    &[("error", "device handle is not available")],
                );
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_vial_hid_task(&mut self, ctx: &egui::Context) {
        self.poll_vial_hid_task_with_settings_save(ctx, save_app_settings);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_vial_hid_task_with_settings_save(
        &mut self,
        ctx: &egui::Context,
        save_settings: impl FnOnce(&AppSettings),
    ) {
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
                if self.app_settings.sticky_layout_window {
                    ctx.request_repaint_of(
                        super::layout_indicator_window::sticky_layout_viewport_id(),
                    );
                }
            }
            Ok(VialHidOutcome::Battery(battery)) => {
                if let Some(info) = self.device_about_info.as_mut() {
                    info.battery_halves = battery;
                }
                self.schedule_battery_refresh_for_result(battery);
            }
            Ok(VialHidOutcome::KeyWritten) => {
                if let VialHidOperation::KeyWrite {
                    layer,
                    key_index,
                    old_binding,
                    binding,
                    is_undo,
                    ..
                } = result.operation
                {
                    self.finish_keycode_write(layer, key_index, old_binding, binding, is_undo);
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
            Ok(VialHidOutcome::MacrosWritten) => {
                let VialHidOperation::MacroWrite { revision, .. } = &result.operation else {
                    unreachable!("macro outcome must come from a macro write operation");
                };
                if self.keycode_picker.macro_edit_revision == *revision {
                    self.keycode_picker.mark_macros_clean();
                }
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "status_messages.macros_saved",
                )
                .into();
            }
            Ok(VialHidOutcome::BackgroundUploaded(upload)) => {
                self.app_settings.standby_background_source_path = Some(upload.source_path.clone());
                save_settings(&self.app_settings);
                self.display_settings.clock_background_kind = upload.kind;
                self.display_settings.clock_background_frames = upload.frame_count;
                self.display_settings.clock_background_bytes = upload.total_size;
                self.display_settings.clock_background_file_name = Some(upload.file_name);
                self.display_settings.clock_background_preview_rgba = upload.preview_rgba;
                self.display_settings.clock_background_preview_frames_rgba =
                    upload.preview_frames_rgba;
                self.display_settings.clock_background_preview_delays_ms = upload.preview_delays_ms;
                self.display_settings.clock_background_preview_revision = self
                    .display_settings
                    .clock_background_preview_revision
                    .wrapping_add(1);
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.background_uploaded",
                )
                .into();
            }
            Ok(VialHidOutcome::BackgroundCancelled { cleared: false }) => {
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.upload_cancelled",
                )
                .into();
            }
            Ok(
                VialHidOutcome::BackgroundCleared
                | VialHidOutcome::BackgroundCancelled { cleared: true },
            ) => {
                self.display_settings.clock_background_kind = 0;
                self.display_settings.clock_background_frames = 0;
                self.display_settings.clock_background_bytes = 0;
                self.display_settings.clock_background_file_name = None;
                self.display_settings.clock_background_preview_rgba.clear();
                self.display_settings
                    .clock_background_preview_frames_rgba
                    .clear();
                self.display_settings
                    .clock_background_preview_delays_ms
                    .clear();
                self.display_settings.clock_background_preview_revision = self
                    .display_settings
                    .clock_background_preview_revision
                    .wrapping_add(1);
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    if matches!(result.operation, VialHidOperation::BackgroundUpload { .. }) {
                        "display_settings.upload_cancelled"
                    } else {
                        "display_settings.background_cleared"
                    },
                )
                .into();
            }
            Ok(VialHidOutcome::BackgroundSpeedSet) => {
                let VialHidOperation::BackgroundSpeed { percent, .. } = result.operation else {
                    unreachable!("background speed outcome must come from a speed operation");
                };
                self.display_settings.clock_background_speed_percent = percent;
                self.display_settings
                    .confirmed_clock_background_speed_percent = percent;
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.background_speed_saved",
                )
                .into();
            }
            Ok(VialHidOutcome::StartupImageUploaded(upload)) => {
                self.display_settings.startup_image_present = true;
                self.display_settings.startup_image_bytes = upload.total_size;
                self.display_settings.startup_image_file_name = Some(upload.file_name);
                self.display_settings.startup_image_preview_rgba = upload.preview_rgba;
                self.display_settings.startup_image_preview_revision = self
                    .display_settings
                    .startup_image_preview_revision
                    .wrapping_add(1);
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.startup_image_uploaded",
                )
                .into();
            }
            Ok(VialHidOutcome::StartupImageCleared) => {
                self.display_settings.startup_image_present = false;
                self.display_settings.startup_image_bytes = 0;
                self.display_settings.startup_image_file_name = None;
                self.display_settings.startup_image_preview_rgba.clear();
                self.display_settings.startup_image_preview_revision = self
                    .display_settings
                    .startup_image_preview_revision
                    .wrapping_add(1);
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.startup_image_cleared",
                )
                .into();
            }
            Ok(VialHidOutcome::PictogramsLoaded(library)) => {
                self.display_settings.pictograms.load_failure = None;
                self.display_settings.pictograms.supported = Some(true);
                self.display_settings.pictograms.loaded = true;
                self.display_settings.pictograms.loading = false;
                self.display_settings.pictograms.library = library;
                self.display_settings.pictograms.preserve_editor_on_load = false;
                if !matches!(
                    result.operation,
                    VialHidOperation::PictogramLoad {
                        preserve_editor: true
                    }
                ) {
                    self.restore_pictogram_editor_from_device();
                }
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.pictograms_loaded",
                )
                .into();
            }
            Ok(VialHidOutcome::PictogramsUploaded(library)) => {
                self.display_settings.pictograms.load_failure = None;
                self.display_settings.pictograms.preserve_editor_on_load = false;
                self.display_settings.pictograms.supported = Some(true);
                self.display_settings.pictograms.loaded = true;
                self.display_settings.pictograms.loading = false;
                self.display_settings.pictograms.saving = false;
                self.display_settings.pictograms.library = library;
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.pictograms_saved",
                )
                .into();
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
        old_binding: crate::keyboard::KeyBinding,
        binding: crate::keyboard::KeyBinding,
        is_undo: bool,
    ) {
        if let Some(layout) = self.layout.as_mut() {
            layout.set_key_binding(layer, key_index, binding);
        }
        if !is_undo {
            self.undo_stack.push(UndoAction::Key {
                layer,
                key_idx: key_index,
                old_binding,
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
                old_binding,
                binding,
                is_undo,
                ..
            } => {
                if let Some(layout) = self.layout.as_mut() {
                    if layout.get_key_binding(*layer, *key_index) == *binding {
                        layout.set_key_binding(*layer, *key_index, *old_binding);
                    }
                }
                if *is_undo {
                    self.undo_stack.push(UndoAction::Key {
                        layer: *layer,
                        key_idx: *key_index,
                        old_binding: *binding,
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
        match &operation {
            VialHidOperation::PictogramLoad { .. } => {
                self.display_settings.pictograms.loading = false;
                self.display_settings.pictograms.load_failure =
                    Some((self.connection_generation, error.clone()));
                // A failed read is not proof that the firmware lacks storage.
                if self.display_settings.pictograms.supported != Some(true) {
                    self.display_settings.pictograms.supported = Some(false);
                }
            }
            VialHidOperation::PictogramUpload { .. }
            | VialHidOperation::PictogramSlotUpload { .. } => {
                let pictograms = &mut self.display_settings.pictograms;
                // Flash/transport errors can leave even the old library uncertain.
                // Invalidate the device snapshot, not the user's editor; the next
                // storage read preserves that editor and confirms the actual bytes.
                pictograms.loaded = false;
                pictograms.loading = false;
                pictograms.saving = false;
                pictograms.load_failure = None;
                pictograms.preserve_editor_on_load = true;
                pictograms.library = PictogramLibrary::default();
                pictograms.upload_due = None;
            }
            _ => {}
        }

        if let VialHidOperation::MacroWrite { revision, .. } = &operation {
            self.keycode_picker.macros_dirty = true;
            self.keycode_picker.macro_attempted_revision =
                (self.keycode_picker.macro_edit_revision == *revision).then_some(*revision);
            self.status_msg = crate::i18n::tr_catalog_format(
                self.app_settings.language,
                "status_messages.macro_write_error",
                &[("error", &error)],
            );
        }

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
            // Connection cleanup owns device state, but an uncertain write must
            // not erase the user's independent editor. Carry the invalidated
            // snapshot through cleanup; never restore a confirmed device library.
            let draft = matches!(
                operation,
                VialHidOperation::PictogramUpload { .. }
                    | VialHidOperation::PictogramSlotUpload { .. }
                    | VialHidOperation::PictogramLoad {
                        preserve_editor: true
                    }
            )
            .then(|| {
                let mut draft = std::mem::take(&mut self.display_settings.pictograms);
                draft.supported = None;
                draft.loaded = false;
                draft.loading = false;
                draft.saving = false;
                draft.library = PictogramLibrary::default();
                draft
            });
            if !self.begin_bluetooth_reconnect(error.clone()) {
                self.clear_connected_keyboard_state(error);
            }
            if let Some(draft) = draft {
                self.display_settings.pictograms = draft;
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
            VialHidOperation::MacroWrite { .. } => {
                // The localized error and edit-gated dirty state were set above.
            }
            VialHidOperation::BackgroundUpload { .. } | VialHidOperation::BackgroundClear => {
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "display_settings.background_error",
                    &[("error", &error)],
                );
            }
            VialHidOperation::BackgroundSpeed { old_percent, .. } => {
                self.display_settings.clock_background_speed_percent = old_percent;
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "display_settings.background_speed_error",
                    &[("error", &error)],
                );
            }
            VialHidOperation::StartupImageUpload { .. } | VialHidOperation::StartupImageClear => {
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "display_settings.startup_image_error",
                    &[("error", &error)],
                );
            }
            VialHidOperation::PictogramLoad { .. } => {
                self.status_msg = format!(
                    "{}: {error}",
                    crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "display_settings.pictograms_firmware_required",
                    )
                );
            }
            VialHidOperation::PictogramUpload { .. }
            | VialHidOperation::PictogramSlotUpload { .. } => {
                self.status_msg = format!(
                    "{}: {error}",
                    crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "display_settings.pictograms_status",
                    )
                );
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod repeat_lifecycle_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32};
    use std::time::{Duration, Instant};

    fn app() -> (EntropyApp, egui::Context, crate::hid::TestHidRecorder) {
        let mut app = EntropyApp::new_inert_for_test();
        assert!(app.device_manager.devices().is_empty());
        assert!(matches!(app.update_check, UpdateCheckState::Idle));
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid);
        app.app_settings.language = crate::i18n::Language::Russian;
        app.display_settings.supported = true;
        app.display_settings.clock_settings_supported = true;
        app.display_settings.clock_background_asset_supported = true;
        app.display_settings.pictograms.supported = Some(true);
        app.display_settings.pictograms.loaded = true;
        app.keycode_picker.macro_count = 4;
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        for family in ["display_preview", "clock_montserrat"] {
            fonts.families.insert(
                egui::FontFamily::Name(family.into()),
                fonts.families[&egui::FontFamily::Proportional].clone(),
            );
        }
        ctx.set_fonts(fonts);
        (app, ctx, recorder)
    }

    fn frame(
        app: &mut EntropyApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 1000.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.draw_display_settings_page(ui, ui.max_rect()),
        )
    }

    fn text_position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing rendered text: {label}"))
    }

    fn tab(app: &mut EntropyApp, ctx: &egui::Context, label: &str) {
        let output = frame(app, ctx, Vec::new());
        let pos = text_position(&output, label);
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    }

    fn poll(app: &mut EntropyApp, ctx: &egui::Context, saved: &mut usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.vial_hid_task_active() {
            assert!(
                Instant::now() < deadline,
                "same-session worker did not finish"
            );
            app.poll_vial_hid_task_with_settings_save(ctx, |_| *saved += 1);
            frame(app, ctx, Vec::new());
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn repeat_lifecycle_two_pictogram_uploads_use_one_app_and_remain_renderable() {
        let (mut app, ctx, recorder) = app();
        tab(&mut app, &ctx, "Пиктограммы");
        let generation = app.connection_generation;
        let mut saved = 0;
        for byte in [0xAA, 0x55] {
            recorder.respond_with(test_pictogram_upload_responses(true, 0));
            app.display_settings.pictograms.source_levels =
                pictogram_bitmap_levels(&[byte; PICTOGRAM_BYTES]);
            assert!(app.apply_current_pictogram(&ctx));
            frame(&mut app, &ctx, Vec::new());
            poll(&mut app, &ctx, &mut saved);
            assert_eq!(app.connection_generation, generation);
            assert!(app.hid_device.is_some());
            assert!(app.display_settings.pictograms.loaded);
            assert!(!app.display_settings.pictograms.loading);
            assert!(!app.vial_hid_task_blocks_user_action());
            let output = frame(&mut app, &ctx, Vec::new());
            text_position(&output, "Выбрать");
            tab(&mut app, &ctx, "Выбрать");
            assert!(
                egui::Popup::is_any_open(&ctx),
                "Select did not reopen after upload"
            );
            egui::Popup::close_all(&ctx);
        }
        assert_eq!(saved, 0);
        assert_eq!(
            recorder.requests().iter().filter(|r| r[0] == 0xC9).count(),
            2
        );
    }

    fn directory() -> tempfile::TempDir {
        let root = std::env::var_os("ENTROPY_TEST_ARTIFACT_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        tempfile::Builder::new()
            .prefix("repeat-lifecycle-")
            .tempdir_in(root)
            .unwrap()
    }

    #[test]
    fn repeat_lifecycle_failed_recovery_read_stops_until_explicit_retry() {
        let (mut app, ctx, recorder) = app();
        tab(&mut app, &ctx, "Пиктограммы");
        let before = app.display_settings.pictograms.library.clone();
        app.display_settings.pictograms.editor_name = "Unsaved draft".into();
        let draft = app.display_settings.pictograms.source_levels.clone();
        recorder.respond_with(test_pictogram_upload_responses(true, 4));
        // A real firmware error on the automatic recovery QUERY, not a timeout
        // or an unsupported capability. Known support must remain known.
        let mut read_error = [0; 32];
        read_error[0] = 0xC0;
        read_error[1] = 4;
        recorder.respond_with([read_error]);
        assert!(app.apply_current_pictogram(&ctx));
        for _ in 0..30 {
            app.poll_vial_hid_task_with_settings_save(&ctx, |_| panic!("no settings write"));
            frame(&mut app, &ctx, Vec::new());
            std::thread::sleep(Duration::from_millis(2));
        }
        let queries = recorder.requests().iter().filter(|r| r[0] == 0xC0).count();
        assert_eq!(
            queries, 2,
            "upload QUERY + one recovery QUERY, not a render-driven retry loop"
        );
        assert!(!app.vial_hid_task_active());
        assert_eq!(app.display_settings.pictograms.supported, Some(true));
        assert!(!app.display_settings.pictograms.loaded);
        assert_eq!(app.display_settings.pictograms.source_levels, draft);
        assert_eq!(app.display_settings.pictograms.editor_name, "Unsaved draft");
        assert!(app
            .display_settings
            .pictograms
            .load_failure
            .as_ref()
            .unwrap()
            .1
            .contains("status 4"));
        // Merely navigating away and back is not a recovery attempt.
        tab(&mut app, &ctx, "Экран ожидания");
        tab(&mut app, &ctx, "Пиктограммы");
        assert_eq!(
            recorder.requests().iter().filter(|r| r[0] == 0xC0).count(),
            2
        );
        // The existing Select control is an explicit read retry boundary when
        // the snapshot is unknown; it must never seed another upload itself.
        recorder.respond_with(test_pictogram_read_responses(&before));
        tab(&mut app, &ctx, "Выбрать");
        poll(&mut app, &ctx, &mut 0);
        assert!(app.display_settings.pictograms.loaded);
        assert_eq!(app.display_settings.pictograms.library, before);
        assert!(app.display_settings.pictograms.load_failure.is_none());
        assert_eq!(app.display_settings.pictograms.source_levels, draft);
        recorder.respond_with(test_pictogram_upload_responses(true, 0));
        assert!(app.apply_current_pictogram(&ctx));
        poll(&mut app, &ctx, &mut 0);
        assert!(app.hid_device.is_some());
        assert!(app.display_settings.pictograms.loaded);
    }

    #[test]
    fn repeat_lifecycle_stale_read_result_cannot_overwrite_successor_state() {
        let (mut app, ctx, _) = app();
        let (release, gate) = std::sync::mpsc::channel();
        assert_eq!(
            app.start_vial_hid_operation_with_runner(
                &ctx,
                VialHidOperation::PictogramLoad {
                    preserve_editor: true
                },
                move |_, _, _, _| {
                    gate.recv_timeout(Duration::from_secs(3)).unwrap();
                    anyhow::bail!("old read failed")
                },
            ),
            VialHidTaskStart::Started
        );
        app.connection_generation += 1;
        app.display_settings.pictograms = PictogramSettingsState::default();
        app.display_settings.pictograms.editor_name = "Successor".into();
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.vial_hid_task_active() {
            assert!(Instant::now() < deadline);
            app.poll_vial_hid_task_with_settings_save(&ctx, |_| panic!("no save"));
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(app.hid_device.is_none(), "stale HID handle was restored");
        assert!(app.display_settings.pictograms.load_failure.is_none());
        assert_eq!(app.display_settings.pictograms.editor_name, "Successor");
    }

    fn start_background(
        app: &mut EntropyApp,
        ctx: &egui::Context,
        directory: &std::path::Path,
        gate: Option<std::sync::mpsc::Receiver<()>>,
    ) {
        let path = directory.join("source.png");
        let red = if app.display_settings.clock_background_preview_revision == 0 {
            32
        } else {
            224
        };
        image::RgbaImage::from_pixel(8, 8, image::Rgba([red, 87, 120, 255]))
            .save(&path)
            .unwrap();
        let cache = directory.join("cache.ehbg");
        assert_eq!(
            app.start_vial_hid_operation_with_runner(
                ctx,
                VialHidOperation::BackgroundUpload {
                    path,
                    fallback: [0; 3],
                    scale: StandbyBackgroundScale::Fill
                },
                move |hid, operation, progress: &AtomicU32, cancel: &AtomicBool| {
                    if let Some(gate) = gate {
                        gate.recv_timeout(Duration::from_secs(3)).unwrap();
                    }
                    let VialHidOperation::BackgroundUpload {
                        path,
                        fallback,
                        scale,
                    } = operation
                    else {
                        panic!("wrong operation")
                    };
                    hid.upload_standby_background_with_cache(
                        &path,
                        fallback,
                        scale,
                        progress,
                        cancel,
                        |package| Ok(std::fs::write(&cache, package)?),
                    )
                    .map(|upload| match upload {
                        Some(upload) => VialHidOutcome::BackgroundUploaded(upload),
                        None => VialHidOutcome::BackgroundCancelled {
                            cleared: progress.load(std::sync::atomic::Ordering::Relaxed) >= 150,
                        },
                    })
                },
            ),
            VialHidTaskStart::Started
        );
    }

    #[test]
    fn repeat_lifecycle_two_standby_uploads_keep_same_handle_and_gui_live() {
        let (mut app, ctx, recorder) = app();
        tab(&mut app, &ctx, "Экран ожидания");
        let directory = directory();
        let generation = app.connection_generation;
        let mut saved = 0;
        let mut previous_package = Vec::new();
        for revision in 1..=2 {
            recorder
                .respond_with(crate::app::standby_background::test_background_upload_responses(0));
            let (release, gate) = std::sync::mpsc::channel();
            start_background(&mut app, &ctx, directory.path(), Some(gate));
            // Deterministically stalled worker, not HID. GUI polling/rendering
            // must remain independent while the original owner is in-flight.
            let started = Instant::now();
            for _ in 0..3 {
                app.poll_vial_hid_task_with_settings_save(&ctx, |_| panic!("worker is gated"));
                let output = frame(&mut app, &ctx, Vec::new());
                text_position(&output, "Отмена");
                assert!(app.standby_background_upload_progress().is_some());
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            release.send(()).unwrap();
            poll(&mut app, &ctx, &mut saved);
            assert_eq!(
                app.display_settings.clock_background_preview_revision,
                revision
            );
            assert!(app.standby_background_upload_progress().is_none());
            assert!(!app.vial_hid_task_blocks_user_action());
            assert!(app.hid_device.is_some());
            assert_eq!(app.connection_generation, generation);
            let output = frame(&mut app, &ctx, Vec::new());
            text_position(&output, "Загрузить");
            text_position(&output, "Сбросить");
            assert!(
                std::fs::metadata(directory.path().join("cache.ehbg"))
                    .unwrap()
                    .len()
                    > 256
            );
            let package = std::fs::read(directory.path().join("cache.ehbg")).unwrap();
            assert_ne!(
                package, previous_package,
                "second upload must replace different pixels"
            );
            previous_package = package;
        }
        assert_eq!(saved, 2);
        assert_eq!(
            recorder.requests().iter().filter(|r| r[0] == 0xB3).count(),
            2
        );
    }

    #[test]
    fn repeat_lifecycle_failed_or_cancelled_standby_upload_can_retry() {
        for cancel_first in [false, true] {
            let (mut app, ctx, recorder) = app();
            tab(&mut app, &ctx, "Экран ожидания");
            let directory = directory();
            let mut saved = 0;
            let generation = app.connection_generation;
            let (release, gate) = std::sync::mpsc::channel();
            if !cancel_first {
                recorder.respond_with(
                    crate::app::standby_background::test_background_upload_responses(4),
                );
            }
            start_background(&mut app, &ctx, directory.path(), Some(gate));
            if cancel_first {
                app.cancel_background_upload();
            }
            release.send(()).unwrap();
            poll(&mut app, &ctx, &mut saved);
            assert!(app.hid_device.is_some());
            assert_eq!(saved, 0);
            assert_eq!(app.display_settings.clock_background_preview_revision, 0);
            if cancel_first {
                assert!(recorder.requests().is_empty());
            } else {
                assert!(app.status_msg.contains("status 4"));
            }
            recorder
                .respond_with(crate::app::standby_background::test_background_upload_responses(0));
            start_background(&mut app, &ctx, directory.path(), None);
            poll(&mut app, &ctx, &mut saved);
            assert_eq!(saved, 1);
            assert_eq!(app.connection_generation, generation);
            assert!(app.hid_device.is_some());
            assert!(!app.vial_hid_task_blocks_user_action());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::KeyBinding;

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
            layers: vec![vec![keycode.into()]],
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
            rgb_supported: false,
            lighting_mode: None,
            supports_rmk_native_key_actions: false,
            supports_universal_symbols: false,
            supports_universal_russian_letters: false,
            supports_rmk_native_combo_output: false,
            supports_rmk_native_tap_dance_actions: false,
            supports_rmk_combo_layers: false,
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
    fn pending_unlock_is_not_ready_and_does_not_restart_physical_hold() {
        // The firmware keeps the unlocked bit set if another unlock sequence
        // starts on an unlocked board, but gates normal commands while pending.
        for unlocked in [0, 1] {
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            let mut status = [0xFF; 32];
            status[..4].copy_from_slice(&[unlocked, 1, 0, 0]);
            recorder.respond_with([status]);
            let result = run_vial_hid_operation(&hid, VialHidOperation::UnlockStart).unwrap();
            assert!(matches!(
                result,
                VialHidOutcome::UnlockStarted {
                    unlocked: false,
                    ..
                }
            ));
            assert_eq!(
                recorder.requests().len(),
                1,
                "an existing physical hold must not receive another FE06"
            );
            assert!(recorder.requests()[0].starts_with(&[0xFE, 5]));
        }
    }

    #[test]
    fn pending_unlock_status_is_not_exposed_as_available_for_normal_commands() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let mut status = [0xFF; 32];
        status[..4].copy_from_slice(&[1, 1, 0, 0]);
        recorder.respond_with([status]);
        let (ready, keys) = hid.get_unlock_status().unwrap();
        assert!(!ready);
        assert_eq!(keys, vec![(0, 0)]);
    }

    #[test]
    fn pending_unlock_poll_does_not_complete_on_the_unlocked_bit_alone() {
        let mut app = EntropyApp::new_inert_for_test();
        app.unlock_open = true;
        app.finish_vial_unlock_start(false, vec![(0, 0)]);
        app.finish_vial_unlock_poll(true, true, 10);
        assert!(app.unlock_open && app.vial_unlock_polling);
        assert_eq!(app.vial_unlocked, Some(false));
        app.finish_vial_unlock_poll(true, false, 0);
        assert!(!app.unlock_open && !app.vial_unlock_polling);
        assert_eq!(app.vial_unlocked, Some(true));
    }

    #[test]
    fn stopped_unlock_poll_releases_ui_instead_of_polling_forever() {
        let mut app = EntropyApp::new_inert_for_test();
        app.unlock_open = true;
        app.finish_vial_unlock_start(false, vec![(0, 0)]);
        app.finish_vial_unlock_poll(false, false, 0);
        assert!(!app.unlock_open && !app.vial_unlock_polling);
        assert_eq!(app.vial_unlocked, Some(false));
        assert!(app.macro_auto_unlock_cancelled);
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
        assert_eq!(requests.len(), 5);
        assert!(requests
            .iter()
            .all(|request| &request[..3] == [0x08, 0xE8, 0x01]));
    }

    #[test]
    fn incomplete_split_battery_results_retry_soon() {
        assert_eq!(
            battery_refresh_delay(Some(crate::hid::BatteryHalves {
                left: Some(80),
                right: Some(75),
            })),
            BATTERY_REFRESH_INTERVAL
        );
        assert_eq!(
            battery_refresh_delay(Some(crate::hid::BatteryHalves {
                left: Some(80),
                right: None,
            })),
            BATTERY_INCOMPLETE_REFRESH_INTERVAL
        );
        assert_eq!(
            battery_refresh_delay(None),
            BATTERY_INCOMPLETE_REFRESH_INTERVAL
        );
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
                old_binding: 0x0004.into(),
                binding: 0.into(),
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
    fn macro_write_uses_the_serialized_background_transport() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let mut macros = vec![Vec::new(); 32];
        macros[0] = vec![b'x'; 30];
        app.hid_device = Some(hid);
        app.keycode_picker.macro_texts = macros;
        app.keycode_picker.mark_macros_dirty();

        app.maybe_start_macro_write(&ctx);

        assert!(app.vial_hid_task_active());
        assert!(app.hid_device.is_none());
        assert!(!app.keycode_picker.macros_dirty);

        poll_until_vial_hid_idle(&mut app, &ctx);

        assert!(app.hid_device.is_some());
        assert!(!app.keycode_picker.macros_dirty);
        let requests = recorder.requests();
        assert_eq!(requests.len(), 4);
        assert_eq!(requests[0][0], 0x0D);
        assert_eq!(&requests[1][..4], &[0x0F, 0, 0, 28]);
        assert_eq!(&requests[2][..4], &[0x0F, 0, 28, 28]);
        assert_eq!(&requests[3][..4], &[0x0F, 0, 56, 6]);
    }

    #[test]
    fn failed_background_macro_write_waits_for_a_new_edit_before_retrying() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, recorder) = crate::hid::HidDevice::test_device_with_fault_after_requests(Some((
            1,
            crate::hid::TestHidFault::Timeout,
        )));
        app.hid_device = Some(hid);
        app.keycode_picker.macro_texts = vec![Vec::new(); 32];
        app.keycode_picker.mark_macros_dirty();
        let failed_revision = app.keycode_picker.macro_edit_revision;

        app.maybe_start_macro_write(&ctx);
        poll_until_vial_hid_idle(&mut app, &ctx);

        assert!(app.keycode_picker.macros_dirty);
        assert_eq!(
            app.keycode_picker.macro_attempted_revision,
            Some(failed_revision)
        );
        let request_count = recorder.requests().len();
        app.maybe_start_macro_write(&ctx);
        assert!(!app.vial_hid_task_active());
        assert_eq!(recorder.requests().len(), request_count);
    }

    #[test]
    fn a_new_macro_edit_after_failure_can_start_another_write() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, _) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid);
        app.keycode_picker.macro_texts = vec![Vec::new(); 32];
        app.keycode_picker.mark_macros_dirty();
        let failed_revision = app.keycode_picker.macro_edit_revision;
        app.keycode_picker.macro_attempted_revision = Some(failed_revision);

        app.maybe_start_macro_write(&ctx);
        assert!(!app.vial_hid_task_active());

        app.keycode_picker.macro_texts[0].push(b'x');
        app.keycode_picker.mark_macros_dirty();
        assert_ne!(app.keycode_picker.macro_edit_revision, failed_revision);
        app.maybe_start_macro_write(&ctx);
        assert!(app.vial_hid_task_active());
        poll_until_vial_hid_idle(&mut app, &ctx);
        assert!(!app.keycode_picker.macros_dirty);
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
                old_binding: KeyBinding::Vial(0x0004),
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
        layout.layers.push(vec![0.into()]);
        layout.layer_names.push("Layer 1".into());
        layout.encoder_layers.push(Vec::new());
        app.layout = Some(layout);
        app.deferred_device_load = DeferredDeviceLoadState::staged((*context).clone());
        app.undo_stack.push(UndoAction::Key {
            layer: 0,
            key_idx: 0,
            old_binding: 0.into(),
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
    fn background_layer_queues_layer_operation_and_runs_it_before_more_loading() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let context = background_layer_context();
        let mut layout = single_key_layout(0x0004);
        layout.layers.push(vec![0.into()]);
        layout.layer_names.push("Layer 1".into());
        layout.encoder_layers.push(Vec::new());
        app.layout = Some(layout);
        app.deferred_device_load = DeferredDeviceLoadState::staged((*context).clone());
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

        app.apply_layer_snapshot(
            0,
            LayerSnapshot {
                keycodes: vec![0.into()],
                encoder_keycodes: Vec::new(),
            },
            "layer_actions.fill_none",
        );

        assert!(app.pending_layer_write.is_some());
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 0x0004);

        for _ in 0..100 {
            app.poll_vial_hid_task(&ctx);
            app.maybe_start_pending_layer_write();
            app.poll_layer_write(&ctx);
            if !app.vial_hid_task_active()
                && app.pending_layer_write.is_none()
                && app.layer_write_task.is_none()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        assert!(app.pending_layer_write.is_none());
        assert!(app.layer_write_task.is_none());
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 0);
        assert!(matches!(
            app.undo_stack.last(),
            Some(UndoAction::Layer {
                layer: 0,
                old: LayerSnapshot { keycodes, .. },
                requires_firmware: true,
            }) if keycodes == &[0x0004.into()]
        ));
        let requests = recorder.requests();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0][0], 0x12);
        assert_eq!(requests[1][0], 0x05);
        assert_eq!(requests[2][0], 0x04);
    }

    #[test]
    fn config_writes_wait_for_background_layer_and_keep_transport_available() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let context = background_layer_context();
        app.deferred_device_load = DeferredDeviceLoadState::staged((*context).clone());
        app.module_settings.set_value(134, 0);
        app.layer_led_settings = LayerLedSettingsState {
            brightness: Some(LayerLedNumericSetting {
                qsid: 316,
                width: 2,
                value: 32,
                max: 255,
                variants: Vec::new(),
            }),
            supported: true,
            ..LayerLedSettingsState::default()
        };
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
        assert!(app.hid_device.is_none());
        assert!(app.qmk_setting_transport_available());

        app.queue_module_setting_write(
            "Left Modules".to_owned(),
            "Mode".to_owned(),
            "Mode".to_owned(),
            134,
            1,
            0,
            3,
        );
        app.queue_layer_led_setting_write("Layer LED brightness".to_owned(), 316, 2, 32, 128);

        assert!(app.settings_write_task.is_none());
        assert_eq!(app.pending_settings_write_value(134), Some(3));
        assert_eq!(app.pending_settings_write_value(316), Some(128));

        for _ in 0..500 {
            app.poll_vial_hid_task(&ctx);
            app.poll_settings_write(&ctx);
            if !app.vial_hid_task_active() && !app.qmk_settings_write_busy() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        assert!(!app.vial_hid_task_active());
        assert!(!app.qmk_settings_write_busy());
        assert_eq!(app.module_settings.value(134), 3);
        assert_eq!(
            app.layer_led_settings
                .brightness
                .as_ref()
                .map(|setting| setting.value),
            Some(128)
        );

        let requests = recorder.requests();
        let module_set = requests.iter().position(|request| {
            request[0] == 0xFE
                && request[1] == 0x0B
                && u16::from_le_bytes([request[2], request[3]]) == 134
        });
        let layer_led_set = requests.iter().position(|request| {
            request[0] == 0xFE
                && request[1] == 0x0B
                && u16::from_le_bytes([request[2], request[3]]) == 316
        });
        assert_eq!(requests.first().map(|request| request[0]), Some(0x12));
        assert!(module_set.is_some_and(|index| index > 0));
        assert!(layer_led_set.is_some_and(|index| Some(index) > module_set));
        let layer_led_requests = requests
            .iter()
            .filter(|request| {
                request[0] == 0xFE
                    && matches!(request[1], 0x0A | 0x0B)
                    && u16::from_le_bytes([request[2], request[3]]) == 316
            })
            .collect::<Vec<_>>();
        assert_eq!(layer_led_requests.len(), 2);
        assert_eq!(layer_led_requests[0][1], 0x0B);
        assert_eq!(layer_led_requests[1][1], 0x0A);
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
