use super::*;

const SETTINGS_WRITEBACK_DELAYS: [std::time::Duration; MODULE_SETTING_READBACK_ATTEMPTS] = [
    std::time::Duration::from_millis(20),
    std::time::Duration::from_millis(80),
    std::time::Duration::from_millis(200),
];
const SETTINGS_WRITE_STATUS_WIDTH: f32 = 22.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SettingsWriteStatus {
    Pending,
    Saved,
    Failed(String),
}

#[derive(Clone, Debug)]
enum SettingsWriteTarget {
    Module {
        group_title: String,
        field_title: String,
        display_label: String,
    },
    Touchpad {
        display_label: String,
    },
}

impl SettingsWriteTarget {
    fn display_label(&self) -> &str {
        match self {
            Self::Module { display_label, .. } | Self::Touchpad { display_label } => display_label,
        }
    }

    fn log_context(&self) -> String {
        match self {
            Self::Module {
                group_title,
                field_title,
                ..
            } => format!("module group={group_title:?} field={field_title:?}"),
            Self::Touchpad { display_label } => {
                format!("touchpad field={display_label:?}")
            }
        }
    }

    fn is_touchpad(&self) -> bool {
        matches!(self, Self::Touchpad { .. })
    }

    fn reconcile_readback(
        &self,
        module_settings: &mut ModuleSettingsState,
        touchpad_settings: &mut TouchpadSettingsState,
        qsid: u16,
        readback: u16,
    ) {
        match self {
            Self::Module { .. } => module_settings.set_value(qsid, readback),
            Self::Touchpad { .. } => match qsid {
                120 => touchpad_settings.dpi = readback,
                121 => touchpad_settings.sniper_sens = readback.min(u8::MAX as u16) as u8,
                122 => touchpad_settings.scroll_sens = readback.min(u8::MAX as u16) as u8,
                123 => touchpad_settings.text_sens = readback.min(u8::MAX as u16) as u8,
                124 => touchpad_settings.bits = readback.min(u8::MAX as u16) as u8,
                142 => touchpad_settings.auto_layer_enable = readback != 0,
                143 => touchpad_settings.auto_layer = readback.min(u8::MAX as u16) as u8,
                _ => {}
            },
        }
    }
}

#[derive(Clone, Debug)]
struct SettingsWriteRequest {
    id: u64,
    generation: u64,
    qsid: u16,
    width: u8,
    old_value: u16,
    requested: u16,
    target: SettingsWriteTarget,
}

#[derive(Clone, Debug)]
struct SettingsWriteStatusEntry {
    request_id: u64,
    requested: u16,
    status: SettingsWriteStatus,
}

#[derive(Default)]
pub(super) struct SettingsWriteQueueState {
    pending: std::collections::VecDeque<SettingsWriteRequest>,
    statuses: std::collections::BTreeMap<u16, SettingsWriteStatusEntry>,
    next_request_id: u64,
}

impl SettingsWriteQueueState {
    fn enqueue(&mut self, mut request: SettingsWriteRequest) -> u64 {
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        request.id = self.next_request_id;
        self.statuses.insert(
            request.qsid,
            SettingsWriteStatusEntry {
                request_id: request.id,
                requested: request.requested,
                status: SettingsWriteStatus::Pending,
            },
        );
        let id = request.id;
        self.pending.push_back(request);
        id
    }

    fn pop_front(&mut self) -> Option<SettingsWriteRequest> {
        self.pending.pop_front()
    }

    fn status(&self, qsid: u16) -> Option<&SettingsWriteStatus> {
        self.statuses.get(&qsid).map(|entry| &entry.status)
    }

    fn pending_value(&self, qsid: u16) -> Option<u16> {
        let entry = self.statuses.get(&qsid)?;
        matches!(entry.status, SettingsWriteStatus::Pending).then_some(entry.requested)
    }

    fn complete(&mut self, request_id: u64, qsid: u16, status: SettingsWriteStatus) -> bool {
        let Some(entry) = self.statuses.get_mut(&qsid) else {
            return false;
        };
        if entry.request_id != request_id {
            return false;
        }
        entry.status = status;
        true
    }

