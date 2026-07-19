use super::*;

const QMK_SETTINGS_WRITE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(180);

#[derive(Clone, Debug)]
struct QmkSettingWriteRequest {
    qsid: u16,
    old_value: u16,
    requested: u16,
    due_at: std::time::Instant,
    display_label: String,
}

#[derive(Default)]
pub(super) struct QmkSettingsWriteQueue {
    pending: std::collections::BTreeMap<u16, QmkSettingWriteRequest>,
}

impl QmkSettingsWriteQueue {
    fn enqueue(&mut self, mut request: QmkSettingWriteRequest) -> bool {
        if let Some(previous) = self.pending.get(&request.qsid) {
            // Keep the confirmed pre-burst value so returning to it cancels the write.
            request.old_value = previous.old_value;
        }
        if request.requested == request.old_value {
            self.pending.remove(&request.qsid);
            return false;
        }
        self.pending.insert(request.qsid, request);
        true
    }

    fn take_due(&mut self, now: std::time::Instant) -> Vec<QmkSettingWriteRequest> {
        let due_qsids = self
            .pending
            .iter()
            .filter_map(|(qsid, request)| (request.due_at <= now).then_some(*qsid))
            .collect::<Vec<_>>();
        due_qsids
            .into_iter()
            .filter_map(|qsid| self.pending.remove(&qsid))
            .collect()
    }

    fn take_all(&mut self) -> Vec<QmkSettingWriteRequest> {
        std::mem::take(&mut self.pending).into_values().collect()
    }

    fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    fn pending_value(&self, qsid: u16) -> Option<u16> {
        self.pending.get(&qsid).map(|request| request.requested)
    }

    pub(super) fn clear(&mut self) {
        self.pending.clear();
    }
}

impl EntropyApp {
    pub(super) fn qmk_settings_write_pending(&self) -> bool {
        !self.qmk_settings_write_queue.is_empty()
    }

    pub(super) fn pending_qmk_settings_write_value(&self, qsid: u16) -> Option<u16> {
        self.qmk_settings_write_queue.pending_value(qsid)
    }

    pub(super) fn debounce_touchpad_setting_write(
        &mut self,
        ctx: &egui::Context,
        display_label: String,
        qsid: u16,
        old_value: u16,
        requested: u16,
    ) {
        let request = QmkSettingWriteRequest {
            qsid,
            old_value,
            requested,
            due_at: std::time::Instant::now() + QMK_SETTINGS_WRITE_DEBOUNCE,
            display_label,
        };
        if self.qmk_settings_write_queue.enqueue(request) {
            ctx.request_repaint_after(QMK_SETTINGS_WRITE_DEBOUNCE);
        }
    }

    pub(super) fn flush_due_qmk_setting_writes(&mut self) {
        let requests = self
            .qmk_settings_write_queue
            .take_due(std::time::Instant::now());
        self.enqueue_debounced_qmk_setting_writes(requests);
    }

    pub(super) fn flush_pending_qmk_setting_writes(&mut self) {
        let requests = self.qmk_settings_write_queue.take_all();
        self.enqueue_debounced_qmk_setting_writes(requests);
    }

