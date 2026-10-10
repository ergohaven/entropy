use super::module_settings_ui::{fixed_encoder_field_number, fixed_encoder_side_size};
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum EncoderVisibilitySide {
    Left,
    Right,
}

#[derive(Clone, Copy)]
struct EncoderVisibilityRowContext {
    content_width: f32,
    height: f32,
    suppress_tooltips: bool,
}

#[derive(Clone, Copy)]
pub(super) enum EncoderSettingsRow {
    Visibility(usize, Option<usize>),
    Field(usize, usize),
}

// The legacy file is model-wide. Keep it unchanged so every serial can
// inherit the same default; the per-device marker makes imports one-shot.
fn migrate_hidden_encoders(
    app_settings: &mut AppSettings,
    device_key: &str,
    legacy: &mut [bool],
) -> bool {
    let first_import = app_settings
        .migrated_fixed_encoder_visibility
        .insert(device_key.to_owned());
    if first_import {
        let local = app_settings
            .layout_element_visibility
            .entry(device_key.to_owned())
            .or_default();
        for (index, visible) in legacy.iter().enumerate() {
            if !visible {
                local.hidden_encoders.insert(index as u8);
            }
        }
    }
    // Only in memory: a second device of this model still needs the saved
    // legacy vector to inherit its hidden choices.
    legacy.fill(true);
    first_import
}

fn encoder_visibility_side(layout_option: &LayoutOption) -> Option<EncoderVisibilitySide> {
    let label = layout_option.label.to_ascii_lowercase();
    if label.split_whitespace().any(|word| word == "left") {
        Some(EncoderVisibilitySide::Left)
    } else if label.split_whitespace().any(|word| word == "right") {
        Some(EncoderVisibilitySide::Right)
    } else {
        None
    }
}

fn encoder_visibility_copy(
    language: crate::i18n::Language,
    encoder_idx: usize,
    layout_option: Option<&LayoutOption>,
    fixed_side_size: Option<usize>,
) -> (String, String) {
    if let Some(3) = fixed_side_size {
        let (label_key, tooltip_key) = if encoder_idx < 3 {
            (
                "encoder_settings.left_numbered_encoder",
                "encoder_settings.left_numbered_encoder_tooltip",
            )
        } else {
            (
                "encoder_settings.right_numbered_encoder",
                "encoder_settings.right_numbered_encoder_tooltip",
            )
        };
        let number = (encoder_idx % 3 + 1).to_string();
        return (
            crate::i18n::tr_catalog_format(language, label_key, &[("number", &number)]),
            crate::i18n::tr_catalog_format(language, tooltip_key, &[("number", &number)]),
        );
    }
    let (label_key, tooltip_key) = match layout_option.and_then(encoder_visibility_side) {
        Some(EncoderVisibilitySide::Left) => (
            "encoder_settings.left_encoder",
            "encoder_settings.left_encoder_tooltip",
        ),
        Some(EncoderVisibilitySide::Right) => (
            "encoder_settings.right_encoder",
            "encoder_settings.right_encoder_tooltip",
        ),
        _ => (
            "encoder_settings.encoder_number",
            "encoder_settings.encoder_number_tooltip",
        ),
    };
    let number = (encoder_idx + 1).to_string();
    (
        crate::i18n::tr_catalog_format(language, label_key, &[("number", &number)]),
        crate::i18n::tr_catalog_format(language, tooltip_key, &[("number", &number)]),
    )
}

impl EntropyApp {
    pub(super) fn show_separate_encoder_visibility_settings(
        &self,
        layout: &KeyboardLayout,
    ) -> bool {
        separate_encoder_visibility_settings_available(layout)
    }

    pub(super) fn encoder_visibility_allows(
        layout: &KeyboardLayout,
        encoder_idx: u8,
        visibility: &[bool],
    ) -> bool {
        !layout_uses_combined_encoder_press(layout)
            || visibility
                .get(encoder_idx as usize)
                .copied()
                .unwrap_or(true)
    }

