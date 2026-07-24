use super::*;
#[cfg(not(target_arch = "wasm32"))]
use super::vial_hid_task::VialHidTaskStart;

fn about_entropy_label(lang: crate::i18n::Language) -> &'static str {
    match lang {
        crate::i18n::Language::Russian => "Об Entropy",
        crate::i18n::Language::English => "About Entropy",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VialLockMenuState {
    visible: bool,
    enabled: bool,
}

fn vial_lock_menu_state(
    vial_firmware: bool,
    layout_loaded: bool,
    unlock_polling: bool,
    unlock_open: bool,
    vial_hid_idle: bool,
) -> VialLockMenuState {
    let visible = vial_firmware && layout_loaded && !unlock_polling && !unlock_open;
    VialLockMenuState {
        visible,
        enabled: visible && vial_hid_idle,
    }
}

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
            let show_encoders_item = self.show_separate_encoder_visibility_settings(layout);
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
            let show_update_indicator = crate::app::update_available(&self.update_check);
            let show_matrix_item = self.firmware == FirmwareProtocol::Vial;
            #[cfg(not(target_arch = "wasm32"))]
            let vial_hid_idle = !self.vial_hid_task_active();
            #[cfg(target_arch = "wasm32")]
            let vial_hid_idle = true;
            let lock_menu_state = vial_lock_menu_state(
                self.firmware == FirmwareProtocol::Vial,
                self.layout.is_some(),
                self.vial_unlock_polling,
                self.unlock_open,
                vial_hid_idle,
            );
            let show_lock_item = lock_menu_state.visible;
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
            settings_menu_labels.push(about_entropy_label(lang));
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
                let is_unlocked = self.vial_unlocked == Some(true);
                let lock_label = if is_unlocked {
                    crate::i18n::tr_catalog(lang, "ui.lock_keyboard_action")
                } else {
                    default_lock_label
                };
                let item_width = dropdown_rect.width() - 16.0;
                let (
                        app_hovered,
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
                        about_entropy_hovered,
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
                                        top_dropdown_item(
                                            ui,
                                            item_width,
                                            lock_label,
                                            lock_menu_state.enabled,
                                            false,
                                        )
                                    });
                                    let about_entropy_resp = top_dropdown_item_with_indicator(
                                        ui,
                                        item_width,
                                        about_entropy_label(lang),
                                        true,
                                        self.main_menu_tab == MainMenuTab::Settings
                                            && self.settings_tab == SettingsTab::AboutEntropy,
                                        show_update_indicator,
                                    );
                                    if app_resp.clicked() {
                                        self.close_top_dropdowns(ui.ctx());
                                        self.open_app_settings_page();
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
                                            #[cfg(not(target_arch = "wasm32"))]
                                            if self.start_vial_lock(ui.ctx())
                                                == VialHidTaskStart::NoDevice
                                            {
                                                self.status_msg =
                                                    crate::i18n::tr_catalog_format(
                                                        self.app_settings.language,
                                                        "dynamic_status.lock_failed",
                                                        &[(
                                                            "error",
                                                            crate::i18n::tr_catalog(
                                                                self.app_settings.language,
                                                                "status_messages.device_unavailable",
                                                            ),
                                                        )],
                                                    );
                                            }
                                        } else {
                                            self.unlock_open = true;
                                        }
                                    }
                                    if about_entropy_resp.clicked() {
                                        self.close_top_dropdowns(ui.ctx());
                                        self.open_about_entropy_page();
                                    }
                                    (
                                        app_resp.hovered(),
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
                                        about_entropy_resp.hovered(),
                                        app_resp.clicked()
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
                                            || lock_resp.as_ref().map(|r| r.clicked()).unwrap_or(false)
                                            || about_entropy_resp.clicked(),
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
                                || about_entropy_hovered
                                || pointer_over_bridge),
                    )
                });
            } else {
                ui.ctx().data_mut(|d| d.insert_temp(dropdown_id, false));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_vial_worker_keeps_lock_item_visible() {
        assert_eq!(
            vial_lock_menu_state(true, true, false, false, false),
            VialLockMenuState {
                visible: true,
                enabled: false,
            }
        );
    }

    #[test]
    fn idle_vial_worker_enables_lock_item() {
        assert_eq!(
            vial_lock_menu_state(true, true, false, false, true),
            VialLockMenuState {
                visible: true,
                enabled: true,
            }
        );
    }

    #[test]
    fn active_unlock_flow_hides_lock_item() {
        assert!(!vial_lock_menu_state(true, true, true, false, true).visible);
        assert!(!vial_lock_menu_state(true, true, false, true, true).visible);
    }
}
