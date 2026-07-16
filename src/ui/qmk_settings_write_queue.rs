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

    pub(super) fn clear(&mut self) {
        self.pending.clear();
    }
}

impl EntropyApp {
    pub(super) fn qmk_settings_write_pending(&self) -> bool {
        !self.qmk_settings_write_queue.is_empty()
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
        for request in requests {
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
        let writes = queue.take_due(now + QMK_SETTINGS_WRITE_DEBOUNCE + std::time::Duration::from_millis(60));
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
}