    fn encoder_visibility_entries(layout: &KeyboardLayout) -> Vec<(usize, Option<usize>)> {
        let option_indices = Self::encoder_layout_option_indices(layout);
        layout
            .encoders
            .iter()
            .map(|encoder| encoder.encoder_idx as usize)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .enumerate()
            .map(|(position, encoder_idx)| (encoder_idx, option_indices.get(position).copied()))
            .collect()
    }

    pub(super) fn encoder_settings_rows(&self, layout: &KeyboardLayout) -> Vec<EncoderSettingsRow> {
        let mut rows = Vec::new();
        if self.encoder_only_module_settings(layout) {
            // One pair of controls applies to every physical encoder, independently
            // of which encoder visibility switches are enabled in the layout.
            if let Some((group_idx, group)) = self
                .module_settings
                .groups
                .iter()
                .enumerate()
                .find(|(_, group)| group.kind == ModuleSettingsGroupKind::Left)
            {
                for (field_idx, field) in group.fields.iter().enumerate() {
                    let name = group.kind.field_base_title(&field.title);
                    if name.to_ascii_lowercase().ends_with(" interval")
                        || name.to_ascii_lowercase().ends_with(" steps")
                    {
                        if fixed_encoder_field_number(
                            name,
                            fixed_encoder_side_size(layout).unwrap_or(0),
                        ) == Some(1)
                        {
                            rows.push(EncoderSettingsRow::Field(group_idx, field_idx));
                        }
                    }
                }
            }
        }
        if !self.encoder_only_module_settings(layout) {
            rows.extend(
                Self::encoder_visibility_entries(layout)
                    .into_iter()
                    .map(|(idx, option)| EncoderSettingsRow::Visibility(idx, option)),
            );
        }
        rows
    }

    pub(super) fn encoder_visibility_entry_for_module_group(
        layout: &KeyboardLayout,
        group_kind: ModuleSettingsGroupKind,
    ) -> Option<(usize, usize)> {
        let expected_side = match group_kind {
            ModuleSettingsGroupKind::Left => EncoderVisibilitySide::Left,
            ModuleSettingsGroupKind::Right => EncoderVisibilitySide::Right,
            ModuleSettingsGroupKind::AutoLayer | ModuleSettingsGroupKind::Other => return None,
        };
        Self::encoder_visibility_entries(layout)
            .into_iter()
            .find_map(|(encoder_idx, option_idx)| {
                let option_idx = option_idx?;
                let option = layout.layout_options.get(option_idx)?;
                (encoder_visibility_side(option) == Some(expected_side))
                    .then_some((encoder_idx, option_idx))
            })
    }

    fn ensure_encoder_visibility_len(&mut self, len: usize) {
        if self.encoder_visibility.len() < len {
            self.encoder_visibility.resize(len, true);
        }
        self.encoder_visibility.truncate(len);
    }

    pub(super) fn resolve_initial_encoder_visibility(
        layout: &KeyboardLayout,
        packed: Option<u32>,
        saved: Option<Vec<bool>>,
        hidden_by_default: bool,
        shared_controls_available: bool,
    ) -> Vec<bool> {
        let encoder_count = layout.encoder_count();
        if encoder_count == 0 {
            return Vec::new();
        }

        let has_saved_choice = saved.is_some();
        let mut visibility = saved.unwrap_or_else(|| vec![!hidden_by_default; encoder_count]);
        visibility.resize(encoder_count, !hidden_by_default);
        visibility.truncate(encoder_count);

        // These models now use local Layout -> Show/Hide Keys. A firmware
        // hide-option bit may still be set by an older Entropy release; do not
        // reapply it after migrating the old choice into local visibility.
        let fixed_model =
            fixed_encoder_side_size(layout).is_some_and(|side_size| encoder_count == side_size * 2);
        if !(fixed_model && shared_controls_available) && (has_saved_choice || !hidden_by_default) {
            Self::apply_encoder_layout_options_to_visibility(layout, packed, &mut visibility);
        }
        visibility
    }

    pub(super) fn migrate_fixed_encoder_visibility_to_layout(&mut self, layout: &KeyboardLayout) {
        if !self.encoder_only_module_settings(layout) {
            return;
        }
        let Some(device_key) = self.layout_element_visibility_device_key() else {
            return;
        };
        if migrate_hidden_encoders(
            &mut self.app_settings,
            &device_key,
            &mut self.encoder_visibility,
        ) {
            save_app_settings(&self.app_settings);
        }
    }

