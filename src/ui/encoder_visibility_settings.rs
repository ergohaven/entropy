use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum EncoderVisibilitySide {
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EncoderVisibilitySwitchSemantics {
    Show,
    Hide,
}

#[derive(Clone, Copy)]
struct EncoderVisibilityRowContext {
    content_width: f32,
    height: f32,
    suppress_tooltips: bool,
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
    semantics: EncoderVisibilitySwitchSemantics,
) -> (String, String) {
    if semantics == EncoderVisibilitySwitchSemantics::Hide {
        return (
            crate::i18n::tr_catalog(language, "encoder_settings.hide").to_owned(),
            crate::i18n::tr_catalog(language, "encoder_settings.hide_tooltip").to_owned(),
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
        layout.encoder_count() > 0 && !self.module_settings_include_encoder_visibility(layout)
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
    ) -> Vec<bool> {
        let encoder_count = layout.encoder_count();
        if encoder_count == 0 {
            return Vec::new();
        }

        let has_saved_choice = saved.is_some();
        let mut visibility = saved.unwrap_or_else(|| vec![!hidden_by_default; encoder_count]);
        visibility.resize(encoder_count, !hidden_by_default);
        visibility.truncate(encoder_count);

        if has_saved_choice || !hidden_by_default {
            Self::apply_encoder_layout_options_to_visibility(layout, packed, &mut visibility);
        }
        visibility
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
        semantics: EncoderVisibilitySwitchSemantics,
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
            semantics,
        );
        let mut switch_enabled = match semantics {
            EncoderVisibilitySwitchSemantics::Show => self.encoder_visibility[encoder_idx],
            EncoderVisibilitySwitchSemantics::Hide => !self.encoder_visibility[encoder_idx],
        };
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
                    (
                        "encoder_visibility",
                        encoder_idx,
                        semantics == EncoderVisibilitySwitchSemantics::Hide,
                    ),
                    &mut switch_enabled,
                    metrics.size(46.0, 24.0),
                );
                if resp.changed() {
                    let visible = match semantics {
                        EncoderVisibilitySwitchSemantics::Show => switch_enabled,
                        EncoderVisibilitySwitchSemantics::Hide => !switch_enabled,
                    };
                    self.set_encoder_visibility(encoder_idx, option_idx, visible);
                }
            },
        );
    }

    pub(super) fn draw_module_encoder_visibility_setting_row(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        suppress_tooltips: bool,
        encoder_idx: usize,
        option_idx: usize,
    ) {
        let row = EncoderVisibilityRowContext {
            content_width,
            height: row_height,
            suppress_tooltips,
        };
        self.draw_encoder_visibility_setting_row(
            ui,
            row,
            encoder_idx,
            Some(option_idx),
            EncoderVisibilitySwitchSemantics::Hide,
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
        let encoders_content_width = metrics.settings_content_width();
        let encoders_row_height = metrics.settings_row_height();
        let encoders_top_padding = metrics.value(4.0);
        let entries = self
            .layout
            .as_ref()
            .map(Self::encoder_visibility_entries)
            .unwrap_or_default();
        let visibility_len = entries
            .iter()
            .map(|(encoder_idx, _)| *encoder_idx)
            .max()
            .map(|idx| idx + 1)
            .unwrap_or(0);
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
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::EncodersDescription))
                        .size(13.0)
                        .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if entries.is_empty() {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::EncodersUnavailable),
                        None,
                    );
                    return;
                }

                crate::ui_style::modal_content(
                    ui,
                    crate::ui_style::ModalLayout::new(encoders_content_width)
                        .with_top_padding(encoders_top_padding),
                    |ui| {
                        let row = EncoderVisibilityRowContext {
                            content_width: encoders_content_width,
                            height: encoders_row_height,
                            suppress_tooltips: false,
                        };
                        for (encoder_idx, option_idx) in entries.iter().copied() {
                            self.draw_encoder_visibility_setting_row(
                                ui,
                                row,
                                encoder_idx,
                                option_idx,
                                EncoderVisibilitySwitchSemantics::Show,
                            );
                        }
                    },
                );
            });
        });
    }
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
    fn modular_encoder_defaults_to_hidden_until_user_choice_exists() {
        let layout = layout_with_encoder_hide_option();
        assert_eq!(
            EntropyApp::resolve_initial_encoder_visibility(&layout, Some(0), None, true),
            vec![false]
        );
    }

    #[test]
    fn separate_encoder_settings_keep_visible_default() {
        let layout = layout_with_encoder_hide_option();
        assert_eq!(
            EntropyApp::resolve_initial_encoder_visibility(&layout, Some(0), None, false),
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
            ),
            vec![true]
        );
    }

    #[test]
    fn hide_switch_uses_action_copy() {
        assert_eq!(
            encoder_visibility_copy(
                crate::i18n::Language::Russian,
                0,
                None,
                EncoderVisibilitySwitchSemantics::Hide,
            )
            .0,
            "Скрыть"
        );
    }
}
