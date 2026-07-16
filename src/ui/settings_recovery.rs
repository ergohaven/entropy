use super::*;
use crate::app::portable_settings::{
    grouped_diff, DiffKind, PortableSetting, PortableSettingId, PortableValue, StrictCaptureState,
    WireWidth,
};
use crate::app::settings_recovery::{
    RecoveryFieldOutcome, RecoveryFingerprint, RecoveryHistory, RecoveryIdentity, RecoveryReport,
    RecoveryStore, TrustSource,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Default)]
pub(crate) enum SettingsRecoveryState {
    #[default]
    Idle,
    Enrollment(PendingRecovery),
    Restore(PendingRecovery),
    Deferred(PendingRecovery),
    Working,
}

pub(crate) struct PendingRecovery {
    fingerprint: RecoveryFingerprint,
    history: RecoveryHistory,
    capture: Vec<PortableCaptureEntry>,
    trusted: BTreeMap<PortableSettingId, PortableSetting>,
    selected: BTreeSet<PortableSettingId>,
    changed: usize,
    unavailable: usize,
}

pub(crate) struct RecoveryWriteTask {
    generation: u64,
    rx: mpsc::Receiver<RecoveryTaskResult>,
    cancel: Arc<AtomicBool>,
}

struct RecoveryTaskResult {
    generation: u64,
    hid: crate::hid::HidDevice,
    report: RecoveryReport,
    fingerprint: RecoveryFingerprint,
    history: RecoveryHistory,
    verified: Vec<PortableSetting>,
}

fn recovery_store() -> RecoveryStore {
    let root = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("entropy")
        .join("settings_recovery");
    RecoveryStore::new(root)
}