    fn encoder_visibility_device_id(&self) -> String {
        if !self.current_encoder_visibility_id.is_empty() {
            return self.current_encoder_visibility_id.clone();
        }
        if !self.current_device_name.is_empty() {
            return self.current_device_name.clone();
        }
        self.layout
            .as_ref()
            .map(|layout| layout.name.clone())
            .unwrap_or_default()
    }

    fn set_encoder_visibility(
        &mut self,
        encoder_idx: usize,
        option_idx: Option<usize>,
        visible: bool,
    ) {
        if self.encoder_visibility.len() <= encoder_idx {
            self.encoder_visibility.resize(encoder_idx + 1, true);
        }
        self.encoder_visibility[encoder_idx] = visible;

        let device_id = self.encoder_visibility_device_id();
        if !device_id.is_empty() {
            save_encoder_visibility(&self.encoder_visibility, &device_id);
        }

        let Some(option_idx) = option_idx else {
            return;
        };
        let Some(layout_options) = self
            .layout
            .as_ref()
            .map(|layout| layout.layout_options.clone())
        else {
            return;
        };
        let mut values = Self::unpack_layout_option_values(
            &layout_options,
            self.layout_options_value.unwrap_or(0),
        );
        let Some(slot) = values.get_mut(option_idx) else {
            return;
        };
        *slot = u32::from(!visible);
        let packed = Self::pack_layout_option_values(&layout_options, &values);
        self.layout_options_value = Some(packed);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(hid) = &self.hid_device {
            if let Err(e) = hid.set_layout_options(packed) {
                self.status_msg = format!("Failed to save encoder visibility: {e}");
                log::warn!("set_layout_options for encoder visibility failed: {e}");
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.sync_qmk_hid_host_bridges();
    }

    fn draw_encoder_visibility_setting_row(
        &mut self,
        ui: &mut egui::Ui,
        row: EncoderVisibilityRowContext,
        encoder_idx: usize,
        option_idx: Option<usize>,
    ) {
        if self.encoder_visibility.len() <= encoder_idx {
            self.encoder_visibility.resize(encoder_idx + 1, true);
        }
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let layout_option = option_idx
            .and_then(|idx| self.layout.as_ref()?.layout_options.get(idx))
            .cloned();
        let (label, tooltip) = encoder_visibility_copy(
            self.app_settings.language,
            encoder_idx,
            layout_option.as_ref(),
            self.layout.as_ref().and_then(|layout| {
                self.encoder_only_module_settings(layout)
                    .then(|| fixed_encoder_side_size(layout))
                    .flatten()
            }),
        );
        let mut switch_enabled = self.encoder_visibility[encoder_idx];
        crate::ui_style::settings_list_row_with_tooltip(
            ui,
            row.content_width,
            row.height,
            &label,
            true,
            (!row.suppress_tooltips).then_some(tooltip.as_str()),
            metrics.value(46.0),
            |ui| {
                let resp = crate::ui_style::settings_switch_sized_stable(
                    ui,
                    ("encoder_visibility", encoder_idx),
                    &mut switch_enabled,
                    metrics.size(46.0, 24.0),
                );
                if resp.changed() {
                    self.set_encoder_visibility(encoder_idx, option_idx, switch_enabled);
                }
            },
        );
    }

    pub(super) fn draw_encoder_visibility_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
        dark: bool,
    ) {
        let lang = self.app_settings.language;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let rows = self
            .layout
            .as_ref()
            .map(|layout| self.encoder_settings_rows(layout))
            .unwrap_or_default();
        // The scoped page has no visibility rows, but the saved visibility
        // vector is still needed for migration to Layout -> Show/Hide Keys.
        let visibility_len = self
            .layout
            .as_ref()
            .map_or(0, KeyboardLayout::encoder_count);
        self.ensure_encoder_visibility_len(visibility_len);

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::EncodersTitle))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        if self
                            .layout
                            .as_ref()
                            .is_some_and(|layout| self.encoder_only_module_settings(layout))
                        {
                            crate::i18n::tr_catalog(lang, "encoder_settings.controls_description")
                        } else {
                            crate::i18n::tr(lang, crate::i18n::Key::EncodersDescription)
                        },
                    )
                    .size(13.0)
                    .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if rows.is_empty() {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::EncodersUnavailable),
                        None,
                    );
                    return;
                }

                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "encoder_settings",
                    metrics,
                    rows.len(),
                    0.0,
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for row_idx in list.first_visible_row..list.last_visible_row {
                        match rows[row_idx] {
                            EncoderSettingsRow::Visibility(encoder_idx, option_idx) => {
                                self.draw_encoder_visibility_setting_row(
                                    ui,
                                    EncoderVisibilityRowContext {
                                        content_width: list.row_content_width,
                                        height: list.row_height,
                                        suppress_tooltips: list.suppress_tooltips,
                                    },
                                    encoder_idx,
                                    option_idx,
                                );
                            }
                            EncoderSettingsRow::Field(group_idx, field_idx) => self
                                .draw_module_settings_field_row(
                                    ui,
                                    group_idx,
                                    field_idx,
                                    list.row_content_width,
                                    list.row_height,
                                    list.suppress_tooltips,
                                ),
                        }
                    }
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
}

