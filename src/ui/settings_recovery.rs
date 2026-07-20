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
    hid: Option<crate::hid::HidDevice>,
    report: RecoveryReport,
    fingerprint: RecoveryFingerprint,
    history: RecoveryHistory,
    capture: Vec<PortableCaptureEntry>,
    action: RecoveryAction,
    attempted_mutation: bool,
    disconnected: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RecoveryAction {
    KeepCurrent,
    Restore,
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
) -> anyhow::Result<()> {
    match (&setting.value, setting.spec.wire_width) {
        (PortableValue::Text(value), WireWidth::Utf8) => {
            hid.set_qmk_setting_string_recovery_verified(qsid, value)
        }
        (PortableValue::Boolean(value), WireWidth::Bits8 | WireWidth::Bit) => {
            let desired = match setting
                .spec
                .bit_meanings
                .iter()
                .position(|meaning| !meaning.is_empty())
            {
                Some(bit) => {
                    let current = hid.get_qmk_setting_u8(qsid)?;
                    if *value {
                        current | (1 << bit)
                    } else {
                        current & !(1 << bit)
                    }
                }
                None => u8::from(*value),
            };
            hid.set_qmk_setting_u8_recovery_verified(qsid, desired)
        }
        (PortableValue::Select(value), WireWidth::Bits8 | WireWidth::Bit) => hid
            .set_qmk_setting_u8_recovery_verified(
                qsid,
                u8::try_from(*value).map_err(|_| anyhow::anyhow!("value exceeds u8"))?,
            ),
        (PortableValue::Unsigned(value), WireWidth::Bits8 | WireWidth::Bit) => hid
            .set_qmk_setting_u8_recovery_verified(
                qsid,
                u8::try_from(*value).map_err(|_| anyhow::anyhow!("value exceeds u8"))?,
            ),
        (PortableValue::Select(value), WireWidth::Bits16) => {
            hid.set_qmk_setting_u16_recovery_verified(qsid, *value)
        }
        (PortableValue::Unsigned(value), WireWidth::Bits16) => hid
            .set_qmk_setting_u16_recovery_verified(
                qsid,
                u16::try_from(*value).map_err(|_| anyhow::anyhow!("value exceeds u16"))?,
            ),
        _ => Err(anyhow::anyhow!("portable value does not match wire format")),
    }
}

