use super::*;

impl EntropyApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_layout_settings_dropdown(
        &mut self,
        ui: &mut egui::Ui,
        layout: &KeyboardLayout,
        lang: crate::i18n::Language,
        settings_tab_rect: Option<egui::Rect>,
        device_tab_hovered: bool,
        advanced_tab_hovered: bool,
        settings_tab_hovered: bool,
    ) {
        use crate::i18n::Key as TrKey;

        if let Some(settings_rect) = settings_tab_rect {
            let dropdown_id = ui.make_persistent_id("settings_dropdown_open");
            let was_open = ui
                .ctx()
                .data(|d| d.get_temp::<bool>(dropdown_id))
                .unwrap_or(false);
            let rgb_available_for_menu = self.rgb_settings.supported || layout.supports_rgb;
            let layer_leds_available_for_menu = self.layer_led_settings.supported;
            let show_rgb_item = rgb_available_for_menu;
            let show_layer_leds_item = layer_leds_available_for_menu;
            let show_encoders_item = layout.encoder_count() > 0;
            let show_layout_options_item = layout
                .layout_options
                .iter()
                .any(|option| !Self::is_encoder_layout_option(option));
            let show_modules_item = self.module_settings.supported;
            let show_touchpad_item = self.touchpad_settings.supported;
            let show_bluetooth_item = self.bluetooth_settings.supported;
            let show_live_features_item = self.live_features_available_for_selected_device();
            let show_magic_item = self.magic_settings.supported;
            let show_tap_hold_item =
                self.tap_hold_settings.supported || self.one_shot_settings.supported;
            let show_matrix_item = self.firmware == FirmwareProtocol::Vial;
            let show_lock_item = self.firmware == FirmwareProtocol::Vial
                && self.layout.is_some()
                && !self.vial_unlock_polling
                && !self.unlock_open;
            let default_lock_label = crate::i18n::tr_catalog(lang, "ui.unlock_keyboard_action");
            let settings_item_count = 3
                + show_matrix_item as usize
                + show_rgb_item as usize
                + show_layer_leds_item as usize
                + show_encoders_item as usize
                + show_layout_options_item as usize
                + show_modules_item as usize
                + show_touchpad_item as usize
                + show_bluetooth_item as usize
                + show_live_features_item as usize
                + show_magic_item as usize
                + show_tap_hold_item as usize
                + show_lock_item as usize;
            // Keep hover bridge in sync with actual item height (30px) and frame padding.
            // Underestimating this makes lower items close the dropdown on hover.
            let dropdown_height = settings_item_count as f32 * 30.0 + 12.0;
            let mut settings_menu_labels = vec![
                crate::i18n::tr(lang, TrKey::AppSettingsTitle),
                crate::i18n::tr_catalog(lang, "notifications.title"),
                crate::i18n::tr(lang, TrKey::UniversalSymbolsTitle),
            ];
            if show_matrix_item {
                settings_menu_labels.push(crate::i18n::tr(lang, TrKey::MatrixTesterTitle));
            }
            if show_rgb_item {
                settings_menu_labels.push(crate::i18n::tr(lang, TrKey::RgbTitle));
            }
            if show_layer_leds_item {
                settings_menu_labels.push(crate::i18n::tr(lang, TrKey::LayerLedsTitle));
            }
            if show_encoders_item {
                settings_menu_labels.push(crate::i18n::tr(lang, TrKey::EncodersTitle));
            }
            if show_layout_options_item {
                settings_menu_labels.push(crate::i18n::tr(lang, TrKey::DisplayPresetsTitle));
            }
            if show_modules_item {
                settings_menu_labels.push(crate::i18n::tr_catalog(lang, "modules_settings.title"));
            }
            if show_touchpad_item {
                settings_menu_labels.push(crate::i18n::tr(lang, TrKey::TouchpadTitle));
            }
            if show_bluetooth_item {
                settings_menu_labels
                    .push(crate::i18n::tr_catalog(lang, "bluetooth_settings.title"));
            }
            if show_live_features_item {
                settings_menu_labels.push(crate::i18n::tr(lang, TrKey::LiveFeaturesTitle));
            }
            if show_magic_item {
                settings_menu_labels.push(crate::i18n::tr(lang, TrKey::MagicTitle));
            }
            if show_tap_hold_item {
                settings_menu_labels.push(crate::i18n::tr(lang, TrKey::TapHoldOneShotTitle));
            }
            if show_lock_item {
                settings_menu_labels.push(default_lock_label);
            }
            let dropdown_width = adaptive_top_dropdown_width(ui, settings_menu_labels, 184.0);
            let dropdown_rect = egui::Rect::from_min_size(
                egui::pos2(
                    settings_rect.center().x - dropdown_width / 2.0,
                    settings_rect.bottom() + 6.0,
                ),
                Vec2::new(dropdown_width, dropdown_height),
            );
            let hover_bridge_rect = settings_rect.union(dropdown_rect).expand(3.0);
            let pointer_over_bridge = ui
                .ctx()
                .input(|i| i.pointer.hover_pos())
                .map(|pos| hover_bridge_rect.contains(pos))
                .unwrap_or(false);
            let show_dropdown = !device_tab_hovered
                && !advanced_tab_hovered
                && (settings_tab_hovered || (was_open && pointer_over_bridge));

            if show_dropdown {
                let dark = ui.visuals().dark_mode;
                let rgb_available = rgb_available_for_menu;
                let mut is_unlocked = false;
                let mut lock_label = default_lock_label;
                if show_lock_item {
                    match self.hid_device.as_ref().map(|hid| hid.get_unlock_status()) {
                        Some(Ok((unlocked, _keys))) => {
                            is_unlocked = unlocked;
                            lock_label = if is_unlocked {
                                crate::i18n::tr_catalog(lang, "ui.lock_keyboard_action")
                            } else {
                                crate::i18n::tr_catalog(lang, "ui.unlock_keyboard_action")
                            };
                        }
                        Some(Err(e)) if crate::hid::is_disconnect_error(&e) => {
                            self.clear_connected_keyboard_state("Device disconnected");
                            return;
                        }
                        Some(Err(e)) => {
                            log::warn!("get_unlock_status for settings dropdown failed: {e}");
                        }
                        None => {}
                    }
                }
                let item_width = dropdown_rect.width() - 16.0;
                let (
                        app_hovered,
                        notifications_hovered,
                        matrix_hovered,
                        universal_symbols_hovered,
                        rgb_hovered,
                        layer_leds_hovered,
                        encoders_hovered,
                        layout_options_hovered,
                        modules_hovered,
                        touchpad_hovered,
                        bluetooth_hovered,
                        live_features_hovered,
                        magic_hovered,
                        tap_hold_hovered,
                        lock_hovered,
                        settings_clicked,
                    ) = egui::Area::new(egui::Id::new("settings_dropdown_area"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(dropdown_rect.min)
                        .show(ui.ctx(), |ui| {
                            top_dropdown_frame(dark)
                                .show(ui, |ui| {
                                    ui.set_min_width(item_width);
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    let app_resp = top_dropdown_item(
                                        ui,
                                        item_width,
                                        crate::i18n::tr(lang, TrKey::AppSettingsTitle),
                                        true,
                                        self.main_menu_tab == MainMenuTab::Settings
                                            && self.settings_tab == SettingsTab::AppSettings,
                                    );
                                    let notifications_resp = top_dropdown_item(
                                        ui,
                                        item_width,
                                        crate::i18n::tr_catalog(lang, "notifications.title"),
                                        true,
                                        self.main_menu_tab == MainMenuTab::Settings
                                            && self.settings_tab == SettingsTab::Notifications,
                                    );
                                    let matrix_resp = show_matrix_item.then(|| {
                                        top_dropdown_item(
                                            ui,
                                            item_width,
                                            crate::i18n::tr(lang, TrKey::MatrixTesterTitle),
                                            true,
                                            self.main_menu_tab == MainMenuTab::Settings
                                                && self.settings_tab == SettingsTab::MatrixTester,
                                        )
                                    });
                                    let universal_symbols_resp = top_dropdown_item(
                                        ui,
                                        item_width,
                                        crate::i18n::tr(lang, TrKey::UniversalSymbolsTitle),
                                        true,
                                        self.main_menu_tab == MainMenuTab::Settings
                                            && self.settings_tab
                                                == SettingsTab::UniversalSymbolsSetup,
                                    );
                                    let rgb_resp = if show_rgb_item {
                                        Some(top_dropdown_item(
                                            ui,
                                            item_width,
                                            crate::i18n::tr(lang, TrKey::RgbTitle),
                                            rgb_available,
                                            self.main_menu_tab == MainMenuTab::Settings
                                                && self.settings_tab == SettingsTab::Rgb,
                                        ))
                                    } else {
                                        None
                                    };
                                    let layer_leds_resp = show_layer_leds_item.then(|| {
                                        top_dropdown_item(
                                            ui,
                                            item_width,
                                            crate::i18n::tr(lang, TrKey::LayerLedsTitle),
                                            true,
                                            self.main_menu_tab == MainMenuTab::Settings
                                                && self.settings_tab == SettingsTab::LayerLeds,
                                        )
                                    });
                                    let encoders_resp = show_encoders_item.then(|| {
                                        top_dropdown_item(
                                            ui,
                                            item_width,
                                            crate::i18n::tr(lang, TrKey::EncodersTitle),
                                            true,
                                            self.main_menu_tab == MainMenuTab::Settings
                                                && self.settings_tab == SettingsTab::Encoders,
                                        )
                                    });
                                    let layout_options_resp = show_layout_options_item.then(|| {
                                        top_dropdown_item(
                                            ui,
                                            item_width,
                                            crate::i18n::tr(lang, TrKey::DisplayPresetsTitle),
                                            true,
                                            self.main_menu_tab == MainMenuTab::Settings
                                                && self.settings_tab == SettingsTab::LayoutOptions,
                                        )
                                    });
                                    let modules_resp = show_modules_item.then(|| {
                                        top_dropdown_item(
                                            ui,
                                            item_width,
                                            crate::i18n::tr_catalog(lang, "modules_settings.title"),
                                            true,
                                            self.main_menu_tab == MainMenuTab::Settings
                                                && self.settings_tab == SettingsTab::Modules,
                                        )
                                    });
                                    let touchpad_resp = show_touchpad_item.then(|| {
                                        top_dropdown_item(
                                            ui,
                                            item_width,
                                            crate::i18n::tr(lang, TrKey::TouchpadTitle),
                                            true,
                                            self.main_menu_tab == MainMenuTab::Settings
                                                && self.settings_tab == SettingsTab::Touchpad,
                                        )
                                    });
                                    let bluetooth_resp = show_bluetooth_item.then(|| {
                                        top_dropdown_item(
                                            ui,
                                            item_width,
                                            crate::i18n::tr_catalog(
                                                lang,
                                                "bluetooth_settings.title",
                                            ),
                                            true,
                                            self.main_menu_tab == MainMenuTab::Settings
                                                && self.settings_tab == SettingsTab::Bluetooth,
                                        )
                                    });
                                    let live_features_resp = show_live_features_item.then(|| {
                                        top_dropdown_item(
                                            ui,
                                            item_width,
                                            crate::i18n::tr(lang, TrKey::LiveFeaturesTitle),
                                            true,
                                            self.main_menu_tab == MainMenuTab::Settings
                                                && self.settings_tab == SettingsTab::LiveFeatures,
                                        )
                                    });
                                    let magic_resp = show_magic_item.then(|| {
                                        top_dropdown_item(
                                            ui,
                                            item_width,
                                            crate::i18n::tr(lang, TrKey::MagicTitle),
                                            true,
                                            self.main_menu_tab == MainMenuTab::Settings
                                                && self.settings_tab == SettingsTab::Magic,
                                        )
                                    });
                                    let tap_hold_resp = show_tap_hold_item.then(|| {
                                        top_dropdown_item(
                                            ui,
                                            item_width,
                                            crate::i18n::tr(lang, TrKey::TapHoldOneShotTitle),
                                            true,
                                            self.main_menu_tab == MainMenuTab::Settings
                                                && self.settings_tab == SettingsTab::TapHold,
                                        )
                                    });
                                    let lock_resp = show_lock_item.then(|| {
                                        top_dropdown_item(ui, item_width, lock_label, true, false)
                                    });
                                    if app_resp.clicked() {
                                        self.close_top_dropdowns(ui.ctx());
                                        self.open_app_settings_page();
                                    }
                                    if notifications_resp.clicked() {
                                        self.close_top_dropdowns(ui.ctx());
                                        self.open_notifications_settings_page();
                                    }
                                    if matrix_resp.as_ref().map(|r| r.clicked()).unwrap_or(false) {
                                        self.close_top_dropdowns(ui.ctx());
                                        self.settings_tab = SettingsTab::MatrixTester;
                                        if self.main_menu_tab != MainMenuTab::Settings {
                                            self.reset_matrix_tester_state();
                                        }
                                        self.matrix_tester_unlock_prompted = false;
                                        self.matrix_tester_lock_checked = false;
                                        self.main_menu_tab = MainMenuTab::Settings;
                                    }
                                    if universal_symbols_resp.clicked() {
                                        self.close_top_dropdowns(ui.ctx());
                                        self.open_universal_symbols_setup_page();
                                    }
                                    if let Some(rgb_resp) = &rgb_resp {
                                        if rgb_resp.clicked() && rgb_available {
                                            self.close_top_dropdowns(ui.ctx());
                                            self.settings_tab = SettingsTab::Rgb;
                                            self.main_menu_tab = MainMenuTab::Settings;
                                        }
                                        if !rgb_available {
                                            let _ = rgb_resp.clone().on_hover_text(
                                                crate::i18n::tr(lang, TrKey::RgbUnavailableTooltip),
                                            );
                                        }
                                    }
                                    if layer_leds_resp
                                        .as_ref()
                                        .map(|r| r.clicked())
                                        .unwrap_or(false)
                                    {
                                        self.close_top_dropdowns(ui.ctx());
                                        self.open_layer_led_settings_page();
                                    }
                                    if encoders_resp.as_ref().map(|r| r.clicked()).unwrap_or(false)
                                    {
                                        self.close_top_dropdowns(ui.ctx());
                                        self.settings_tab = SettingsTab::Encoders;
                                        self.main_menu_tab = MainMenuTab::Settings;
                                    }
                                    if layout_options_resp
                                        .as_ref()
                                        .map(|r| r.clicked())
                                        .unwrap_or(false)
                                    {
                                        self.close_top_dropdowns(ui.ctx());
                                        self.open_layout_options_settings_page();
                                    }
                                    if modules_resp.as_ref().map(|r| r.clicked()).unwrap_or(false) {
                                        self.close_top_dropdowns(ui.ctx());
                                        self.open_modules_settings_page();
                                    }
                                    if touchpad_resp.as_ref().map(|r| r.clicked()).unwrap_or(false)
                                    {
                                        self.close_top_dropdowns(ui.ctx());
                                        self.open_touchpad_settings_page();
                                    }
                                    if bluetooth_resp
                                        .as_ref()
                                        .map(|r| r.clicked())
                                        .unwrap_or(false)
                                    {
                                        self.close_top_dropdowns(ui.ctx());
                                        self.open_bluetooth_settings_page();
                                    }
                                    if live_features_resp
                                        .as_ref()
                                        .map(|r| r.clicked())
                                        .unwrap_or(false)
                                    {
                                        self.close_top_dropdowns(ui.ctx());
                                        self.open_live_features_settings_page();
                                    }
                                    if magic_resp.as_ref().map(|r| r.clicked()).unwrap_or(false) {
                                        self.close_top_dropdowns(ui.ctx());
                                        self.open_magic_settings_page();
                                    }
                                    if tap_hold_resp.as_ref().map(|r| r.clicked()).unwrap_or(false)
                                    {
                                        self.close_top_dropdowns(ui.ctx());
                                        self.open_tap_hold_settings_page();
                                    }
                                    if lock_resp.as_ref().map(|r| r.clicked()).unwrap_or(false) {
                                        self.close_top_dropdowns(ui.ctx());
                                        if is_unlocked {
                                            if let Some(hid) = &self.hid_device {
                                                let layout_indicator_was_open =
                                                    self.app_settings.sticky_layout_window;
                                                match hid.lock() {
                                                    Ok(()) => {
                                                        if layout_indicator_was_open {
                                                            self.status_msg = crate::i18n::tr_catalog(
                                                                self.app_settings.language,
                                                                "ui.sticky_layout_closed_due_to_lock",
                                                            )
                                                            .into();
                                                            self.app_settings.sticky_layout_window = false;
                                                            self.pending_layout_indicator_open_after_unlock = false;
                                                            self.sticky_layout_last_size = None;
                                                            save_app_settings(&self.app_settings);
                                                        } else {
                                                            self.status_msg = "Device locked".into();
                                                        }
                                                    }
                                                    Err(e) => {
                                                        self.status_msg = format!("Lock failed: {e}")
                                                    }
                                                }
                                            }
                                        } else {
                                            self.unlock_open = true;
                                        }
                                    }
                                    (
                                        app_resp.hovered(),
                                        notifications_resp.hovered(),
                                        matrix_resp.as_ref().map(|r| r.hovered()).unwrap_or(false),
                                        universal_symbols_resp.hovered(),
                                        rgb_resp
                                            .as_ref()
                                            .map(|resp| resp.hovered())
                                            .unwrap_or(false),
                                        layer_leds_resp
                                            .as_ref()
                                            .map(|r| r.hovered())
                                            .unwrap_or(false),
                                        encoders_resp
                                            .as_ref()
                                            .map(|r| r.hovered())
                                            .unwrap_or(false),
                                        layout_options_resp
                                            .as_ref()
                                            .map(|r| r.hovered())
                                            .unwrap_or(false),
                                        modules_resp.as_ref().map(|r| r.hovered()).unwrap_or(false),
                                        touchpad_resp
                                            .as_ref()
                                            .map(|r| r.hovered())
                                            .unwrap_or(false),
                                        bluetooth_resp
                                            .as_ref()
                                            .map(|r| r.hovered())
                                            .unwrap_or(false),
                                        live_features_resp
                                            .as_ref()
                                            .map(|r| r.hovered())
                                            .unwrap_or(false),
                                        magic_resp.as_ref().map(|r| r.hovered()).unwrap_or(false),
                                        tap_hold_resp
                                            .as_ref()
                                            .map(|r| r.hovered())
                                            .unwrap_or(false),
                                        lock_resp.as_ref().map(|r| r.hovered()).unwrap_or(false),
                                        app_resp.clicked()
                                            || notifications_resp.clicked()
                                            || matrix_resp
                                                .as_ref()
                                                .map(|r| r.clicked())
                                                .unwrap_or(false)
                                            || universal_symbols_resp.clicked()
                                            || rgb_resp
                                                .as_ref()
                                                .map(|resp| resp.clicked() && rgb_available)
                                                .unwrap_or(false)
                                            || layer_leds_resp
                                                .as_ref()
                                                .map(|r| r.clicked())
                                                .unwrap_or(false)
                                            || encoders_resp
                                                .as_ref()
                                                .map(|r| r.clicked())
                                                .unwrap_or(false)
                                            || layout_options_resp
                                                .as_ref()
                                                .map(|r| r.clicked())
                                                .unwrap_or(false)
                                            || modules_resp
                                                .as_ref()
                                                .map(|r| r.clicked())
                                                .unwrap_or(false)
                                            || touchpad_resp
                                                .as_ref()
                                                .map(|r| r.clicked())
                                                .unwrap_or(false)
                                            || bluetooth_resp
                                                .as_ref()
                                                .map(|r| r.clicked())
                                                .unwrap_or(false)
                                            || live_features_resp
                                                .as_ref()
                                                .map(|r| r.clicked())
                                                .unwrap_or(false)
                                            || magic_resp
                                                .as_ref()
                                                .map(|r| r.clicked())
                                                .unwrap_or(false)
                                            || tap_hold_resp
                                                .as_ref()
                                                .map(|r| r.clicked())
                                                .unwrap_or(false)
                                            || lock_resp.as_ref().map(|r| r.clicked()).unwrap_or(false),
                                    )
                                })
                                .inner
                        })
                        .inner;
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        dropdown_id,
                        !settings_clicked
                            && (settings_tab_hovered
                                || app_hovered
                                || notifications_hovered
                                || matrix_hovered
                                || universal_symbols_hovered
                                || rgb_hovered
                                || layer_leds_hovered
                                || encoders_hovered
                                || layout_options_hovered
                                || modules_hovered
                                || touchpad_hovered
                                || bluetooth_hovered
                                || live_features_hovered
                                || magic_hovered
                                || tap_hold_hovered
                                || lock_hovered
                                || pointer_over_bridge),
                    )
                });
            } else {
                ui.ctx().data_mut(|d| d.insert_temp(dropdown_id, false));
            }
        }
    }
}
