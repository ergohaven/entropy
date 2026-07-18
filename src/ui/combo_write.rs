use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ComboWritePlan {
    Clean,
    Incomplete { index: usize },
    Write { index: usize, entry: ComboEntry },
}

fn combo_entry_is_writable(entry: &ComboEntry) -> bool {
    let trigger_count = entry.keys.iter().filter(|&&keycode| keycode != 0).count();
    let is_empty = trigger_count == 0 && entry.output == 0;
    is_empty || ((2..=4).contains(&trigger_count) && entry.output != 0)
}

pub(super) fn next_combo_write(
    entries: &[ComboEntry],
    synced_entries: &[ComboEntry],
) -> ComboWritePlan {
    let mut first_incomplete = None;

    for (index, entry) in entries.iter().enumerate() {
        if synced_entries.get(index) == Some(entry) {
            continue;
        }
        if combo_entry_is_writable(entry) {
            return ComboWritePlan::Write {
                index,
                entry: entry.clone(),
            };
        }
        first_incomplete.get_or_insert(index);
    }

    first_incomplete
        .map(|index| ComboWritePlan::Incomplete { index })
        .unwrap_or(ComboWritePlan::Clean)
}

fn record_combo_write_success(
    entries: &[ComboEntry],
    synced_entries: &mut Vec<ComboEntry>,
    index: usize,
    entry: ComboEntry,
) -> bool {
    if synced_entries.len() <= index {
        synced_entries.resize(index + 1, ComboEntry::default());
    }
    synced_entries[index] = entry;
    entries != synced_entries.as_slice()
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct ComboWriteTask {
    receiver: std::sync::mpsc::Receiver<ComboWriteResult>,
    revision: u64,
}

#[cfg(not(target_arch = "wasm32"))]
struct ComboWriteResult {
    hid_device: Option<crate::hid::HidDevice>,
    index: usize,
    entry: ComboEntry,
    revision: u64,
    result: Result<(), String>,
}

impl EntropyApp {
    pub(super) fn mark_combo_dirty(&mut self) {
        self.combo_dirty = true;
        self.combo_edit_revision = self.combo_edit_revision.wrapping_add(1);
        self.combo_attempted_revision = None;
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn hid_write_task_active(&self) -> bool {
        self.layer_write_task.is_some() || self.combo_write_task.is_some()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn maybe_start_combo_write(&mut self, ctx: &egui::Context) {
        if self.hid_write_task_active()
            || self.combo_attempted_revision == Some(self.combo_edit_revision)
        {
            return;
        }

        let revision = self.combo_edit_revision;
        let Some(plan) = super::app_lifecycle::combo_write_lifecycle_plan(
            self.combo_dirty,
            self.keycode_picker.open,
            &self.combo_entries,
            &self.combo_synced_entries,
        ) else {
            return;
        };
        match plan {
            ComboWritePlan::Clean => {
                self.combo_dirty = false;
                self.combo_attempted_revision = None;
            }
            ComboWritePlan::Incomplete { index } => {
                self.combo_attempted_revision = Some(revision);
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "status_messages.combo_incomplete",
                    &[("index", &index.to_string())],
                );
            }
            ComboWritePlan::Write { index, entry } => {
                self.combo_attempted_revision = Some(revision);
                let Some(hid_device) = self.hid_device.take() else {
                    self.status_msg = crate::i18n::tr_catalog_format(
                        self.app_settings.language,
                        "status_messages.combo_write_error",
                        &[(
                            "error",
                            crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "status_messages.device_unavailable",
                            ),
                        )],
                    );
                    return;
                };

                let task_entry = entry.clone();
                let (sender, receiver) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    #[cfg(target_os = "macos")]
                    let _hid_lock = crate::hid::macos_hid_operation_lock();

                    let write_result =
                        hid_device.set_combo(index as u8, task_entry.keys, task_entry.output);
                    let disconnected = write_result
                        .as_ref()
                        .err()
                        .map(crate::hid::is_disconnect_error)
                        .unwrap_or(false);
                    let result = write_result.map_err(|error| error.to_string());
                    let hid_device = (!disconnected).then_some(hid_device);
                    let _ = sender.send(ComboWriteResult {
                        hid_device,
                        index,
                        entry: task_entry,
                        revision,
                        result,
                    });
                });
                self.combo_write_task = Some(ComboWriteTask { receiver, revision });
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "status_messages.combos_saving",
                )
                .into();
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_combo_write(&mut self, ctx: &egui::Context) {
        let result = match self.combo_write_task.as_ref() {
            Some(task) => task.receiver.try_recv(),
            None => return,
        };

        match result {
            Ok(result) => {
                self.combo_write_task = None;
                self.finish_combo_write(result);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let task = self
                    .combo_write_task
                    .take()
                    .expect("combo write task checked above");
                self.hid_device = None;
                self.combo_dirty = self.combo_entries != self.combo_synced_entries;
                self.combo_attempted_revision =
                    (self.combo_edit_revision == task.revision).then_some(task.revision);
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "status_messages.combo_write_error",
                    &[(
                        "error",
                        crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "status_messages.combo_write_task_stopped",
                        ),
                    )],
                );
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_combo_write(&mut self, result: ComboWriteResult) {
        self.hid_device = result.hid_device;
        match result.result {
            Ok(()) => {
                self.combo_dirty = record_combo_write_success(
                    &self.combo_entries,
                    &mut self.combo_synced_entries,
                    result.index,
                    result.entry,
                );
                self.combo_attempted_revision = None;
                if !self.combo_dirty {
                    self.status_msg = crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "status_messages.combos_saved",
                    )
                    .into();
                }
            }
            Err(error) => {
                self.combo_dirty = self.combo_entries != self.combo_synced_entries;
                self.combo_attempted_revision =
                    (self.combo_edit_revision == result.revision).then_some(result.revision);
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "status_messages.combo_write_error",
                    &[("error", &error)],
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combo(keys: [u16; 4], output: u16) -> ComboEntry {
        ComboEntry { keys, output }
    }

    #[test]
    fn incomplete_combo_is_not_scheduled_for_device_write() {
        let entries = [combo([0x5220, 0x0005, 0, 0], 0)];
        let synced = [ComboEntry::default()];

        assert_eq!(
            next_combo_write(&entries, &synced),
            ComboWritePlan::Incomplete { index: 0 }
        );
    }

    #[test]
    fn combo_requires_two_triggers_and_an_output() {
        assert!(combo_entry_is_writable(&ComboEntry::default()));
        assert!(!combo_entry_is_writable(&combo([0x0004, 0, 0, 0], 0x0005)));
        assert!(!combo_entry_is_writable(&combo([0x0004, 0x0005, 0, 0], 0)));
        assert!(combo_entry_is_writable(&combo(
            [0x0004, 0x0005, 0, 0],
            0x0006
        )));
    }

    #[test]
    fn clearing_combo_is_scheduled_as_valid_deletion() {
        let entries = [ComboEntry::default()];
        let synced = [combo([0x0004, 0x0005, 0, 0], 0x0006)];

        assert_eq!(
            next_combo_write(&entries, &synced),
            ComboWritePlan::Write {
                index: 0,
                entry: ComboEntry::default(),
            }
        );
    }

    #[test]
    fn only_changed_combo_slot_is_scheduled() {
        let unchanged = combo([0x0004, 0x0005, 0, 0], 0x0006);
        let changed = combo([0x0007, 0x0008, 0, 0], 0x0009);
        let previous = combo([0x0007, 0x0008, 0, 0], 0x000A);

        assert_eq!(
            next_combo_write(
                &[unchanged.clone(), changed.clone(), unchanged.clone()],
                &[unchanged.clone(), previous, unchanged]
            ),
            ComboWritePlan::Write {
                index: 1,
                entry: changed,
            }
        );
    }

    #[test]
    fn newer_edit_remains_pending_after_in_flight_write_completes() {
        let old = combo([0x0004, 0x0005, 0, 0], 0x0006);
        let requested = combo([0x0007, 0x0008, 0, 0], 0x0009);
        let latest = combo([0x000A, 0x000B, 0, 0], 0x000C);
        let mut synced = vec![old];

        let dirty =
            record_combo_write_success(std::slice::from_ref(&latest), &mut synced, 0, requested);

        assert!(dirty);
        assert_eq!(
            next_combo_write(std::slice::from_ref(&latest), &synced),
            ComboWritePlan::Write {
                index: 0,
                entry: latest,
            }
        );
    }
}
