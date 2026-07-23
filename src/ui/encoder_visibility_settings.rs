use super::*;

fn encoder_visibility_copy(
    language: crate::i18n::Language,
    encoder_idx: usize,
    layout_option: Option<&LayoutOption>,
) -> (String, String) {
    let side = layout_option
        .map(|option| option.label.to_ascii_lowercase())
        .and_then(|label| {
            if label.split_whitespace().any(|word| word == "left") {
                Some("left")
            } else if label.split_whitespace().any(|word| word == "right") {
                Some("right")
            } else {
                None
            }
        });
    let (label_key, tooltip_key) = match side {
        Some("left") => (
            "encoder_settings.left_encoder",
            "encoder_settings.left_encoder_tooltip",
        ),
        Some("right") => (
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
        let switch_width = metrics.value(46.0);
        let switch_size = metrics.size(46.0, 24.0);

        let (encoder_indices, device_id, encoder_option_indices, layout_options) = self
            .layout
            .as_ref()
            .map(|layout| {
                let indices = layout
                    .encoders
                    .iter()
                    .map(|encoder| encoder.encoder_idx as usize)
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                let device_id = if self.current_encoder_visibility_id.is_empty() {
                    if self.current_device_name.is_empty() {
                        layout.name.clone()
                    } else {
                        self.current_device_name.clone()
                    }
                } else {
                    self.current_encoder_visibility_id.clone()
                };
                (
                    indices,
                    device_id,
                    Self::encoder_layout_option_indices(layout),
                    layout.layout_options.clone(),
                )
            })
            .unwrap_or((Vec::new(), String::new(), Vec::new(), Vec::new()));
        let visibility_len = encoder_indices
            .iter()
            .copied()
            .max()
            .map(|idx| idx + 1)
            .unwrap_or(0);

        if self.encoder_visibility.len() < visibility_len {
            self.encoder_visibility.resize(visibility_len, true);
        }
        self.encoder_visibility.truncate(visibility_len);

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

                if encoder_indices.is_empty() {
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
                        for (encoder_position, encoder_idx) in encoder_indices.iter().enumerate() {
                            let mut visible = self.encoder_visibility[*encoder_idx];
                            let layout_option = encoder_option_indices
                                .get(encoder_position)
                                .and_then(|option_idx| layout_options.get(*option_idx));
                            let (label, tooltip) = encoder_visibility_copy(
                                self.app_settings.language,
                                *encoder_idx,
                                layout_option,
                            );
                            crate::ui_style::settings_list_row_with_tooltip(
                                ui,
                                encoders_content_width,
                                encoders_row_height,
                                &label,
                                true,
                                Some(&tooltip),
                                switch_width,
                                |ui| {
                                    let resp = crate::ui_style::settings_switch_sized(
                                        ui,
                                        &mut visible,
                                        switch_size,
                                    );
                                    if resp.changed() {
                                        self.encoder_visibility[*encoder_idx] = visible;
                                        if !device_id.is_empty() {
                                            save_encoder_visibility(
                                                &self.encoder_visibility,
                                                &device_id,
                                            );
                                        }
                                        if let Some(option_idx) =
                                            encoder_option_indices.get(encoder_position).copied()
                                        {
                                            let mut values = Self::unpack_layout_option_values(
                                                &layout_options,
                                                self.layout_options_value.unwrap_or(0),
                                            );
                                            if let Some(slot) = values.get_mut(option_idx) {
                                                *slot = u32::from(!visible);
                                                let packed = Self::pack_layout_option_values(
                                                    &layout_options,
                                                    &values,
                                                );
                                                self.layout_options_value = Some(packed);
                                                #[cfg(not(target_arch = "wasm32"))]
                                                if let Some(hid) = &self.hid_device {
                                                    if let Err(e) = hid.set_layout_options(packed) {
                                                        self.status_msg = format!(
                                                            "Failed to save encoder visibility: {e}"
                                                        );
                                                        log::warn!(
                                                            "set_layout_options for encoder visibility failed: {e}"
                                                        );
                                                    }
                                                }
                                                #[cfg(not(target_arch = "wasm32"))]
                                                self.sync_qmk_hid_host_bridges();
                                            }
                                        }
                                    }
                                },
                            );
                        }
                    },
                );
            });
        });
    }
}