    fn enqueue_debounced_qmk_setting_writes(&mut self, requests: Vec<QmkSettingWriteRequest>) {
        for request in requests {
            if !self.qmk_setting_transport_available() {
                self.set_touchpad_numeric_value(request.qsid, request.old_value);
                let error = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "settings_write.device_not_connected",
                );
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "settings_write.failed_status",
                    &[("setting", &request.display_label), ("error", error)],
                );
                continue;
            }
            // The worker queue owns the HID handle and retains this request while a
            // layer write is in flight. It also provides verified readback.
            self.queue_touchpad_setting_write(
                request.display_label,
                request.qsid,
                1,
                request.old_value,
                request.requested,
            );
        }
    }

    pub(super) fn cancel_pending_qmk_setting_writes(&mut self) {
        for request in self.qmk_settings_write_queue.take_all() {
            self.set_touchpad_numeric_value(request.qsid, request.old_value);
        }
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

    #[cfg(not(target_arch = "wasm32"))]
    fn drain_hid_writes(app: &mut EntropyApp, ctx: &egui::Context) {
        for _ in 0..200 {
            app.poll_layer_write(ctx);
            app.poll_combo_write(ctx);
            app.poll_settings_write(ctx);
            if !app.hid_write_task_active() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("HID write tasks did not drain");
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn qsid_request_count(recorder: &crate::hid::TestHidRecorder, qsid: u16) -> usize {
        recorder
            .requests()
            .iter()
            .filter(|request| request[2] == qsid as u8 && request[3] == (qsid >> 8) as u8)
            .count()
    }

    fn request(
        qsid: u16,
        old_value: u16,
        requested: u16,
        due_at: std::time::Instant,
    ) -> QmkSettingWriteRequest {
        QmkSettingWriteRequest {
            qsid,
            old_value,
            requested,
            due_at,
            display_label: "Scroll sensitivity".to_owned(),
        }
    }

    #[test]
    fn repeated_qsid_write_keeps_only_latest_value() {
        let now = std::time::Instant::now();
        let mut queue = QmkSettingsWriteQueue::default();
        queue.enqueue(request(122, 8, 9, now + QMK_SETTINGS_WRITE_DEBOUNCE));
        queue.enqueue(request(
            122,
            9,
            12,
            now + QMK_SETTINGS_WRITE_DEBOUNCE + std::time::Duration::from_millis(60),
        ));

        assert!(queue.take_due(now + QMK_SETTINGS_WRITE_DEBOUNCE).is_empty());
        let writes = queue
            .take_due(now + QMK_SETTINGS_WRITE_DEBOUNCE + std::time::Duration::from_millis(60));
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].old_value, 8);
        assert_eq!(writes[0].requested, 12);
    }

    #[test]
    fn returning_to_original_value_cancels_pending_write() {
        let now = std::time::Instant::now();
        let mut queue = QmkSettingsWriteQueue::default();
        assert!(queue.enqueue(request(123, 10, 12, now)));
        assert!(!queue.enqueue(request(123, 12, 10, now)));
        assert!(queue.is_empty());
    }

    #[test]
    fn cancellation_returns_all_pending_requests() {
        let now = std::time::Instant::now();
        let mut queue = QmkSettingsWriteQueue::default();
        queue.enqueue(request(121, 8, 9, now));
        queue.enqueue(request(122, 10, 12, now));

        let requests = queue.take_all();

        assert_eq!(requests.len(), 2);
        assert!(queue.is_empty());
    }

    #[test]
    fn cancellation_restores_confirmed_touchpad_value() {
        let mut app = test_app();
        app.touchpad_settings.scroll_sens = 12;
        app.qmk_settings_write_queue
            .enqueue(request(122, 8, 12, std::time::Instant::now()));

        app.cancel_pending_qmk_setting_writes();

        assert_eq!(app.touchpad_settings.scroll_sens, 8);
        assert!(!app.qmk_settings_write_pending());
    }

    #[test]
    fn device_switch_cancels_debounced_write_without_transport() {
        let mut app = test_app();
        app.touchpad_settings.scroll_sens = 12;
        app.qmk_settings_write_queue
            .enqueue(request(122, 8, 12, std::time::Instant::now()));

        app.start_connect(usize::MAX);

        assert_eq!(app.touchpad_settings.scroll_sens, 8);
        assert!(!app.qmk_settings_write_pending());
    }

    #[test]
    fn disconnect_then_reconnect_cancels_old_debounce_and_writes_new_value() {
        let mut app = test_app();
        app.touchpad_settings.scroll_sens = 12;
        app.qmk_settings_write_queue
            .enqueue(request(122, 8, 12, std::time::Instant::now()));

        app.clear_connected_keyboard_state("device disconnected");

        assert!(!app.qmk_settings_write_pending());
        assert_eq!(app.touchpad_settings.scroll_sens, 8);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let ctx = egui::Context::default();
            let (hid_device, recorder) = crate::hid::HidDevice::test_device();
            app.hid_device = Some(hid_device);
            app.touchpad_settings.scroll_sens = 14;
            app.qmk_settings_write_queue
                .enqueue(request(122, 8, 14, std::time::Instant::now()));
            app.flush_pending_qmk_setting_writes();
            drain_hid_writes(&mut app, &ctx);

            assert_eq!(app.touchpad_settings.scroll_sens, 14);
            assert_eq!(qsid_request_count(&recorder, 122), 2);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn debounced_write_waits_for_combo_then_writes_only_newest_value() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        let (hid_device, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid_device);
        app.combo_entries = vec![ComboEntry {
            keys: [0x0004, 0x0005, 0, 0],
            output: 0x0006,
        }];
        app.combo_synced_entries = vec![ComboEntry::default()];
        app.mark_combo_dirty();
        app.maybe_start_combo_write(&ctx);
        assert!(app.combo_write_task.is_some());

        app.qmk_settings_write_queue
            .enqueue(request(122, 8, 9, std::time::Instant::now()));
        app.qmk_settings_write_queue
            .enqueue(request(122, 9, 12, std::time::Instant::now()));

        app.flush_due_qmk_setting_writes();

        assert_eq!(app.pending_settings_write_value(122), Some(12));
        assert!(!app.qmk_settings_write_pending());
        drain_hid_writes(&mut app, &ctx);

        assert_eq!(
            app.settings_write_queue.status(122),
            Some(&SettingsWriteStatus::Saved)
        );
        assert_eq!(app.touchpad_settings.scroll_sens, 12);
        assert_eq!(qsid_request_count(&recorder, 122), 2);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn device_switch_waits_for_real_debounced_settings_write() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        let (hid_device, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid_device);
        app.qmk_settings_write_queue
            .enqueue(request(122, 8, 12, std::time::Instant::now()));

        app.start_connect(usize::MAX);
        assert_eq!(app.pending_device_connect, Some(usize::MAX));
        assert!(app.settings_write_task.is_some());

        drain_hid_writes(&mut app, &ctx);

        assert!(app.pending_device_connect.is_none());
        assert_eq!(app.status_msg, "Device not found");
        assert_eq!(qsid_request_count(&recorder, 122), 2);
    }
}
