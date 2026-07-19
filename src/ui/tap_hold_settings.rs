use super::*;

const TAP_HOLD_TERM_MAX_MS: u32 = 1000;
const TAP_HOLD_DELAY_MAX_MS: u32 = 255;
const TAPPING_TOGGLE_MAX_TAPS: u32 = 10;
const ONE_SHOT_TAP_TOGGLE_MAX_TAPS: u32 = 10;
const ONE_SHOT_TIMEOUT_MAX_MS: u32 = 10_000;
const TAP_HOLD_WRITE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(250);

fn update_pending_tap_hold_numeric_write(
    pending: &mut std::collections::BTreeMap<u16, u16>,
    qsid: u16,
    current: u16,
    edited: u16,
) -> bool {
    if edited == current {
        pending.remove(&qsid);
        false
    } else {
        pending.insert(qsid, edited);
        true
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsRowKind {
    TapHold,
    OneShot,
}

enum SettingsRow {
    Section(&'static str),
    Setting {
        kind: SettingsRowKind,
        qsid: u16,
        label: &'static str,
        tooltip: &'static str,
        is_bool: bool,
        max: u32,
    },
}

fn rmk_hrm_profile_notice(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => {
            "RMK HRM при перекатах зависит от Morse/Tap-Hold profiles в firmware и карты рук матрицы. Entropy показывает только runtime settings, которые прошивка явно открывает; profiles и карту рук нужно настраивать в RMK firmware."
        }
        crate::i18n::Language::English => {
            "RMK HRM rolling behavior depends on firmware Morse/Tap-Hold profiles and the matrix hand map. Entropy shows only runtime settings exposed by this firmware; profiles and hand mapping must be configured in RMK firmware."
        }
    }
}

fn push_tap_hold_row(
    rows: &mut Vec<SettingsRow>,
    settings: &TapHoldSettingsState,
    row: SettingsRow,
) {
    let SettingsRow::Setting { qsid, .. } = row else {
        rows.push(row);
        return;
    };
    if settings.supports_qsid(qsid) {
        rows.push(row);
    }
}

fn tap_hold_one_shot_rows(
    lang: crate::i18n::Language,
    tap_hold_settings: &TapHoldSettingsState,
    one_shot_settings: &OneShotSettingsState,
) -> Vec<SettingsRow> {
    let mut rows: Vec<SettingsRow> = Vec::with_capacity(13);
    if tap_hold_settings.supported {
        push_tap_hold_row(
            &mut rows,
            tap_hold_settings,
            SettingsRow::Setting {
                kind: SettingsRowKind::TapHold,
                qsid: 7,
                label: crate::i18n::tr_catalog(lang, "tap_hold_settings.tapping_term_label"),
                tooltip: crate::i18n::tr_catalog(
                    lang,
                    "tap_hold_settings.global_tap_vs_hold_decision_window_for_dual_role_keys",
                ),
                is_bool: false,
                max: TAP_HOLD_TERM_MAX_MS,
            },
        );
        push_tap_hold_row(
            &mut rows,
            tap_hold_settings,
            SettingsRow::Setting {
                kind: SettingsRowKind::TapHold,
                qsid: 22,
                label: crate::i18n::tr_catalog(lang, "tap_hold_settings.permissive_hold"),
                tooltip: crate::i18n::tr_catalog(
                    lang,
                    "tap_hold_settings.nested_taps_choose_hold_for_mod_tap_and_layer_tap_keys",
                ),
                is_bool: true,
                max: 1,
            },
        );
        push_tap_hold_row(
            &mut rows,
            tap_hold_settings,
            SettingsRow::Setting {
                kind: SettingsRowKind::TapHold,
                qsid: 23,
                label: crate::i18n::tr_catalog(lang, "tap_hold_settings.hold_on_other_key"),
                tooltip: crate::i18n::tr_catalog(
                    lang,
                    "tap_hold_settings.pressing_another_key_immediately_chooses_hold_for_dual_role_keys",
                ),
                is_bool: true,
                max: 1,
            },
        );
        push_tap_hold_row(
            &mut rows,
            tap_hold_settings,
            SettingsRow::Setting {
                kind: SettingsRowKind::TapHold,
                qsid: 24,
                label: crate::i18n::tr_catalog(lang, "tap_hold_settings.retro_tapping"),
                tooltip: crate::i18n::tr_catalog(
                    lang,
                    "tap_hold_settings.a_held_and_released_alone_dual_role_key_still_sends_its_tap_action",
                ),
                is_bool: true,
                max: 1,
            },
        );
        push_tap_hold_row(
            &mut rows,
            tap_hold_settings,
            SettingsRow::Setting {
                kind: SettingsRowKind::TapHold,
                qsid: 26,
                label: crate::i18n::tr_catalog(lang, "tap_hold_settings.chordal_hold"),
                tooltip: crate::i18n::tr_catalog(
                    lang,
                    "tap_hold_settings.same_hand_chords_prefer_tap_to_reduce_home_row_mod_accidents",
                ),
                is_bool: true,
                max: 1,
            },
        );
        push_tap_hold_row(
            &mut rows,
            tap_hold_settings,
            SettingsRow::Setting {
                kind: SettingsRowKind::TapHold,
                qsid: 25,
                label: crate::i18n::tr_catalog(lang, "tap_hold_settings.quick_tap_term"),
                tooltip: crate::i18n::tr_catalog(
                    lang,
                    "tap_hold_settings.tap_then_hold_repeat_window_for_dual_role_key_tap_actions",
                ),
                is_bool: false,
                max: TAP_HOLD_TERM_MAX_MS,
            },
        );
        push_tap_hold_row(
            &mut rows,
            tap_hold_settings,
            SettingsRow::Setting {
                kind: SettingsRowKind::TapHold,
                qsid: 18,
                label: crate::i18n::tr_catalog(lang, "tap_hold_settings.tap_code_delay"),
                tooltip: crate::i18n::tr_catalog(
                    lang,
                    "tap_hold_settings.delay_between_register_and_unregister_in_tap_code",
                ),
                is_bool: false,
                max: TAP_HOLD_DELAY_MAX_MS,
            },
        );
        push_tap_hold_row(
            &mut rows,
            tap_hold_settings,
            SettingsRow::Setting {
                kind: SettingsRowKind::TapHold,
                qsid: 19,
                label: crate::i18n::tr_catalog(lang, "tap_hold_settings.tap_hold_caps_delay"),
                tooltip: crate::i18n::tr_catalog(
                    lang,
                    "tap_hold_settings.extra_delay_for_lt_mt_keys_whose_tap_action_is_caps_lock",
                ),
                is_bool: false,
                max: TAP_HOLD_DELAY_MAX_MS,
            },
        );
        push_tap_hold_row(
            &mut rows,
            tap_hold_settings,
            SettingsRow::Setting {
                kind: SettingsRowKind::TapHold,
                qsid: 20,
                label: crate::i18n::tr_catalog(lang, "tap_hold_settings.tapping_toggle"),
                tooltip: crate::i18n::tr_catalog(
                    lang,
                    "tap_hold_settings.number_of_taps_needed_for_tt_layer_toggle",
                ),
                is_bool: false,
                max: TAPPING_TOGGLE_MAX_TAPS,
            },
        );
        push_tap_hold_row(
            &mut rows,
            tap_hold_settings,
            SettingsRow::Setting {
                kind: SettingsRowKind::TapHold,
                qsid: 27,
                label: crate::i18n::tr_catalog(lang, "tap_hold_settings.flow_tap"),
                tooltip: crate::i18n::tr_catalog(
                    lang,
                    "tap_hold_settings.fast_typing_timeout_that_forces_mt_lt_keys_to_tap",
                ),
                is_bool: false,
                max: TAP_HOLD_TERM_MAX_MS,
            },
        );
    }
    if one_shot_settings.supported {
        if tap_hold_settings.supported {
            rows.push(SettingsRow::Section(crate::i18n::tr_catalog(
                lang,
                "tap_hold_settings.one_shot_keys",
            )));
        }
        rows.extend([
            SettingsRow::Setting {
                kind: SettingsRowKind::OneShot,
                qsid: 5,
                label: crate::i18n::tr_catalog(lang, "tap_hold_settings.one_shot_tap_toggle"),
                tooltip: crate::i18n::tr_catalog(
                    lang,
                    "tap_hold_settings.tap_this_many_times_to_keep_a_one_shot_key_held_until_tapped_again",
                ),
                is_bool: false,
                max: ONE_SHOT_TAP_TOGGLE_MAX_TAPS,
            },
            SettingsRow::Setting {
                kind: SettingsRowKind::OneShot,
                qsid: 6,
                label: crate::i18n::tr_catalog(lang, "tap_hold_settings.one_shot_timeout"),
                tooltip: crate::i18n::tr_catalog(
                    lang,
                    "tap_hold_settings.how_long_one_shot_state_waits_before_it_is_released",
                ),
                is_bool: false,
                max: ONE_SHOT_TIMEOUT_MAX_MS,
            },
        ]);
    }
    rows
}

impl EntropyApp {
    pub(super) fn draw_tap_hold_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let hid_ready = {
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.hid_device.is_some()
            }
            #[cfg(target_arch = "wasm32")]
            {
                false
            }
        };

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::TapHoldOneShotTitle))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr(
                        lang,
                        crate::i18n::Key::TapHoldOneShotDescription,
                    ))
                    .size(13.0)
                    .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if !self.tap_hold_settings.supported && !self.one_shot_settings.supported {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::TapHoldOneShotUnavailable),
                        Some(crate::i18n::tr(
                            lang,
                            crate::i18n::Key::QmkSettingsEnableHint,
                        )),
                    );
                    return;
                }

                if !hid_ready {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::TapHoldOneShotConnect),
                        None,
                    );
                    return;
                }

                if self.current_device_is_likely_rmk() && self.tap_hold_settings.supported {
                    ui.label(
                        RichText::new(rmk_hrm_profile_notice(lang))
                            .size(12.0)
                            .color(Color32::from_rgb(180, 120, 40)),
                    );
                    ui.add_space(12.0);
                }

                let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                let total_rows = self.tap_hold_one_shot_row_count();
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "tap_hold_settings",
                    metrics,
                    total_rows,
                    0.0,
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    self.draw_tap_hold_editor_content(
                        ui,
                        list.first_visible_row..list.last_visible_row,
                        list.row_content_width,
                        list.row_height,
                        list.suppress_tooltips,
                    );
                });

                if list.has_scrollbar {
                    crate::ui_style::paint_floating_scrollbar_handle(
                        ui,
                        list.track_rect,
                        list.handle_height,
                        list.scroll_ratio,
                        list.track_hovered,
                    );
                }
            });
        });
    }

    fn tap_hold_one_shot_row_count(&self) -> usize {
        tap_hold_one_shot_rows(
            self.app_settings.language,
            &self.tap_hold_settings,
            &self.one_shot_settings,
        )
        .len()
    }

    fn draw_tap_hold_editor_content(
        &mut self,
        ui: &mut egui::Ui,
        row_range: std::ops::Range<usize>,
        content_width: f32,
        row_height: f32,
        suppress_tooltips: bool,
    ) {
        let rows = tap_hold_one_shot_rows(
            self.app_settings.language,
            &self.tap_hold_settings,
            &self.one_shot_settings,
        );
        let scale = (row_height / 54.0).clamp(1.0, 1.12);
        let field_width = 86.0 * scale;
        let switch_width = 46.0 * scale;
        let switch_size = egui::vec2(46.0 * scale, 24.0 * scale);
        let control_height = 32.0 * scale;

        for row_idx in row_range {
            let Some(row) = rows.get(row_idx) else {
                continue;
            };
            let SettingsRow::Setting {
                kind,
                qsid,
                label,
                tooltip,
                is_bool,
                max,
            } = row
            else {
                if let SettingsRow::Section(title) = row {
                    self.draw_tap_hold_section_divider(ui, content_width, row_height, title);
                }
                continue;
            };
            let kind = *kind;
            let qsid = *qsid;
            let is_bool = *is_bool;
            let max = *max;
            if is_bool {
                let mut value = self.tap_hold_bool_value(qsid);
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    content_width,
                    row_height,
                    label,
                    true,
                    if suppress_tooltips {
                        None
                    } else {
                        Some(tooltip)
                    },
                    switch_width,
                    |ui| {
                        let resp = crate::ui_style::settings_switch_sized_stable(
                            ui,
                            ("tap_hold_settings", qsid),
                            &mut value,
                            switch_size,
                        );
                        if resp.changed() {
                            self.write_tap_hold_bool_setting(qsid, value);
                        }
                    },
                );
            } else {
                let current = match kind {
                    SettingsRowKind::TapHold => self.tap_hold_numeric_value(qsid),
                    SettingsRowKind::OneShot => self.one_shot_numeric_value(qsid),
                };
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    content_width,
                    row_height,
                    label,
                    true,
                    if suppress_tooltips {
                        None
                    } else {
                        Some(tooltip)
                    },
                    field_width,
                    |ui| {
                        let edit_id = egui::Id::new((
                            match kind {
                                SettingsRowKind::TapHold => "tap_hold_edit",
                                SettingsRowKind::OneShot => "one_shot_edit",
                            },
                            self.current_keyboard_id,
                            qsid,
                        ));
                        let mut text = ui.ctx().data_mut(|d| {
                            d.get_temp::<String>(edit_id)
                                .unwrap_or_else(|| current.to_string())
                        });
                        let tap_hold_edit_pending = kind == SettingsRowKind::TapHold
                            && self.pending_tap_hold_numeric_writes.contains_key(&qsid);
                        if text.parse::<u16>().ok() != Some(current)
                            && !ui.memory(|m| m.has_focus(edit_id))
                            && !tap_hold_edit_pending
                        {
                            text = current.to_string();
                        }
                        let resp = crate::ui_style::modern_text_field_sized(
                            ui,
                            edit_id,
                            &mut text,
                            field_width,
                            control_height,
                            "",
                            5,
                            egui::Align::RIGHT,
                        );
                        let resp = match (kind, qsid) {
                            (SettingsRowKind::TapHold, 7 | 25 | 18 | 19 | 27)
                            | (SettingsRowKind::OneShot, 6) => settings_field_unit_tooltip(
                                resp,
                                self.app_settings.language,
                                suppress_tooltips,
                                SettingsFieldUnit::Milliseconds,
                            ),
                            _ => resp,
                        };
                        let commit_tap_hold_edit = kind == SettingsRowKind::TapHold
                            && (resp.lost_focus()
                                || (resp.has_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))));
                        if resp.changed() {
                            let filtered: String =
                                text.chars().filter(|c: &char| c.is_ascii_digit()).collect();
                            let clamped = filtered.parse::<u32>().unwrap_or(0).min(max);
                            let new_value = clamped as u16;
                            match kind {
                                SettingsRowKind::TapHold => {
                                    if update_pending_tap_hold_numeric_write(
                                        &mut self.pending_tap_hold_numeric_writes,
                                        qsid,
                                        current,
                                        new_value,
                                    ) {
                                        self.tap_hold_numeric_write_due = Some(
                                            std::time::Instant::now() + TAP_HOLD_WRITE_DEBOUNCE,
                                        );
                                        ui.ctx().request_repaint_after(TAP_HOLD_WRITE_DEBOUNCE);
                                    } else if self.pending_tap_hold_numeric_writes.is_empty() {
                                        self.tap_hold_numeric_write_due = None;
                                    }
                                }
                                SettingsRowKind::OneShot if new_value != current => {
                                    self.set_one_shot_numeric_value(qsid, new_value);
                                    self.write_one_shot_numeric_setting(qsid, new_value);
                                }
                                SettingsRowKind::OneShot => {}
                            }
                            text = clamped.to_string();
                        }
                        if commit_tap_hold_edit {
                            self.commit_pending_tap_hold_numeric_write(qsid, current);
                        }
                        ui.ctx().data_mut(|d| d.insert_temp(edit_id, text));
                    },
                );
            }
        }
    }

    fn draw_tap_hold_section_divider(
        &self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        title: &str,
    ) {
        let dark = ui.visuals().dark_mode;
        let (row_rect, _) =
            ui.allocate_exact_size(egui::vec2(content_width, row_height), egui::Sense::hover());
        let separator =
            crate::ui_style::border_color(dark).gamma_multiply(if dark { 0.72 } else { 0.9 });
        ui.painter().line_segment(
            [row_rect.left_bottom(), row_rect.right_bottom()],
            egui::Stroke::new(1.0_f32, separator),
        );
        ui.painter().text(
            row_rect.center(),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(12.5),
            app_muted_text(dark),
        );
    }

    fn one_shot_numeric_value(&self, qsid: u16) -> u16 {
        match qsid {
            5 => self.one_shot_settings.tap_toggle as u16,
            6 => self.one_shot_settings.timeout,
            _ => 0,
        }
    }

    fn set_one_shot_numeric_value(&mut self, qsid: u16, value: u16) {
        match qsid {
            5 => self.one_shot_settings.tap_toggle = value.min(u8::MAX as u16) as u8,
            6 => self.one_shot_settings.timeout = value,
            _ => {}
        }
    }

    fn write_one_shot_numeric_setting(&mut self, qsid: u16, value: u16) {
        let Some(hid) = &self.hid_device else {
            return;
        };
        let result = if qsid == 5 {
            hid.set_qmk_setting_u8(qsid, value.min(u8::MAX as u16) as u8)
        } else {
            hid.set_qmk_setting_u16(qsid, value)
        };
        if let Err(e) = result {
            self.status_msg = format!("Failed to save One Shot setting (qsid {qsid}): {}", e);
            log::warn!("set_qmk_setting(one_shot qsid {qsid}) failed: {e}");
        }
    }

    fn tap_hold_numeric_value(&self, qsid: u16) -> u16 {
        match qsid {
            7 => self.tap_hold_settings.tapping_term,
            25 => self.tap_hold_settings.quick_tap_term,
            18 => self.tap_hold_settings.tap_code_delay,
            19 => self.tap_hold_settings.tap_hold_caps_delay,
            20 => self.tap_hold_settings.tapping_toggle,
            27 => self.tap_hold_settings.flow_tap,
            _ => 0,
        }
    }

    fn set_tap_hold_numeric_value(&mut self, qsid: u16, value: u16) {
        match qsid {
            7 => self.tap_hold_settings.tapping_term = value,
            25 => self.tap_hold_settings.quick_tap_term = value,
            18 => self.tap_hold_settings.tap_code_delay = value,
            19 => self.tap_hold_settings.tap_hold_caps_delay = value,
            20 => self.tap_hold_settings.tapping_toggle = value,
            27 => self.tap_hold_settings.flow_tap = value,
            _ => {}
        }
    }

    fn tap_hold_bool_value(&self, qsid: u16) -> bool {
        match qsid {
            22 => self.tap_hold_settings.permissive_hold,
            23 => self.tap_hold_settings.hold_on_other_key_press,
            24 => self.tap_hold_settings.retro_tapping,
            26 => self.tap_hold_settings.chordal_hold,
            _ => false,
        }
    }

    fn set_tap_hold_bool_value(&mut self, qsid: u16, value: bool) {
        match qsid {
            22 => self.tap_hold_settings.permissive_hold = value,
            23 => self.tap_hold_settings.hold_on_other_key_press = value,
            24 => self.tap_hold_settings.retro_tapping = value,
            26 => self.tap_hold_settings.chordal_hold = value,
            _ => {}
        }
    }

    fn tap_hold_write_error(&mut self, qsid: u16, error: &str) {
        let qsid = qsid.to_string();
        self.status_msg = crate::i18n::tr_catalog_format(
            self.app_settings.language,
            "status_messages.tap_hold_write_error",
            &[("qsid", &qsid), ("error", error)],
        );
    }

    pub(super) fn flush_due_tap_hold_numeric_writes(&mut self) {
        let Some(due) = self.tap_hold_numeric_write_due else {
            return;
        };
        if std::time::Instant::now() < due {
            return;
        }

        self.flush_pending_tap_hold_numeric_writes();
    }

    fn commit_pending_tap_hold_numeric_write(&mut self, qsid: u16, current: u16) {
        let Some(new_value) = self.pending_tap_hold_numeric_writes.remove(&qsid) else {
            return;
        };

        if new_value != current && !self.write_tap_hold_numeric_setting(qsid, new_value) {
            self.pending_tap_hold_numeric_writes.insert(qsid, new_value);
        }
        if self.pending_tap_hold_numeric_writes.is_empty() {
            self.tap_hold_numeric_write_due = None;
        } else {
            self.tap_hold_numeric_write_due =
                Some(std::time::Instant::now() + TAP_HOLD_WRITE_DEBOUNCE);
        }
    }

    pub(super) fn flush_pending_tap_hold_numeric_writes(&mut self) {
        self.tap_hold_numeric_write_due = None;
        let pending = std::mem::take(&mut self.pending_tap_hold_numeric_writes);
        for (qsid, value) in pending {
            if !self.write_tap_hold_numeric_setting(qsid, value) {
                self.pending_tap_hold_numeric_writes.insert(qsid, value);
            }
        }
        if !self.pending_tap_hold_numeric_writes.is_empty() {
            self.tap_hold_numeric_write_due =
                Some(std::time::Instant::now() + TAP_HOLD_WRITE_DEBOUNCE);
        }
    }

    fn write_tap_hold_numeric_setting(&mut self, qsid: u16, value: u16) -> bool {
        let Some(hid) = &self.hid_device else {
            self.tap_hold_write_error(
                qsid,
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "status_messages.device_unavailable",
                ),
            );
            return false;
        };
        let result = if qsid == 20 {
            hid.set_qmk_setting_u8_verified(qsid, value.min(u8::MAX as u16) as u8)
        } else {
            hid.set_qmk_setting_u16_verified(qsid, value)
        };
        match result {
            Ok(()) => {
                self.set_tap_hold_numeric_value(qsid, value);
                true
            }
            Err(e) => {
                self.tap_hold_write_error(qsid, &e.to_string());
                log::warn!("set_qmk_setting(tap_hold qsid {qsid}) failed: {e}");
                false
            }
        }
    }

    fn write_tap_hold_bool_setting(&mut self, qsid: u16, value: bool) {
        let Some(hid) = &self.hid_device else {
            self.tap_hold_write_error(
                qsid,
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "status_messages.device_unavailable",
                ),
            );
            return;
        };
        match hid.set_qmk_setting_u8_verified(qsid, u8::from(value)) {
            Ok(()) => self.set_tap_hold_bool_value(qsid, value),
            Err(e) => {
                self.tap_hold_write_error(qsid, &e.to_string());
                log::warn!("set_qmk_setting_u8(tap_hold qsid {qsid}) failed: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn pending_numeric_write_is_retained_when_hid_is_unavailable() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        app.pending_tap_hold_numeric_writes.insert(7, 175);

        app.flush_pending_tap_hold_numeric_writes();

        assert_eq!(app.pending_tap_hold_numeric_writes.get(&7), Some(&175));
        assert!(app.tap_hold_numeric_write_due.is_some());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn committed_numeric_write_is_requeued_when_hid_is_unavailable() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        app.pending_tap_hold_numeric_writes.insert(7, 175);

        app.commit_pending_tap_hold_numeric_write(7, 150);

        assert_eq!(app.pending_tap_hold_numeric_writes.get(&7), Some(&175));
        assert!(app.tap_hold_numeric_write_due.is_some());
    }

    #[test]
    fn tap_hold_rows_hide_unadvertised_qsids() {
        let mut tap_hold = TapHoldSettingsState::default();
        tap_hold.supported = true;
        tap_hold.set_qsid_supported(7);

        let rows = tap_hold_one_shot_rows(
            crate::i18n::Language::English,
            &tap_hold,
            &OneShotSettingsState::default(),
        );
        let qsids: Vec<u16> = rows
            .iter()
            .filter_map(|row| match row {
                SettingsRow::Setting { qsid, .. } => Some(*qsid),
                SettingsRow::Section(_) => None,
            })
            .collect();

        assert_eq!(qsids, vec![7]);
    }

    #[test]
    fn rmk_hrm_notice_mentions_firmware_profiles() {
        let notice = rmk_hrm_profile_notice(crate::i18n::Language::English);

        assert!(notice.contains("RMK"));
        assert!(notice.contains("firmware"));
        assert!(notice.contains("profiles"));
    }

    #[test]
    fn reverting_tap_hold_edit_cancels_pending_write() {
        let mut pending = std::collections::BTreeMap::new();

        assert!(update_pending_tap_hold_numeric_write(
            &mut pending,
            7,
            250,
            150
        ));
        assert_eq!(pending.get(&7), Some(&150));

        assert!(!update_pending_tap_hold_numeric_write(
            &mut pending,
            7,
            250,
            250
        ));
        assert!(!pending.contains_key(&7));
    }
}