    fn fail_pending(&mut self, error: &str) {
        while let Some(request) = self.pending.pop_front() {
            self.complete(
                request.id,
                request.qsid,
                SettingsWriteStatus::Failed(error.to_owned()),
            );
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(super) fn clear(&mut self) {
        self.pending.clear();
        self.statuses.clear();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct SettingsWriteTask {
    receiver: std::sync::mpsc::Receiver<SettingsWriteResult>,
    request: SettingsWriteRequest,
}

#[cfg(not(target_arch = "wasm32"))]
struct SettingsWriteResult {
    hid_device: Option<crate::hid::HidDevice>,
    request: SettingsWriteRequest,
    result: Result<u16, ModuleSettingWritebackError>,
    disconnected: bool,
}

#[cfg(not(target_arch = "wasm32"))]
fn run_settings_write(
    hid: &crate::hid::HidDevice,
    request: &SettingsWriteRequest,
) -> (Result<u16, ModuleSettingWritebackError>, bool) {
    let disconnected = std::cell::Cell::new(false);
    let mut readback_attempt = 0;
    let mut state = ModuleSettingsState::default();
    let result = state.write_verified_value(
        request.qsid,
        request.requested,
        || {
            let result = if request.width > 1 {
                hid.set_qmk_setting_u16(request.qsid, request.requested)
            } else {
                hid.set_qmk_setting_u8(request.qsid, request.requested as u8)
            };
            result.map_err(|error| {
                disconnected.set(crate::hid::is_disconnect_error(&error));
                error.to_string()
            })
        },
        || {
            let delay = SETTINGS_WRITEBACK_DELAYS
                [readback_attempt.min(SETTINGS_WRITEBACK_DELAYS.len() - 1)];
            readback_attempt += 1;
            std::thread::sleep(delay);
            let result = if request.width > 1 {
                hid.get_qmk_setting_u16(request.qsid)
            } else {
                hid.get_qmk_setting_u8(request.qsid)
                    .map(|readback| readback as u16)
            };
            result.map_err(|error| {
                disconnected.set(crate::hid::is_disconnect_error(&error));
                error.to_string()
            })
        },
    );
    (result, disconnected.get())
}

impl EntropyApp {
    pub(super) fn qmk_settings_write_busy(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.settings_write_task.is_some() || !self.settings_write_queue.is_empty()
        }
        #[cfg(target_arch = "wasm32")]
        {
            false
        }
    }

    pub(super) fn qmk_setting_transport_available(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.hid_device.is_some() || self.hid_write_task_active()
        }
        #[cfg(target_arch = "wasm32")]
        {
            false
        }
    }

    pub(super) fn settings_write_status_width(
        &self,
        metrics: crate::ui_style::ResponsiveMetrics,
    ) -> f32 {
        metrics.value(SETTINGS_WRITE_STATUS_WIDTH)
    }

    pub(super) fn pending_settings_write_value(&self, qsid: u16) -> Option<u16> {
        self.settings_write_queue.pending_value(qsid)
    }

    #[cfg(test)]
    pub(super) fn settings_write_status(&self, qsid: u16) -> Option<&SettingsWriteStatus> {
        self.settings_write_queue.status(qsid)
    }

    pub(super) fn draw_settings_write_status(
        &self,
        ui: &mut egui::Ui,
        qsid: u16,
        metrics: crate::ui_style::ResponsiveMetrics,
        suppress_tooltips: bool,
    ) {
        let size = egui::vec2(
            self.settings_write_status_width(metrics),
            metrics.settings_control_height(),
        );
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
        let status = self
            .pending_qmk_settings_write_value(qsid)
            .map(|_| SettingsWriteStatus::Pending)
            .or_else(|| self.settings_write_queue.status(qsid).cloned());
        let tooltip = match status.as_ref() {
            Some(SettingsWriteStatus::Pending) => {
                ui.put(
                    rect.shrink(metrics.value(4.0)),
                    egui::Spinner::new().size(metrics.value(12.0)),
                );
                Some(
                    crate::i18n::tr_catalog(self.app_settings.language, "settings_write.pending")
                        .to_owned(),
                )
            }
            Some(SettingsWriteStatus::Saved) => {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "\u{2713}",
                    egui::FontId::proportional(metrics.value(14.0)),
                    egui::Color32::from_rgb(72, 158, 108),
                );
                Some(
                    crate::i18n::tr_catalog(self.app_settings.language, "settings_write.saved")
                        .to_owned(),
                )
            }
            Some(SettingsWriteStatus::Failed(error)) => {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "!",
                    egui::FontId::proportional(metrics.value(14.0)),
                    egui::Color32::from_rgb(205, 80, 80),
                );
                Some(crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "settings_write.failed",
                    &[("error", error)],
                ))
            }
            None => None,
        };
        if !suppress_tooltips {
            if let Some(tooltip) = tooltip {
                response.on_hover_text(tooltip);
            }
        }
    }

    pub(super) fn queue_module_setting_write(
        &mut self,
        group_title: String,
        field_title: String,
        display_label: String,
        qsid: u16,
        width: u8,
        old_value: u16,
        requested: u16,
    ) {
        self.queue_settings_write(SettingsWriteRequest {
            id: 0,
            generation: self.settings_write_generation,
            qsid,
            width,
            old_value,
            requested,
            target: SettingsWriteTarget::Module {
                group_title,
                field_title,
                display_label,
            },
        });
    }

    pub(super) fn queue_touchpad_setting_write(
        &mut self,
        display_label: String,
        qsid: u16,
        width: u8,
        old_value: u16,
        requested: u16,
    ) {
        self.queue_settings_write(SettingsWriteRequest {
            id: 0,
            generation: self.settings_write_generation,
            qsid,
            width,
            old_value,
            requested,
            target: SettingsWriteTarget::Touchpad { display_label },
        });
    }

    fn queue_settings_write(&mut self, request: SettingsWriteRequest) {
        let label = request.target.display_label().to_owned();
        let context = request.target.log_context();
        let qsid = request.qsid;
        let old_value = request.old_value;
        let requested = request.requested;
        self.settings_write_queue.enqueue(request);
        if !self.qmk_setting_transport_available() {
            let error = crate::i18n::tr_catalog(
                self.app_settings.language,
                "settings_write.device_not_connected",
            )
            .to_owned();
            self.settings_write_queue.fail_pending(&error);
            self.status_msg = crate::i18n::tr_catalog_format(
                self.app_settings.language,
                "settings_write.failed_status",
                &[("setting", &label), ("error", &error)],
            );
            log::warn!(
                "settings write skipped: {context} qsid={qsid} old={old_value} requested={requested} error={error}"
            );
            return;
        }

        self.status_msg = crate::i18n::tr_catalog_format(
            self.app_settings.language,
            "settings_write.pending_status",
            &[("setting", &label)],
        );
        self.start_next_settings_write();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start_next_settings_write(&mut self) {
        if self.hid_write_task_owner_active() {
            return;
        }
        let Some(request) = self.settings_write_queue.pop_front() else {
            return;
        };
        let Some(hid_device) = self.hid_device.take() else {
            let error = crate::i18n::tr_catalog(
                self.app_settings.language,
                "settings_write.device_disconnected",
            )
            .to_owned();
            self.settings_write_queue.complete(
                request.id,
                request.qsid,
                SettingsWriteStatus::Failed(error.clone()),
            );
            self.settings_write_queue.fail_pending(&error);
            return;
        };

        let fallback = request.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            #[cfg(target_os = "macos")]
            let _hid_lock = crate::hid::macos_hid_operation_lock();

            let (result, disconnected) = run_settings_write(&hid_device, &request);
            let hid_device = (!disconnected).then_some(hid_device);
            let _ = sender.send(SettingsWriteResult {
                hid_device,
                request,
                result,
                disconnected,
            });
        });
        self.settings_write_task = Some(SettingsWriteTask {
            receiver,
            request: fallback,
        });
    }

    /// Continue Settings work in same frame that another HID owner returns
    /// transport, rather than waiting for background-update cadence.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn continue_pending_settings_writes(&mut self, ctx: &egui::Context) {
        self.cancel_pending_settings_writes_without_transport();
        self.start_next_settings_write();

        if self.qmk_settings_write_busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn start_next_settings_write(&mut self) {}

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_settings_write(&mut self, ctx: &egui::Context) {
        let result = match self.settings_write_task.as_ref() {
            Some(task) => task.receiver.try_recv(),
            None => {
                self.continue_pending_settings_writes(ctx);
                return;
            }
        };

        match result {
            Ok(result) => {
                self.settings_write_task = None;
                self.finish_settings_write(result);
                self.continue_pending_settings_writes(ctx);
                self.resume_pending_device_connect();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let task = self
                    .settings_write_task
                    .take()
                    .expect("settings write task checked above");
                if task.request.generation != self.settings_write_generation {
                    return;
                }
                self.hid_device = None;
                let error = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "settings_write.worker_failed",
                )
                .to_owned();
                self.settings_write_queue.complete(
                    task.request.id,
                    task.request.qsid,
                    SettingsWriteStatus::Failed(error.clone()),
                );
                self.settings_write_queue.fail_pending(&error);
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "settings_write.failed_status",
                    &[
                        ("setting", task.request.target.display_label()),
                        ("error", &error),
                    ],
                );
                self.continue_pending_settings_writes(ctx);
                self.resume_pending_device_connect();
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_settings_write(&mut self, result: SettingsWriteResult) {
        let SettingsWriteResult {
            hid_device,
            request,
            result,
            disconnected,
        } = result;
        if request.generation != self.settings_write_generation {
            return;
        }
        self.hid_device = hid_device;
        let context = request.target.log_context();
        let newer_debounced_value = self.pending_qmk_settings_write_value(request.qsid);
        match result {
            Ok(readback) => {
                if request.target.is_touchpad() {
                    self.qmk_settings_write_queue
                        .record_confirmed_value(request.qsid, readback);
                }
                let current = self.settings_write_queue.complete(
                    request.id,
                    request.qsid,
                    SettingsWriteStatus::Saved,
                );
                if current && newer_debounced_value.is_none() {
                    request.target.reconcile_readback(
                        &mut self.module_settings,
                        &mut self.touchpad_settings,
                        request.qsid,
                        readback,
                    );
                    self.status_msg = crate::i18n::tr_catalog_format(
                        self.app_settings.language,
                        "settings_write.saved_status",
                        &[("setting", request.target.display_label())],
                    );
                }
                log::info!(
                    "settings write saved: {context} qsid={} old={} requested={} readback={}",
                    request.qsid,
                    request.old_value,
                    request.requested,
                    readback,
                );
            }
            Err(error) => {
                if request.target.is_touchpad() {
                    if let ModuleSettingWritebackError::ReadbackMismatch { actual, .. } = &error {
                        self.qmk_settings_write_queue
                            .record_confirmed_value(request.qsid, *actual);
                    }
                }
                let error_text = error.to_string();
                let current = self.settings_write_queue.complete(
                    request.id,
                    request.qsid,
                    SettingsWriteStatus::Failed(error_text.clone()),
                );
                if current && newer_debounced_value.is_none() {
                    if let ModuleSettingWritebackError::ReadbackMismatch { actual, .. } = &error {
                        request.target.reconcile_readback(
                            &mut self.module_settings,
                            &mut self.touchpad_settings,
                            request.qsid,
                            *actual,
                        );
                    }
                    self.status_msg = crate::i18n::tr_catalog_format(
                        self.app_settings.language,
                        "settings_write.failed_status",
                        &[
                            ("setting", request.target.display_label()),
                            ("error", &error_text),
                        ],
                    );
                }
                log::warn!(
                    "settings write failed: {context} qsid={} old={} requested={} error={}",
                    request.qsid,
                    request.old_value,
                    request.requested,
                    error_text,
                );
            }
        }

        if disconnected {
            let error = crate::i18n::tr_catalog(
                self.app_settings.language,
                "settings_write.device_disconnected",
            );
            self.settings_write_queue.fail_pending(error);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn cancel_pending_settings_writes_without_transport(&mut self) {
        if self.hid_device.is_some() || self.hid_write_task_owner_active() {
            return;
        }

        let error = crate::i18n::tr_catalog(
            self.app_settings.language,
            "settings_write.device_disconnected",
        );
        self.settings_write_queue.fail_pending(error);
    }

    pub(super) fn reset_settings_write_context(&mut self) {
        self.settings_write_generation = self.settings_write_generation.wrapping_add(1);
        self.settings_write_queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(qsid: u16, requested: u16) -> SettingsWriteRequest {
        SettingsWriteRequest {
            id: 0,
            generation: 0,
            qsid,
            width: 1,
            old_value: 1,
            requested,
            target: SettingsWriteTarget::Touchpad {
                display_label: "Setting".to_owned(),
            },
        }
    }

    fn test_app() -> EntropyApp {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        EntropyApp::new(&creation_context)
    }

    #[test]
    fn settings_write_queue_is_fifo() {
        let mut queue = SettingsWriteQueueState::default();
        let first_id = queue.enqueue(request(1, 10));
        let second_id = queue.enqueue(request(2, 20));

        assert_eq!(queue.pop_front().map(|request| request.id), Some(first_id));
        assert_eq!(queue.pop_front().map(|request| request.id), Some(second_id));
        assert!(queue.pop_front().is_none());
    }

    #[test]
    fn older_completion_keeps_newer_same_qsid_pending() {
        let mut queue = SettingsWriteQueueState::default();
        let first_id = queue.enqueue(request(122, 10));
        let second_id = queue.enqueue(request(122, 11));

        assert_eq!(queue.pending_value(122), Some(11));
        assert!(!queue.complete(first_id, 122, SettingsWriteStatus::Saved));
        assert_eq!(queue.status(122), Some(&SettingsWriteStatus::Pending));
        assert_eq!(queue.pending_value(122), Some(11));
        assert!(queue.complete(second_id, 122, SettingsWriteStatus::Saved));
        assert_eq!(queue.status(122), Some(&SettingsWriteStatus::Saved));
        assert_eq!(queue.pending_value(122), None);
    }

    #[test]
    fn failed_transport_marks_all_queued_settings() {
        let mut queue = SettingsWriteQueueState::default();
        queue.enqueue(request(121, 10));
        queue.enqueue(request(122, 11));

        queue.fail_pending("device disconnected");

        assert_eq!(
            queue.status(121),
            Some(&SettingsWriteStatus::Failed(
                "device disconnected".to_owned()
            ))
        );
        assert_eq!(
            queue.status(122),
            Some(&SettingsWriteStatus::Failed(
                "device disconnected".to_owned()
            ))
        );
        assert!(queue.is_empty());
    }

    #[test]
    fn readback_reconciliation_updates_module_and_touchpad_state() {
        let mut module_settings = ModuleSettingsState::default();
        let mut touchpad_settings = TouchpadSettingsState::default();

        SettingsWriteTarget::Module {
            group_title: "Modules".to_owned(),
            field_title: "Mode".to_owned(),
            display_label: "Mode".to_owned(),
        }
        .reconcile_readback(&mut module_settings, &mut touchpad_settings, 7, 3);
        SettingsWriteTarget::Touchpad {
            display_label: "Scroll sensitivity".to_owned(),
        }
        .reconcile_readback(&mut module_settings, &mut touchpad_settings, 122, 9);

        assert_eq!(module_settings.value(7), 3);
        assert_eq!(touchpad_settings.scroll_sens, 9);
    }

    #[test]
    fn readback_mismatch_reconciles_module_and_touchpad_state() {
        let mut app = test_app();
        let mut module_request = request(7, 3);
        module_request.target = SettingsWriteTarget::Module {
            group_title: "Modules".to_owned(),
            field_title: "Mode".to_owned(),
            display_label: "Mode".to_owned(),
        };
        app.settings_write_queue.enqueue(module_request);
        let module_request = app
            .settings_write_queue
            .pop_front()
            .expect("queued module request");
        app.finish_settings_write(SettingsWriteResult {
            hid_device: None,
            request: module_request,
            result: Err(ModuleSettingWritebackError::ReadbackMismatch {
                expected: 3,
                actual: 2,
            }),
            disconnected: false,
        });

        let mut touchpad_request = request(122, 9);
        touchpad_request.target = SettingsWriteTarget::Touchpad {
            display_label: "Scroll sensitivity".to_owned(),
        };
        app.settings_write_queue.enqueue(touchpad_request);
        let touchpad_request = app
            .settings_write_queue
            .pop_front()
            .expect("queued touchpad request");
        app.finish_settings_write(SettingsWriteResult {
            hid_device: None,
            request: touchpad_request,
            result: Err(ModuleSettingWritebackError::ReadbackMismatch {
                expected: 9,
                actual: 7,
            }),
            disconnected: false,
        });

        assert_eq!(app.module_settings.value(7), 2);
        assert_eq!(app.touchpad_settings.scroll_sens, 7);
    }

    #[test]
    fn stale_readback_mismatch_does_not_replace_newer_touchpad_value() {
        let mut app = test_app();
        let old_request = request(122, 7);
        app.settings_write_queue.enqueue(old_request);
        let old_request = app
            .settings_write_queue
            .pop_front()
            .expect("queued old request");
        app.settings_write_queue.enqueue(request(122, 9));
        app.touchpad_settings.scroll_sens = 9;

        app.finish_settings_write(SettingsWriteResult {
            hid_device: None,
            request: old_request,
            result: Err(ModuleSettingWritebackError::ReadbackMismatch {
                expected: 7,
                actual: 6,
            }),
            disconnected: false,
        });

        assert_eq!(app.touchpad_settings.scroll_sens, 9);
        assert_eq!(
            app.settings_write_queue.status(122),
            Some(&SettingsWriteStatus::Pending)
        );
    }

    #[test]
    fn handle_loss_cancels_queued_settings_and_clears_busy_state() {
        let mut app = test_app();
        app.settings_write_queue.enqueue(request(121, 10));

        app.cancel_pending_settings_writes_without_transport();

        assert!(!app.qmk_settings_write_busy());
        assert_eq!(
            app.settings_write_queue.status(121),
            Some(&SettingsWriteStatus::Failed(
                crate::i18n::tr_catalog(
                    app.app_settings.language,
                    "settings_write.device_disconnected"
                )
                .to_owned()
            ))
        );
    }

    #[test]
    fn reset_context_discards_stale_worker_result() {
        let mut app = test_app();
        let request = request(122, 10);
        app.touchpad_settings.scroll_sens = 8;
        app.reset_settings_write_context();

        app.finish_settings_write(SettingsWriteResult {
            hid_device: None,
            request,
            result: Ok(10),
            disconnected: false,
        });

        assert_eq!(app.touchpad_settings.scroll_sens, 8);
        assert!(app.hid_device.is_none());
    }

    #[test]
    fn older_readback_does_not_replace_newer_debounced_value() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        app.settings_write_queue.enqueue(request(122, 10));
        let request = app
            .settings_write_queue
            .pop_front()
            .expect("queued setting write");
        app.touchpad_settings.scroll_sens = 12;
        app.debounce_touchpad_setting_write(&ctx, "Scroll sensitivity".to_owned(), 122, 10, 12);

        app.finish_settings_write(SettingsWriteResult {
            hid_device: None,
            request,
            result: Ok(10),
            disconnected: false,
        });

        assert_eq!(app.touchpad_settings.scroll_sens, 12);
        assert_eq!(app.pending_qmk_settings_write_value(122), Some(12));
    }

    #[test]
    fn confirmed_readback_updates_pending_debounce_rollback_value() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        app.touchpad_settings.scroll_sens = 12;
        app.debounce_touchpad_setting_write(&ctx, "Scroll sensitivity".to_owned(), 122, 8, 12);
        app.settings_write_queue.enqueue(request(122, 10));
        let request = app
            .settings_write_queue
            .pop_front()
            .expect("queued setting write");

        app.finish_settings_write(SettingsWriteResult {
            hid_device: None,
            request,
            result: Ok(10),
            disconnected: false,
        });
        app.flush_pending_qmk_setting_writes();

        assert_eq!(app.touchpad_settings.scroll_sens, 10);
        assert!(!app.qmk_settings_write_pending());
    }

    #[test]
    fn readback_mismatch_updates_pending_debounce_rollback_value() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        app.touchpad_settings.scroll_sens = 12;
        app.debounce_touchpad_setting_write(&ctx, "Scroll sensitivity".to_owned(), 122, 8, 12);
        app.settings_write_queue.enqueue(request(122, 10));
        let request = app
            .settings_write_queue
            .pop_front()
            .expect("queued setting write");

        app.finish_settings_write(SettingsWriteResult {
            hid_device: None,
            request,
            result: Err(ModuleSettingWritebackError::ReadbackMismatch {
                expected: 10,
                actual: 9,
            }),
            disconnected: false,
        });
        app.flush_pending_qmk_setting_writes();

        assert_eq!(app.touchpad_settings.scroll_sens, 9);
        assert!(!app.qmk_settings_write_pending());
    }
}
