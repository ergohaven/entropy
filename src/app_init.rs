use super::*;

impl EntropyApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app_settings = load_app_settings();
        app_settings.ui_scale = clamp_ui_scale(app_settings.ui_scale);
        crate::diagnostics::set_enabled(app_settings.diagnostics_enabled);
        cc.egui_ctx.set_zoom_factor(app_settings.ui_scale);
        crate::ui_style::set_accent(app_settings.accent_color.color());
        crate::smart_input::set_text_expander_config(
            app_settings.text_expander_enabled,
            {
                let mut rules = app_settings.text_expansion_rules.clone();
                rules.extend(load_extra_text_expansion_rules(
                    &app_settings.text_expander_rule_files,
                ));
                rules
            },
            parse_text_expander_blacklist(&app_settings.text_expander_app_blacklist),
        );
        #[cfg(target_os = "linux")]
        crate::smart_input::refresh_installed_ibus_backend();
        crate::smart_input::start();

        let text_expander_rules_signature =
            text_expander_rules_signature(&app_settings.text_expander_rule_files);
        let typing_trainer = TypingTrainerState::from_settings(app_settings.typing_trainer);
        let dark_mode = app_settings.dark_mode;

        Self {
            #[cfg(not(target_arch = "wasm32"))]
            hid_device: None,
            #[cfg(not(target_arch = "wasm32"))]
            shared_hid_output: None,
            #[cfg(not(target_arch = "wasm32"))]
            layer_write_task: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_layer_write: None,
            qmk_settings_write_queue: QmkSettingsWriteQueue::default(),
            #[cfg(not(target_arch = "wasm32"))]
            combo_write_task: None,
            settings_write_task: None,
            #[cfg(not(target_arch = "wasm32"))]
            vial_hid_task: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_layout_undo: false,
            #[cfg(not(target_arch = "wasm32"))]
            deferred_device_load: DeferredDeviceLoadState::default(),
            #[cfg(not(target_arch = "wasm32"))]
            deferred_full_layout_action: None,
            settings_write_queue: SettingsWriteQueueState::default(),
            settings_write_generation: 0,
            #[cfg(not(target_arch = "wasm32"))]
            qmk_hid_hosts: std::collections::HashMap::new(),
            pending_device_connect: None,
            firmware: FirmwareProtocol::Vial,
            #[cfg(not(target_arch = "wasm32"))]
            supported_qmk_settings: Vec::new(),
            undo_stack: Vec::new(),
            layer_clipboard: None,
            scan_frame: 0,
            last_device_scan_at: 0.0,
            #[cfg(not(target_arch = "wasm32"))]
            next_battery_refresh_at: None,
            hover_layer: None,
            last_layout_geometry: None,
            prev_hovered_key: None,
            prev_hovered_encoder: false,
            prev_hovered_encoder_keycode: None,
            secondary_click_handled: false,
            pending_handed_swap: None,
            hover_layer_progress: 0.0,
            jump_back_stack: Vec::new(),
            device_manager: DeviceManager::new(),
            selected_device: None,
            selected_layer: 0,
            selected_key: None,
            selected_encoder: None,
            layout: None,
            layer_count: 4,
            keycode_picker: KeycodePicker::default(),
            status_msg: String::new(),
            import_report_open: false,
            import_report_title: String::new(),
            import_report_body: String::new(),
            pending_entlayout_import_path: None,
            pending_file_dialog: None,
            connection_generation: 0,
            parent_window_handle: None,
            parent_display_handle: None,
            pending_entsettings_import_path: None,
            import_progress_started_at: None,
            import_progress_title: String::new(),
            import_progress_body: String::new(),
            dark_mode,
            last_applied_theme: None,
            app_settings,
            text_expander_rules_signature,
            text_expander_rules_last_check_at: 0.0,
            text_expander_settings_save_pending: false,
            text_expander_settings_last_edit_at: 0.0,
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            tray_icon: None,
            #[cfg(target_os = "windows")]
            windows_hwnd: None,
            #[cfg(target_os = "windows")]
            windows_window_hidden_to_tray: false,
            #[cfg(target_os = "macos")]
            macos_ns_window: None,
            #[cfg(target_os = "macos")]
            macos_window_hidden_to_menu_bar: false,
            #[cfg(target_os = "macos")]
            macos_app_was_hidden: false,
            #[cfg(target_os = "macos")]
            macos_hidden_to_menu_bar_at: None,
            close_to_tray_prompt_open: false,
            close_to_tray_prompt_remember: false,
            force_close_requested: false,
            #[cfg(not(target_arch = "wasm32"))]
            exit_after_hid_write: false,
            main_menu_tab: MainMenuTab::Keyboard,
            combo_entries: vec![],
            combo_synced_entries: vec![],
            combo_names: vec![],
            combo_colors: vec![],
            selected_combo: 0,
            combo_dirty: false,
            combo_edit_revision: 0,
            combo_attempted_revision: None,
            combo_names_dirty: false,
            combo_colors_dirty: false,
            combo_term: None,
            auto_shift_options: AutoShiftOptionsState::default(),
            auto_shift_timeout: None,
            auto_shift_timeout_text: String::new(),
            mouse_keys_settings: MouseKeysSettingsState::default(),
            touchpad_settings: TouchpadSettingsState::default(),
            bluetooth_settings: BluetoothSettingsState::default(),
            module_settings: ModuleSettingsState::default(),
            tap_hold_settings: TapHoldSettingsState::default(),
            pending_tap_hold_numeric_writes: std::collections::BTreeMap::new(),
            tap_hold_numeric_write_due: None,
            magic_settings: MagicSettingsState::default(),
            one_shot_settings: OneShotSettingsState::default(),
            grave_escape_settings: GraveEscapeSettingsState::default(),
            layer_led_settings: LayerLedSettingsState::default(),
            alt_repeat_entries: vec![],
            alt_repeat_names: vec![],
            alt_repeat_undo_stack: Vec::new(),
            selected_alt_repeat: 0,
            alt_repeat_visible_count: 1,
            alt_repeat_pick_target: None,
            last_single_instance_signal: read_single_instance_signal(),
            rgb_settings: RgbSettingsState::default(),
            layout_options_value: None,
            encoder_visibility: vec![],
            combo_term_dirty: false,
            combo_visible_count: 1,
            combo_undo_stack: Vec::new(),
            combo_pick_target: None,
            key_override_entries: Vec::new(),
            key_override_names: vec![],
            key_override_dirty: false,
            key_override_visible_count: 1,
            key_override_undo_stack: Vec::new(),
            text_expander_deleted_rules: Vec::new(),
            typing_trainer,
            typing_trainer_history_open: false,
            selected_key_override: 0,
            key_override_pick_target: None,
            matrix_tester_pressed: Vec::new(),
            matrix_tester_ever_pressed: Vec::new(),
            matrix_tester_rmk_byte_order: false,
            sticky_layout_prev_pressed: Vec::new(),
            sticky_layout_pressed_key_layers: Vec::new(),
            sticky_layout_toggled_layers: Vec::new(),
            sticky_layout_active_combos: Vec::new(),
            sticky_layout_tap_dance_states: Vec::new(),
            sticky_layout_base_layer: 0,
            sticky_layout_active_layer: 0,
            sticky_layout_last_size: None,
            sticky_layout_resize_opacity_hold_frames: 0,
            pending_layout_indicator_open_after_unlock: false,
            matrix_tester_last_poll: std::time::Instant::now(),
            matrix_tester_last_lock_check: std::time::Instant::now()
                - MATRIX_TESTER_LOCK_CHECK_INTERVAL,
            matrix_tester_unlock_prompted: false,
            matrix_tester_lock_checked: false,
            macro_auto_unlock_cancelled: false,
            settings_tab: SettingsTab::MatrixTester,
            layer_names: load_layer_names("default"),
            editing_layer: None,
            editing_layer_text: String::new(),
            editing_layer_focus_requested: false,
            current_device_name: String::new(),
            current_keyboard_id: None,
            current_encoder_visibility_id: String::new(),
            device_display_names: std::collections::HashMap::new(),
            device_about_info: None,
            update_check: start_update_check(),
            tour_state: TourState::default(),
            tour_target_rects: Vec::new(),
            unlock_open: false,
            vial_unlocked: None,
            vial_unlock_keys: vec![],
            vial_unlock_polling: false,
            vial_unlock_counter: 0,
            vial_unlock_best: 50,
            vial_unlock_total: 50,
            vial_unlock_last_poll: None,
            vial_unlock_animation_nonce: 0,
            #[cfg(not(target_arch = "wasm32"))]
            connect_state: ConnectState::Idle,
            #[cfg(not(target_arch = "wasm32"))]
            device_scan_state: DeviceScanState::Idle,
        }
    }

    /// Recompute which layers contain assignable keycodes.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn refresh_layer_picker_content_flags(&mut self) {
        if let Some(layout) = &self.layout {
            self.keycode_picker.layer_has_content = layout
                .layers
                .iter()
                .map(|keys| {
                    keys.iter().any(|binding| {
                        let kc = binding.vial_keycode();
                        binding.rmk_action().is_some() || (kc != 0x0000 && kc != 0x0001)
                    })
                })
                .collect();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn is_vial_locked(&self) -> bool {
        self.firmware == FirmwareProtocol::Vial
            && self.layout.is_some()
            && !self.vial_unlock_polling
            && self.vial_unlocked != Some(true)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn is_vial_locked(&self) -> bool {
        false
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn reopen_vial_hid(&mut self) {
        let Some(dev) = self
            .selected_device
            .and_then(|idx| self.device_manager.devices().get(idx))
        else {
            self.hid_device = None;
            self.shared_hid_output = None;
            return;
        };
        match crate::hid::HidDevice::open_fresh_for(dev) {
            Ok(hid) => {
                self.shared_hid_output = hid.shared_output();
                self.hid_device = Some(hid);
            }
            Err(e) => {
                self.hid_device = None;
                self.shared_hid_output = None;
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "status_messages.hid_reopen_failed",
                    &[("error", &e.to_string())],
                );
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn cancel_vial_unlock(&mut self, suppress_macro_auto_unlock: bool) {
        if let Some(hid) = &self.hid_device {
            match hid.lock() {
                Ok(()) => {
                    self.vial_unlocked = Some(false);
                    self.status_msg = crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "status_messages.device_unlock_cancelled",
                    )
                    .into();
                }
                Err(e) => {
                    self.status_msg = crate::i18n::tr_catalog_format(
                        self.app_settings.language,
                        "status_messages.cancel_unlock_failed",
                        &[("error", &e.to_string())],
                    );
                }
            }
        }
        self.unlock_open = false;
        self.vial_unlock_polling = false;
        self.vial_unlock_last_poll = None;
        self.pending_layout_indicator_open_after_unlock = false;
        self.vial_unlock_counter = 0;
        self.vial_unlock_best = 50;
        self.matrix_tester_unlock_prompted = false;
        self.matrix_tester_lock_checked = false;
        if suppress_macro_auto_unlock {
            self.macro_auto_unlock_cancelled = true;
        }
    }
}
