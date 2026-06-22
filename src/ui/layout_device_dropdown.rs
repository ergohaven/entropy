use super::*;

fn entlayout_import_label(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Импорт раскладки",
        crate::i18n::Language::English => "Import layout",
    }
}

fn entlayout_export_label(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Экспорт раскладки",
        crate::i18n::Language::English => "Export layout",
    }
}

fn layout_image_export_label(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Экспорт картинки",
        crate::i18n::Language::English => "Export image",
    }
}

impl EntropyApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_layout_device_dropdown(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        lang: crate::i18n::Language,
        device_tab_rect: Option<egui::Rect>,
        device_tab_hovered: bool,
        advanced_tab_hovered: bool,
        settings_tab_hovered: bool,
    ) {
        use crate::i18n::Key as TrKey;

        if let Some(device_rect) = device_tab_rect {
            let dropdown_id = ui.make_persistent_id("device_dropdown_open");
            let was_open = ui
                .ctx()
                .data(|d| d.get_temp::<bool>(dropdown_id))
                .unwrap_or(false);
            let device_count = self.device_manager.devices().len();
            let device_rows = device_count.max(1) as f32;
            let devices_h = 12.0 + device_rows * 30.0;
            let sticky_layout_h = 36.0;
            #[cfg(not(target_arch = "wasm32"))]
            let import_export_h = 102.0;
            #[cfg(target_arch = "wasm32")]
            let import_export_h = 0.0;
            let show_key_legend_switcher = self.app_settings.key_legend_layout.is_multilingual();
            let key_legend_switcher_h = if show_key_legend_switcher { 36.0 } else { 0.0 };
            let mut device_menu_labels: Vec<String> = if self.device_manager.devices().is_empty() {
                vec![crate::i18n::tr(lang, TrKey::NoDevicesFound).to_owned()]
            } else {
                self.device_manager
                    .devices()
                    .iter()
                    .map(|dev| {
                        self.device_display_names
                            .get(&dev.display_name_cache_key())
                            .cloned()
                            .unwrap_or_else(|| dev.name.clone())
                    })
                    .collect()
            };
            if show_key_legend_switcher {
                if let Some(order_key) = self.app_settings.key_legend_layout.order_i18n_key() {
                    device_menu_labels.push(crate::i18n::tr_catalog(lang, order_key).to_owned());
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                device_menu_labels.push(entlayout_import_label(lang).to_owned());
                device_menu_labels.push(entlayout_export_label(lang).to_owned());
                device_menu_labels.push(layout_image_export_label(lang).to_owned());
            }
            device_menu_labels
                .push(crate::i18n::tr_catalog(lang, "ui.sticky_layout_window_label").to_owned());
            let dropdown_size = Vec2::new(
                adaptive_top_dropdown_width(
                    ui,
                    device_menu_labels.iter().map(String::as_str),
                    152.0,
                ),
                devices_h + key_legend_switcher_h + import_export_h + sticky_layout_h + 12.0,
            );
            let dropdown_rect = egui::Rect::from_min_size(
                egui::pos2(
                    device_rect.center().x - dropdown_size.x / 2.0,
                    device_rect.bottom() + 6.0,
                ),
                dropdown_size,
            );
            let hover_bridge_rect = device_rect.union(dropdown_rect).expand(3.0);
            let pointer_over_bridge = ui
                .ctx()
                .input(|i| i.pointer.hover_pos())
                .map(|pos| hover_bridge_rect.contains(pos))
                .unwrap_or(false);
            let show_dropdown = !advanced_tab_hovered
                && !settings_tab_hovered
                && (device_tab_hovered || (was_open && pointer_over_bridge));

            if show_dropdown {
                let area_id = ui.make_persistent_id("device_dropdown_area");
                let mut device_clicked = false;
                egui::Area::new(area_id)
                        .order(egui::Order::Foreground)
                        .fixed_pos(dropdown_rect.min)
                        .show(ctx, |ui| {
                            let dark = ui.visuals().dark_mode;
                            top_dropdown_frame(dark).show(ui, |ui| {
                                ui.set_min_width(dropdown_size.x - 16.0);

                                let prev_selected = self.selected_device;
                                if self.device_manager.devices().is_empty() {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(dropdown_size.x - 16.0, 30.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.add_space(10.0);
                                            ui.label(
                                                RichText::new(crate::i18n::tr(
                                                    lang,
                                                    TrKey::NoDevicesFound,
                                                ))
                                                .size(13.0)
                                                .color(app_muted_text(ui.visuals().dark_mode)),
                                            );
                                        },
                                    );
                                } else {
                                    for (i, dev) in self.device_manager.devices().iter().enumerate()
                                    {
                                        let is_selected = self.selected_device == Some(i);
                                        let cached_display_name = self
                                            .device_display_names
                                            .get(&dev.display_name_cache_key())
                                            .map(String::as_str);
                                        let display_name =
                                            cached_display_name.unwrap_or(dev.name.as_str());
                                        let resp = top_dropdown_item(
                                            ui,
                                            dropdown_size.x - 16.0,
                                            display_name,
                                            true,
                                            is_selected,
                                        );
                                        if resp.clicked() {
                                            self.selected_device = Some(i);
                                            self.main_menu_tab = MainMenuTab::Keyboard;
                                            device_clicked = true;
                                        }
                                    }
                                }

                                #[cfg(not(target_arch = "wasm32"))]
                                if self.selected_device != prev_selected {
                                    if let Some(idx) = self.selected_device {
                                        self.start_connect(idx);
                                    }
                                }


                                if show_key_legend_switcher {
                                    if let Some(order_key) =
                                        self.app_settings.key_legend_layout.order_i18n_key()
                                    {
                                        ui.add_space(6.0);
                                        let order_label = crate::i18n::tr_catalog(lang, order_key);
                                        if top_dropdown_item(
                                            ui,
                                            dropdown_size.x - 16.0,
                                            order_label,
                                            true,
                                            false,
                                        )
                                        .clicked()
                                        {
                                            self.app_settings.key_legend_layout =
                                                self.app_settings.key_legend_layout.toggled_order();
                                            save_app_settings(&self.app_settings);
                                            ctx.request_repaint();
                                        }
                                    }
                                }

                                #[cfg(not(target_arch = "wasm32"))]
                                {
                                    ui.add_space(6.0);
                                    if top_dropdown_item(
                                        ui,
                                        dropdown_size.x - 16.0,
                                        entlayout_import_label(lang),
                                        self.layout.is_some(),
                                        false,
                                    )
                                    .clicked()
                                    {
                                        self.close_top_dropdowns(ctx);
                                        self.import_entlayout_dialog();
                                        ctx.request_repaint();
                                    }
                                    if top_dropdown_item(
                                        ui,
                                        dropdown_size.x - 16.0,
                                        entlayout_export_label(lang),
                                        self.layout.is_some(),
                                        false,
                                    )
                                    .clicked()
                                    {
                                        self.close_top_dropdowns(ctx);
                                        self.export_entlayout_dialog();
                                        ctx.request_repaint();
                                    }
                                    if top_dropdown_item(
                                        ui,
                                        dropdown_size.x - 16.0,
                                        layout_image_export_label(lang),
                                        self.layout.is_some(),
                                        false,
                                    )
                                    .clicked()
                                    {
                                        self.close_top_dropdowns(ctx);
                                        self.open_layout_image_export_page();
                                        ctx.request_repaint();
                                    }
                                }

                                ui.add_space(6.0);
                                if top_dropdown_item(
                                    ui,
                                    dropdown_size.x - 16.0,
                                    crate::i18n::tr_catalog(lang, "ui.sticky_layout_window_label"),
                                    true,
                                    self.app_settings.sticky_layout_window,
                                )
                                .clicked()
                                {
                                    if self.app_settings.sticky_layout_window {
                                        self.app_settings.sticky_layout_window = false;
                                        self.pending_layout_indicator_open_after_unlock = false;
                                        self.sticky_layout_last_size = None;
                                        save_app_settings(&self.app_settings);
                                    } else if self.is_vial_locked() {
                                        self.pending_layout_indicator_open_after_unlock = true;
                                        self.unlock_open = true;
                                        self.status_msg = crate::i18n::tr_catalog(
                                            self.app_settings.language,
                                            "matrix_tester.keyboard_is_locked_unlock_it_to_use_matrix_tester",
                                        )
                                        .into();
                                    } else {
                                        self.app_settings.sticky_layout_window = true;
                                        self.sticky_layout_last_size = None;
                                        save_app_settings(&self.app_settings);
                                    }
                                    ctx.request_repaint();
                                    device_clicked = true;
                                }
                            });
                        });

                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        dropdown_id,
                        !device_clicked && (device_tab_hovered || pointer_over_bridge),
                    )
                });
            } else {
                ui.ctx().data_mut(|d| d.insert_temp(dropdown_id, false));
            }
        }
    }
}