fn separate_encoder_visibility_settings_available(layout: &KeyboardLayout) -> bool {
    let normalized_name = layout
        .name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    let is_m4cr0pad =
        normalized_name.starts_with("m4cr0pad") || normalized_name.starts_with("ergohavenm4cr0pad");

    !is_m4cr0pad && layout.encoder_count() > 0 && layout_uses_combined_encoder_press(layout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout_with_encoder_hide_option() -> KeyboardLayout {
        KeyboardLayout {
            name: "Modular keyboard".to_owned(),
            rows: 1,
            cols: 1,
            keys: Vec::new(),
            encoders: vec![PhysicalEncoder {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
                label: String::new(),
                encoder_idx: 0,
                direction: 0,
                rotation: 0.0,
                rotation_x: 0.0,
                rotation_y: 0.0,
                layout_condition: None,
            }],
            layers: Vec::new(),
            encoder_layers: Vec::new(),
            layer_names: Vec::new(),
            custom_keycodes: Vec::new(),
            layout_options: vec![LayoutOption {
                label: "Hide left encoder module".to_owned(),
                choices: Vec::new(),
            }],
            live_features: Default::default(),
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
        }
    }

    #[test]
    fn m4cr0pad_does_not_expose_a_separate_encoder_settings_tab() {
        let mut layout = layout_with_encoder_hide_option();
        layout.name = "M4CR0Pad v3".to_owned();

        assert!(!separate_encoder_visibility_settings_available(&layout));
    }

    #[test]
    fn modular_encoder_defaults_to_hidden_until_user_choice_exists() {
        let layout = layout_with_encoder_hide_option();
        assert_eq!(
            EntropyApp::resolve_initial_encoder_visibility(&layout, Some(0), None, true, false),
            vec![false]
        );
    }

    #[test]
    fn separate_encoder_settings_keep_visible_default() {
        let layout = layout_with_encoder_hide_option();
        assert_eq!(
            EntropyApp::resolve_initial_encoder_visibility(&layout, Some(0), None, false, false),
            vec![true]
        );
    }

    #[test]
    fn saved_modular_encoder_choice_remains_authoritative() {
        let layout = layout_with_encoder_hide_option();
        assert_eq!(
            EntropyApp::resolve_initial_encoder_visibility(
                &layout,
                Some(0),
                Some(vec![true]),
                true,
                false,
            ),
            vec![true]
        );
    }

    fn fixed_test_layout(side_size: usize, name: &str) -> KeyboardLayout {
        let mut layout = layout_with_encoder_hide_option();
        layout.name = name.to_owned();
        layout.encoders = (0..side_size * 2)
            .map(|idx| PhysicalEncoder {
                encoder_idx: idx as u8,
                ..layout.encoders[0].clone()
            })
            .collect();
        layout
    }

    #[test]
    fn old_fixed_firmware_without_saved_visibility_preserves_hide_option() {
        for (side_size, name) in [(3, "Ergohaven K:03"), (1, "Ergohaven Imperial44")] {
            let layout = fixed_test_layout(side_size, name);
            let mut expected = vec![true; side_size * 2];
            expected[0] = false;
            assert_eq!(
                EntropyApp::resolve_initial_encoder_visibility(
                    &layout,
                    Some(1),
                    None,
                    false,
                    false,
                ),
                expected,
                "{name}: old firmware must honor its Hide encoder bit"
            );
            assert_eq!(
                EntropyApp::resolve_initial_encoder_visibility(&layout, Some(1), None, false, true),
                vec![true; side_size * 2],
                "{name}: shared-control firmware must not reapply its obsolete bit"
            );
        }
    }

    #[test]
    fn legacy_visibility_inherits_for_two_serials_without_rehiding_user_choice() {
        for (side_size, name) in [(3, "Ergohaven K:03"), (1, "Ergohaven Imperial44")] {
            let layout = fixed_test_layout(side_size, name);
            let mut legacy = vec![true; side_size * 2];
            legacy[side_size] = false;
            let mut settings = AppSettings::default();
            let base = format!("vial_1234_{name}");
            let first = super::super::layout_element_visibility::local_layout_visibility_key(
                &base,
                Some("SERIAL-A"),
            )
            .unwrap();
            let second = super::super::layout_element_visibility::local_layout_visibility_key(
                &base,
                Some("SERIAL-B"),
            )
            .unwrap();
            assert_ne!(first, second);
            // Preserve pre-existing layout choices in the first local entry.
            settings
                .layout_element_visibility
                .entry(first.clone())
                .or_default()
                .hidden_encoders
                .insert(0);
            for device_key in [&first, &second] {
                let mut visible = EntropyApp::resolve_initial_encoder_visibility(
                    &layout,
                    Some(1),
                    Some(legacy.clone()),
                    false,
                    true,
                );
                assert_eq!(visible, legacy);
                assert!(migrate_hidden_encoders(
                    &mut settings,
                    device_key,
                    &mut visible
                ));
                assert!(visible.iter().all(|value| *value));
                assert!(settings.layout_element_visibility[device_key]
                    .hidden_encoders
                    .contains(&(side_size as u8)));
                assert_eq!(
                    legacy[side_size], false,
                    "shared legacy default must survive"
                );
            }
            assert!(settings.layout_element_visibility[&first]
                .hidden_encoders
                .contains(&0));
            settings
                .layout_element_visibility
                .get_mut(&first)
                .unwrap()
                .hidden_encoders
                .remove(&(side_size as u8));
            let mut visible_again = legacy.clone();
            assert!(!migrate_hidden_encoders(
                &mut settings,
                &first,
                &mut visible_again
            ));
            assert!(visible_again.iter().all(|value| *value));
            assert!(!settings.layout_element_visibility[&first]
                .hidden_encoders
                .contains(&(side_size as u8)));
            assert!(settings.layout_element_visibility[&second]
                .hidden_encoders
                .contains(&(side_size as u8)));
            // Marker and per-serial choices persist across settings reloads.
            let saved = serde_json::to_string(&settings).unwrap();
            let restored: AppSettings = serde_json::from_str(&saved).unwrap();
            assert!(restored.migrated_fixed_encoder_visibility.contains(&first));
            assert!(restored.migrated_fixed_encoder_visibility.contains(&second));
        }
    }

    #[test]
    fn manual_encoder_visibility_is_limited_to_k03_and_imperial44() {
        for name in ["K:03", "Ergohaven K:03", "Imperial44"] {
            let mut layout = layout_with_encoder_hide_option();
            layout.name = name.to_owned();

            assert!(EntropyApp::encoder_visibility_allows(&layout, 0, &[true]));
            assert!(!EntropyApp::encoder_visibility_allows(&layout, 0, &[false]));
        }

        let layout = layout_with_encoder_hide_option();
        assert!(EntropyApp::encoder_visibility_allows(&layout, 0, &[false]));
    }
}