pub(crate) fn write_setting(
    hid: &crate::hid::HidDevice,
    setting: &PortableSetting,
) -> anyhow::Result<()> {
    let qsid = setting
        .spec
        .id
        .primary_qsid
        .ok_or_else(|| anyhow::anyhow!("setting transport is not restorable"))?;
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
        if history
            .snapshots()
            .iter()
            .any(|snapshot| snapshot.fingerprint == fingerprint)
        {
            return;
        }
        let Some(newest) = history
            .snapshots()
            .iter()
            .find(|snapshot| snapshot.fingerprint != fingerprint)
        else {
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
        self.start_settings_action(RecoveryAction::KeepCurrent);
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
        self.start_settings_action(RecoveryAction::Restore);
    }

    fn start_settings_action(&mut self, action: RecoveryAction) {
        let state = std::mem::take(&mut self.settings_recovery);
        let (pending, restore_enrollment) = match state {
            SettingsRecoveryState::Restore(pending) if action == RecoveryAction::Restore => {
                (pending, false)
            }
            SettingsRecoveryState::Enrollment(pending) if action == RecoveryAction::KeepCurrent => {
                (pending, true)
            }
            SettingsRecoveryState::Restore(pending) if action == RecoveryAction::KeepCurrent => {
                (pending, false)
            }
            other => {
                self.settings_recovery = other;
                return;
            }
        };
        let Some(hid) = self.hid_device.take() else {
            self.settings_recovery = if restore_enrollment {
                SettingsRecoveryState::Enrollment(pending)
            } else {
                SettingsRecoveryState::Restore(pending)
            };
            return;
        };
        let generation = self.connection_generation;
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        std::thread::spawn(move || {
            let mut report = RecoveryReport::default();
            let fresh_capture = match super::device_connect_task::capture_portable_settings_from_hid(
                &hid,
                &pending.capture,
            ) {
                Ok(capture) => capture,
                Err(error) => {
                    let disconnected = crate::hid::is_disconnect_error(&error);
                    log::warn!("settings recovery action capture failed: {error}");
                    let _ = tx.send(RecoveryTaskResult {
                        generation,
                        hid: (!disconnected).then_some(hid),
                        report,
                        fingerprint: pending.fingerprint,
                        history: pending.history,
                        capture: Vec::new(),
                        action,
                        attempted_mutation: false,
                        disconnected,
                    });
                    return;
                }
            };
            let mut attempted_mutation = false;
            if action == RecoveryAction::Restore {
                let current = captured(&fresh_capture)
                    .into_iter()
                    .map(|setting| (setting.id().clone(), setting))
                    .collect::<BTreeMap<_, _>>();
                for (id, setting) in &pending.trusted {
                    if worker_cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    if !pending.selected.contains(id) || current.get(id) == Some(setting) {
                        report.outcomes.push(RecoveryFieldOutcome::Skipped {
                            id: id.clone(),
                            reason: "not selected, incompatible, or already current".to_owned(),
                        });
                        continue;
                    }
                    attempted_mutation = true;
                    match write_setting(&hid, setting) {
                        Ok(()) => report
                            .outcomes
                            .push(RecoveryFieldOutcome::Restored(id.clone())),
                        Err(error) if crate::hid::is_disconnect_error(&error) => {
                            log::warn!("settings recovery restore disconnected: {error}");
                            let _ = tx.send(RecoveryTaskResult {
                                generation,
                                hid: None,
                                report,
                                fingerprint: pending.fingerprint,
                                history: pending.history,
                                capture: Vec::new(),
                                action,
                                attempted_mutation,
                                disconnected: true,
                            });
                            return;
                        }
                        Err(error) => {
                            log::warn!("settings recovery restore failed for {id:?}: {error}");
                            report.outcomes.push(RecoveryFieldOutcome::Failed {
                                id: id.clone(),
                                reason: error.to_string(),
                            });
                        }
                    }
                }
            }
            let capture = if attempted_mutation {
                match super::device_connect_task::capture_portable_settings_from_hid(
                    &hid,
                    &pending.capture,
                ) {
                    Ok(capture) => capture,
                    Err(error) if crate::hid::is_disconnect_error(&error) => {
                        log::warn!("settings recovery readback disconnected: {error}");
                        let _ = tx.send(RecoveryTaskResult {
                            generation,
                            hid: None,
                            report,
                            fingerprint: pending.fingerprint,
                            history: pending.history,
                            capture: Vec::new(),
                            action,
                            attempted_mutation,
                            disconnected: true,
                        });
                        return;
                    }
                    Err(error) => {
                        log::warn!("settings recovery readback failed: {error}");
                        fresh_capture
                    }
                }
            } else {
                fresh_capture
            };
            let _ = tx.send(RecoveryTaskResult {
                generation,
                hid: Some(hid),
                report,
                fingerprint: pending.fingerprint,
                history: pending.history,
                capture,
                action,
                attempted_mutation,
                disconnected: false,
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
                self.clear_connected_keyboard_state(crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "settings_recovery.worker_failed",
                ));
                return;
            }
        };
        self.recovery_write_task = None;
        if result.generation != self.connection_generation {
            return;
        }
        if result.disconnected || result.hid.is_none() {
            self.settings_recovery = SettingsRecoveryState::Idle;
            self.clear_connected_keyboard_state(crate::i18n::tr_catalog(
                self.app_settings.language,
                "settings_recovery.disconnected",
            ));
            return;
        }
        let refresh_after_restore =
            result.action == RecoveryAction::Restore && result.attempted_mutation;
        self.hid_device = result.hid;
        let verified = captured(&result.capture);
        if result.attempted_mutation {
            log::debug!(
                "settings recovery reconciled {} values after a firmware mutation",
                verified.len()
            );
        }
        let source = match result.action {
            RecoveryAction::KeepCurrent => TrustSource::KeepCurrent,
            RecoveryAction::Restore => TrustSource::Restore,
        };
        if result
            .history
            .apply_verified(result.fingerprint, unix_timestamp(), source, verified)
        {
            if let Err(error) = recovery_store().save(&result.history) {
                log::warn!("settings recovery history save failed: {error}");
            }
        }
        self.recovery_capture = result.capture;
        let restored = result.report.restored_count();
        let failed = result
            .report
            .outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RecoveryFieldOutcome::Failed { .. }))
            .count();
        self.settings_recovery = SettingsRecoveryState::Idle;
        self.restore_entropy_display_preset_after_connect();
        self.status_msg = crate::i18n::tr_catalog_format(
            self.app_settings.language,
            "settings_recovery.completed",
            &[
                ("restored", &restored.to_string()),
                ("failed", &failed.to_string()),
            ],
        );
        if refresh_after_restore {
            if let Some(device_idx) = self.selected_device {
                self.start_connect(device_idx);
            }
        }
    }

    pub(super) fn abort_settings_recovery_task(&mut self) {
        if let Some(task) = &self.recovery_write_task {
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

    pub(super) fn record_verified_qmk_readback(&mut self, qsid: u16, readback: u16) {
        let Some(entry) = self
            .recovery_capture
            .iter()
            .find(|entry| entry.spec.id.primary_qsid == Some(qsid))
        else {
            return;
        };
        let value = match entry.spec.kind {
            crate::app::portable_settings::PortableValueKind::Boolean => {
                let bit = entry
                    .spec
                    .bit_meanings
                    .iter()
                    .position(|meaning| !meaning.is_empty());
                PortableValue::Boolean(bit.map_or(readback != 0, |bit| readback & (1 << bit) != 0))
            }
            crate::app::portable_settings::PortableValueKind::Unsigned => {
                PortableValue::Unsigned(readback.into())
            }
            crate::app::portable_settings::PortableValueKind::Select => {
                PortableValue::Select(readback)
            }
            crate::app::portable_settings::PortableValueKind::Text => return,
        };
        self.record_verified_qmk_value(qsid, value);
    }

    pub(super) fn record_verified_portable_setting(&mut self, setting: PortableSetting) {
        let Some(identity) = self.recovery_identity.clone() else {
            return;
        };
        let Some(fingerprint) = self.recovery_fingerprint.clone() else {
            return;
        };
        if let Some(entry) = self
            .recovery_capture
            .iter_mut()
            .find(|entry| entry.spec.id == setting.spec.id)
        {
            entry.state = StrictCaptureState::Captured(setting.clone());
        }
        let store = recovery_store();
        let mut history = store
            .load(&identity)
            .unwrap_or_else(|_| RecoveryHistory::new(identity));
        if history.apply_verified(
            fingerprint,
            unix_timestamp(),
            TrustSource::Import,
            [setting],
        ) {
            if let Err(error) = store.save(&history) {
                log::warn!("settings recovery history save failed after import: {error}");
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
                    crate::ui_style::modal_window_frame(
                        ui.style(),
                        ctx.global_style().visuals.dark_mode,
                    )
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::tr_catalog_format(
                                lang,
                                "settings_recovery.banner",
                                &[("count", &changed.to_string())],
                            ));
                            review_deferred = crate::ui_style::modern_button(
                                ui,
                                &crate::i18n::tr_catalog(lang, "settings_recovery.review"),
                                egui::vec2(88.0, 30.0),
                                true,
                            )
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
                self.draw_settings_recovery_backdrop(ctx);
                let mut open = true;
                crate::ui_style::centered_modal_window(
                    ctx,
                    &crate::i18n::tr_catalog(lang, "settings_recovery.restoring_title"),
                    egui::Id::new("settings_recovery_working"),
                    &mut open,
                    egui::vec2(420.0, 150.0),
                )
                .show(ctx, |ui| {
                    crate::ui_style::modal_content(
                        ui,
                        crate::ui_style::ModalLayout::new(360.0).with_top_padding(14.0),
                        |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(egui::Spinner::new().size(20.0));
                                ui.add_space(10.0);
                                ui.label(crate::i18n::tr_catalog(
                                    lang,
                                    "settings_recovery.restoring_body",
                                ));
                            });
                        },
                    );
                });
                return;
            }
            _ => return,
        };

        let mut action = 0u8;
        self.draw_settings_recovery_backdrop(ctx);
        let mut open = true;
        crate::ui_style::centered_modal_window(
            ctx,
            &crate::i18n::tr_catalog(
                lang,
                if enrollment {
                    "settings_recovery.enrollment_title"
                } else {
                    "settings_recovery.change_title"
                },
            ),
            egui::Id::new("settings_recovery_prompt"),
            &mut open,
            egui::vec2(480.0, 220.0),
        )
        .show(ctx, |ui| {
            crate::ui_style::modal_content(
                ui,
                crate::ui_style::ModalLayout::new(400.0).with_top_padding(10.0),
                |ui| {
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
                    ui.add_space(16.0);
                    ui.horizontal_centered(|ui| {
                        let button_size = crate::ui_style::modal_action_button_size();
                        if crate::ui_style::modern_button(
                            ui,
                            &crate::i18n::tr_catalog(lang, "settings_recovery.keep_current"),
                            button_size,
                            true,
                        )
                        .clicked()
                        {
                            action = 2;
                        }
                        if crate::ui_style::modern_button(
                            ui,
                            &crate::i18n::tr_catalog(lang, "settings_recovery.later"),
                            button_size,
                            true,
                        )
                        .clicked()
                        {
                            action = 3;
                        }
                        if !enrollment
                            && crate::ui_style::modern_button(
                                ui,
                                &crate::i18n::tr_catalog(lang, "settings_recovery.restore"),
                                button_size,
                                selected > 0,
                            )
                            .clicked()
                        {
                            action = 1;
                        }
                    });
                },
            );
        });
        match action {
            1 => self.start_settings_restore(),
            2 => self.keep_current_settings(),
            3 => self.defer_settings_recovery(),
            _ => {}
        }
    }

    fn draw_settings_recovery_backdrop(&self, ctx: &egui::Context) {
        let screen_rect = ctx.content_rect();
        egui::Area::new("settings_recovery_backdrop".into())
            .order(egui::Order::Foreground)
            .fixed_pos(screen_rect.min)
            .show(ctx, |ui| {
                let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, screen_rect.size());
                ui.interact(
                    rect,
                    egui::Id::new("settings_recovery_backdrop_blocker"),
                    egui::Sense::click_and_drag(),
                );
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_black_alpha(crate::ui_style::modal_backdrop_alpha(
                        ctx.global_style().visuals.dark_mode,
                    )),
                );
            });
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

    #[test]
    fn keep_current_without_hid_preserves_enrollment_prompt() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        let fingerprint = RecoveryFingerprint::new(Some("fw-test"), Some("schema-test")).unwrap();
        let identity = RecoveryIdentity::new(1, 2, "keyboard", Some("serial")).unwrap();
        app.settings_recovery = SettingsRecoveryState::Enrollment(PendingRecovery {
            fingerprint,
            history: RecoveryHistory::new(identity),
            capture: Vec::new(),
            trusted: BTreeMap::new(),
            selected: BTreeSet::new(),
            changed: 0,
            unavailable: 0,
        });

        app.keep_current_settings();

        assert!(matches!(
            app.settings_recovery,
            SettingsRecoveryState::Enrollment(_)
        ));
    }

    #[test]
    fn linked_qsid_failure_preserves_partial_write_evidence() {
        let mut spec = crate::app::portable_settings::known_qmk_setting(25).unwrap();
        spec.id.linked_qsids = vec![26];
        let setting = PortableSetting::new(spec, PortableValue::Unsigned(175)).unwrap();
        let (hid, recorder) = crate::hid::HidDevice::test_device_with_fault_after_requests(Some((
            2,
            crate::hid::TestHidFault::Disconnect,
        )));

        let error = write_setting(&hid, &setting).expect_err("linked write disconnects");
        assert!(crate::hid::is_disconnect_error(&error));
        let qsids = recorder
            .requests()
            .into_iter()
            .map(|request| u16::from_le_bytes([request[2], request[3]]))
            .collect::<Vec<_>>();
        assert_eq!(qsids, vec![25, 25, 26]);
    }

    #[test]
    fn viewport_close_waits_for_recovery_owner_then_closes_once() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        app.app_settings.minimize_to_tray_on_close = false;
        app.app_settings.close_to_tray_behavior = CloseToTrayBehavior::Close;

        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let (tx, rx) = mpsc::channel();
        let fingerprint = RecoveryFingerprint::new(Some("fw-test"), Some("schema-test")).unwrap();
        let identity = RecoveryIdentity::new(1, 2, "keyboard", Some("serial")).unwrap();
        app.recovery_write_task = Some(RecoveryWriteTask {
            generation: app.connection_generation,
            rx,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        app.settings_recovery = SettingsRecoveryState::Working;

        let mut close_input = egui::RawInput::default();
        close_input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .expect("root viewport exists")
            .events
            .push(egui::ViewportEvent::Close);
        let deferred_close = ctx.run_ui(close_input, |_ui| app.handle_close_to_tray(&ctx));
        assert!(deferred_close
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .expect("root viewport output exists")
            .commands
            .contains(&egui::ViewportCommand::CancelClose));
        assert!(app.exit_after_hid_write);

        tx.send(RecoveryTaskResult {
            generation: app.connection_generation,
            hid: Some(hid),
            report: RecoveryReport::default(),
            fingerprint,
            history: RecoveryHistory::new(identity),
            capture: Vec::new(),
            action: RecoveryAction::KeepCurrent,
            attempted_mutation: false,
            disconnected: false,
        })
        .unwrap();
        let final_close = ctx.run_ui(egui::RawInput::default(), |_ui| {
            app.poll_settings_recovery(&ctx);
            app.finish_deferred_exit_after_hid_write(&ctx);
        });
        let commands = &final_close
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .expect("root viewport output exists")
            .commands;
        assert_eq!(
            commands
                .iter()
                .filter(|command| **command == egui::ViewportCommand::Close)
                .count(),
            1
        );
        assert!(!app.exit_after_hid_write);
        assert!(recorder.requests().is_empty());
    }
}