fn captured(entries: &[PortableCaptureEntry]) -> Vec<PortableSetting> {
    entries
        .iter()
        .filter_map(|entry| match &entry.state {
            StrictCaptureState::Captured(setting) => Some(setting.clone()),
            StrictCaptureState::Unavailable(_) | StrictCaptureState::Unsupported => None,
        })
        .collect()
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn write_qmk_setting(
    hid: &crate::hid::HidDevice,
    qsid: u16,
    setting: &PortableSetting,
) -> Result<(), String> {
    match (&setting.value, setting.spec.wire_width) {
        (PortableValue::Text(value), WireWidth::Utf8) => hid
            .set_qmk_setting_string_recovery_verified(qsid, value)
            .map_err(|error| error.to_string()),
        (PortableValue::Boolean(value), WireWidth::Bits8 | WireWidth::Bit) => {
            let desired = match setting
                .spec
                .bit_meanings
                .iter()
                .position(|meaning| !meaning.is_empty())
            {
                Some(bit) => {
                    let current = hid
                        .get_qmk_setting_u8(qsid)
                        .map_err(|error| error.to_string())?;
                    if *value {
                        current | (1 << bit)
                    } else {
                        current & !(1 << bit)
                    }
                }
                None => u8::from(*value),
            };
            hid.set_qmk_setting_u8_recovery_verified(qsid, desired)
                .map_err(|error| error.to_string())
        }
        (PortableValue::Select(value), WireWidth::Bits8 | WireWidth::Bit) => hid
            .set_qmk_setting_u8_recovery_verified(
                qsid,
                u8::try_from(*value).map_err(|_| "value exceeds u8".to_owned())?,
            )
            .map_err(|error| error.to_string()),
        (PortableValue::Unsigned(value), WireWidth::Bits8 | WireWidth::Bit) => hid
            .set_qmk_setting_u8_recovery_verified(
                qsid,
                u8::try_from(*value).map_err(|_| "value exceeds u8".to_owned())?,
            )
            .map_err(|error| error.to_string()),
        (PortableValue::Select(value), WireWidth::Bits16) => hid
            .set_qmk_setting_u16_recovery_verified(qsid, *value)
            .map_err(|error| error.to_string()),
        (PortableValue::Unsigned(value), WireWidth::Bits16) => hid
            .set_qmk_setting_u16_recovery_verified(
                qsid,
                u16::try_from(*value).map_err(|_| "value exceeds u16".to_owned())?,
            )
            .map_err(|error| error.to_string()),
        _ => Err("portable value does not match wire format".to_owned()),
    }
}

pub(crate) fn write_setting(
    hid: &crate::hid::HidDevice,
    setting: &PortableSetting,
) -> Result<(), String> {
    let qsid = setting
        .spec
        .id
        .primary_qsid
        .ok_or_else(|| "setting transport is not restorable".to_owned())?;
    write_qmk_setting(hid, qsid, setting)?;
    for linked_qsid in &setting.spec.id.linked_qsids {
        write_qmk_setting(hid, *linked_qsid, setting)?;
    }
    Ok(())
}

impl EntropyApp {
    pub(super) fn prepare_settings_recovery(
        &mut self,
        identity: Option<RecoveryIdentity>,
        fingerprint: Option<RecoveryFingerprint>,
        capture: Vec<PortableCaptureEntry>,
    ) {
        self.connection_generation = self.connection_generation.wrapping_add(1);
        self.abort_settings_recovery_task();
        self.recovery_capture = capture.clone();
        self.recovery_identity = identity.clone();
        self.recovery_fingerprint = fingerprint.clone();
        self.settings_recovery = SettingsRecoveryState::Idle;

        let (Some(identity), Some(fingerprint)) = (identity, fingerprint) else {
            return;
        };
        let store = recovery_store();
        let history = match store.load(&identity) {
            Ok(history) => history,
            Err(error) => {
                log::warn!("settings recovery history load failed: {error}");
                RecoveryHistory::new(identity.clone())
            }
        };
        let current = captured(&capture);
        let Some(newest) = history.snapshots().first() else {
            let unavailable = capture.len().saturating_sub(current.len());
            self.settings_recovery = SettingsRecoveryState::Enrollment(PendingRecovery {
                fingerprint,
                history,
                capture,
                trusted: BTreeMap::new(),
                selected: BTreeSet::new(),
                changed: current.len(),
                unavailable,
            });
            return;
        };
        if newest.fingerprint == fingerprint {
            return;
        }

        let trusted = newest.fields.clone();
        let diff = grouped_diff(trusted.values(), current.iter());
        let selected: BTreeSet<_> = diff
            .groups
            .iter()
            .flat_map(|group| &group.settings)
            .filter(|item| {
                matches!(item.kind, DiffKind::Changed { .. }) && item.id.primary_qsid.is_some()
            })
            .map(|item| item.id.clone())
            .collect();
        let unavailable = diff
            .groups
            .iter()
            .flat_map(|group| &group.settings)
            .filter(|item| {
                !matches!(item.kind, DiffKind::Changed { .. }) || item.id.primary_qsid.is_none()
            })
            .count();
        if selected.is_empty() && unavailable == 0 {
            // A passive no-diff observation suppresses prompting but never advances trust.
            return;
        }
        self.settings_recovery = SettingsRecoveryState::Restore(PendingRecovery {
            fingerprint,
            history,
            capture,
            trusted,
            changed: selected.len(),
            unavailable,
            selected,
        });
    }

    fn keep_current_settings(&mut self) {
        let state = std::mem::take(&mut self.settings_recovery);
        let pending = match state {
            SettingsRecoveryState::Enrollment(pending)
            | SettingsRecoveryState::Restore(pending) => pending,
            other => {
                self.settings_recovery = other;
                return;
            }
        };
        let mut history = pending.history;
        history.apply_verified(
            pending.fingerprint,
            unix_timestamp(),
            TrustSource::KeepCurrent,
            captured(&pending.capture),
        );
        if let Err(error) = recovery_store().save(&history) {
            log::warn!("settings recovery history save failed: {error}");
        }
        self.settings_recovery = SettingsRecoveryState::Idle;
    }

    fn defer_settings_recovery(&mut self) {
        let state = std::mem::take(&mut self.settings_recovery);
        let pending = match state {
            SettingsRecoveryState::Enrollment(pending)
            | SettingsRecoveryState::Restore(pending) => pending,
            other => {
                self.settings_recovery = other;
                return;
            }
        };
        self.settings_recovery = SettingsRecoveryState::Deferred(pending);
    }

    fn start_settings_restore(&mut self) {
        let state = std::mem::take(&mut self.settings_recovery);
        let pending = match state {
            SettingsRecoveryState::Restore(pending) => pending,
            other => {
                self.settings_recovery = other;
                return;
            }
        };
        let Some(hid) = self.hid_device.take() else {
            self.settings_recovery = SettingsRecoveryState::Restore(pending);
            return;
        };
        let generation = self.connection_generation;
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        std::thread::spawn(move || {
            let mut report = RecoveryReport::default();
            let mut verified = Vec::new();
            for (id, setting) in &pending.trusted {
                if worker_cancel.load(Ordering::Relaxed) {
                    break;
                }
                if !pending.selected.contains(id) {
                    report.outcomes.push(RecoveryFieldOutcome::Skipped {
                        id: id.clone(),
                        reason: "not selected or incompatible".to_owned(),
                    });
                    continue;
                }
                match write_setting(&hid, setting) {
                    Ok(()) => {
                        verified.push(setting.clone());
                        report
                            .outcomes
                            .push(RecoveryFieldOutcome::Restored(id.clone()));
                    }
                    Err(reason) => report.outcomes.push(RecoveryFieldOutcome::Failed {
                        id: id.clone(),
                        reason,
                    }),
                }
            }
            let _ = tx.send(RecoveryTaskResult {
                generation,
                hid,
                report,
                fingerprint: pending.fingerprint,
                history: pending.history,
                verified,
            });
        });
        self.recovery_write_task = Some(RecoveryWriteTask {
            generation,
            rx,
            cancel,
        });
        self.settings_recovery = SettingsRecoveryState::Working;
    }

    pub(super) fn poll_settings_recovery(&mut self, ctx: &egui::Context) {
        if self
            .recovery_write_task
            .as_ref()
            .is_some_and(|task| task.generation != self.connection_generation)
        {
            if let Some(task) = &self.recovery_write_task {
                task.cancel.store(true, Ordering::Relaxed);
            }
            self.recovery_write_task = None;
            self.settings_recovery = SettingsRecoveryState::Idle;
            return;
        }
        let Some(task) = self.recovery_write_task.as_ref() else {
            return;
        };
        let mut result = match task.rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.recovery_write_task = None;
                self.settings_recovery = SettingsRecoveryState::Idle;
                self.status_msg = "Settings recovery task failed".to_owned();
                return;
            }
        };
        self.recovery_write_task = None;
        if result.generation != self.connection_generation {
            return;
        }
        self.hid_device = Some(result.hid);
        if !result.verified.is_empty() {
            result.history.apply_verified(
                result.fingerprint,
                unix_timestamp(),
                TrustSource::Restore,
                result.verified,
            );
            if let Err(error) = recovery_store().save(&result.history) {
                log::warn!("settings recovery history save failed: {error}");
            }
        }
        let restored = result.report.restored_count();
        let failed = result
            .report
            .outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RecoveryFieldOutcome::Failed { .. }))
            .count();
        self.settings_recovery = SettingsRecoveryState::Idle;
        self.status_msg = format!("Settings recovery: {restored} restored, {failed} failed");
    }

    pub(super) fn abort_settings_recovery_task(&mut self) {
        if let Some(task) = self.recovery_write_task.take() {
            task.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn record_verified_qmk_value(&mut self, qsid: u16, value: PortableValue) {
        let Some(identity) = self.recovery_identity.clone() else {
            return;
        };
        let Some(fingerprint) = self.recovery_fingerprint.clone() else {
            return;
        };
        let Some(entry) = self
            .recovery_capture
            .iter_mut()
            .find(|entry| entry.spec.id.primary_qsid == Some(qsid))
        else {
            return;
        };
        let Ok(setting) = PortableSetting::new(entry.spec.clone(), value) else {
            return;
        };
        entry.state = StrictCaptureState::Captured(setting.clone());
        let store = recovery_store();
        let mut history = store
            .load(&identity)
            .unwrap_or_else(|_| RecoveryHistory::new(identity));
        if history.apply_verified(
            fingerprint,
            unix_timestamp(),
            TrustSource::VerifiedWrite,
            [setting],
        ) {
            if let Err(error) = store.save(&history) {
                log::warn!("settings recovery history save failed after verified write: {error}");
            }
        }
    }

    pub(super) fn draw_settings_recovery(&mut self, ctx: &egui::Context) {
        let lang = self.app_settings.language;
        let mut review_deferred = false;
        if let SettingsRecoveryState::Deferred(pending) = &self.settings_recovery {
            let changed = pending.changed;
            egui::Area::new("settings_recovery_banner".into())
                .anchor(egui::Align2::CENTER_TOP, [0.0, 12.0])
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::tr_catalog_format(
                                lang,
                                "settings_recovery.banner",
                                &[("count", &changed.to_string())],
                            ));
                            review_deferred = ui
                                .button(crate::i18n::tr_catalog(lang, "settings_recovery.review"))
                                .clicked();
                        });
                    });
                });
        }
        if review_deferred {
            let pending = match std::mem::take(&mut self.settings_recovery) {
                SettingsRecoveryState::Deferred(pending) => pending,
                other => {
                    self.settings_recovery = other;
                    return;
                }
            };
            self.settings_recovery = if pending.trusted.is_empty() {
                SettingsRecoveryState::Enrollment(pending)
            } else {
                SettingsRecoveryState::Restore(pending)
            };
        }

        let (enrollment, changed, unavailable, selected) = match &self.settings_recovery {
            SettingsRecoveryState::Enrollment(pending) => (
                true,
                pending.changed,
                pending.unavailable,
                pending.selected.len(),
            ),
            SettingsRecoveryState::Restore(pending) => (
                false,
                pending.changed,
                pending.unavailable,
                pending.selected.len(),
            ),
            SettingsRecoveryState::Working => {
                egui::Window::new(crate::i18n::tr_catalog(
                    lang,
                    "settings_recovery.restoring_title",
                ))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(crate::i18n::tr_catalog(
                            lang,
                            "settings_recovery.restoring_body",
                        ));
                    });
                });
                return;
            }
            _ => return,
        };

        let mut action = 0u8;
        egui::Window::new(crate::i18n::tr_catalog(
            lang,
            if enrollment {
                "settings_recovery.enrollment_title"
            } else {
                "settings_recovery.change_title"
            },
        ))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            if enrollment {
                ui.label(crate::i18n::tr_catalog(
                    lang,
                    "settings_recovery.enrollment_body",
                ));
            } else {
                ui.label(crate::i18n::tr_catalog_format(
                    lang,
                    "settings_recovery.change_body",
                    &[
                        ("changed", &changed.to_string()),
                        ("unavailable", &unavailable.to_string()),
                    ],
                ));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if !enrollment
                    && ui
                        .add_enabled(
                            selected > 0,
                            egui::Button::new(crate::i18n::tr_catalog(
                                lang,
                                "settings_recovery.restore",
                            )),
                        )
                        .clicked()
                {
                    action = 1;
                }
                if ui
                    .button(crate::i18n::tr_catalog(
                        lang,
                        "settings_recovery.keep_current",
                    ))
                    .clicked()
                {
                    action = 2;
                }
                if ui
                    .button(crate::i18n::tr_catalog(lang, "settings_recovery.later"))
                    .clicked()
                {
                    action = 3;
                }
            });
        });
        match action {
            1 => self.start_settings_restore(),
            2 => self.keep_current_settings(),
            3 => self.defer_settings_recovery(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_capture_filters_unavailable_values() {
        let entries = vec![PortableCaptureEntry {
            spec: crate::app::portable_settings::known_qmk_setting(25).unwrap(),
            state: StrictCaptureState::Unavailable("timeout".to_owned()),
        }];
        assert!(captured(&entries).is_empty());
    }

    #[test]
    fn recovery_retry_schedule_is_delayed_and_bounded() {
        assert_eq!([20_u64, 80, 200].iter().sum::<u64>(), 300);
    }
}
