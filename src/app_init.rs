use super::*;

#[cfg(not(target_arch = "wasm32"))]
fn capture_import_backup_namespace() -> Result<entlayout::backup_durability::BackupNamespace, String> {
    // Shipping startup captures this before single-instance/diagnostic storage
    // can mkdir. Tests intentionally resolve their thread-local scoped root.
    #[cfg(not(test))]
    {
        static NAMESPACE: std::sync::OnceLock<
            Result<entlayout::backup_durability::BackupNamespace, String>,
        > = std::sync::OnceLock::new();
        NAMESPACE.get_or_init(resolve_import_backup_namespace).clone()
    }
    #[cfg(test)]
    resolve_import_backup_namespace()
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_import_backup_namespace() -> Result<entlayout::backup_durability::BackupNamespace, String> {
    let root = storage_config_dir().ok_or_else(|| "No backup storage directory".to_owned())?;
    let namespace = entlayout::backup_durability::BackupNamespace::resolve(&root)
        .map_err(|error| error.to_string())?;
    Ok(namespace)
}

// Bootstrap effects are resolved once by `new`; both real and inert callers
// then use the same application-state initializer.
struct AppBootstrap {
    #[cfg(not(target_arch = "wasm32"))]
    import_backup_namespace: Result<entlayout::backup_durability::BackupNamespace, String>,
    app_settings: AppSettings,
    device_manager: DeviceManager,
    typing_trainer: TypingTrainerState,
    text_expander_rules_signature: Vec<(String, Option<std::time::SystemTime>)>,
    last_single_instance_signal: String,
    layer_names: Vec<String>,
    update_check: UpdateCheckState,
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn initialize_import_backup_namespace() {
        let _ = capture_import_backup_namespace();
    }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::new_with_context(&cc.egui_ctx)
    }

    pub(crate) fn new_with_context(ctx: &egui::Context) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let import_backup_namespace = capture_import_backup_namespace();
        let mut app_settings = load_app_settings();
        app_settings.ui_scale = clamp_ui_scale(app_settings.ui_scale);
        crate::diagnostics::set_enabled(app_settings.diagnostics_enabled);
        ctx.set_zoom_factor(app_settings.ui_scale);
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
        let mut typing_trainer = TypingTrainerState::from_settings(app_settings.typing_trainer);
        typing_trainer.set_symbol_stats(load_typing_trainer_symbol_stats());
        let update_check = start_update_check(app_settings.development_updates);
        let mut app = Self::from_bootstrap(AppBootstrap {
            #[cfg(not(target_arch = "wasm32"))]
            import_backup_namespace,
            app_settings,
            device_manager: DeviceManager::new(),
            typing_trainer,
            text_expander_rules_signature,
            last_single_instance_signal: read_single_instance_signal(),
            layer_names: load_layer_names("default"),
            update_check,
        });
        #[cfg(unix)]
        app.start_console_export_server(ctx);
        #[cfg(not(target_arch = "wasm32"))]
        app.update_notifications.initialize(
            ctx,
            app_settings_path().with_file_name("update-notifications.json"),
        );
        app
    }

    /// The app without a window, for `--export-layout`. Reads the same
    /// settings as `new` (language, diagnostics) but starts none of the
    /// desktop machinery: no text expander, no update check, no zoom, no
    /// device scan of its own.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn new_headless() -> Self {
        let import_backup_namespace = capture_import_backup_namespace();
        let app_settings = load_app_settings();
        crate::diagnostics::set_enabled(app_settings.diagnostics_enabled);
        let mut app = Self::from_bootstrap(AppBootstrap {
            #[cfg(not(target_arch = "wasm32"))]
            import_backup_namespace,
            typing_trainer: TypingTrainerState::from_settings(app_settings.typing_trainer),
            app_settings,
            device_manager: DeviceManager::empty(),
            text_expander_rules_signature: Vec::new(),
            last_single_instance_signal: String::new(),
            layer_names: Vec::new(),
            update_check: UpdateCheckState::Idle,
        });
        app.headless = true;
        app
    }

    #[cfg(test)]
    pub(crate) fn new_inert_for_test() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let import_backup_namespace = capture_import_backup_namespace();
        let app_settings = AppSettings::default();
        Self::from_bootstrap(AppBootstrap {
            #[cfg(not(target_arch = "wasm32"))]
            import_backup_namespace,
            typing_trainer: TypingTrainerState::from_settings(app_settings.typing_trainer),
            app_settings,
            device_manager: DeviceManager::empty(),
            text_expander_rules_signature: Vec::new(),
            last_single_instance_signal: String::new(),
            layer_names: Vec::new(),
            update_check: UpdateCheckState::Idle,
        })
    }

    /// Logic/HID fixtures opt out of desktop bootstrap and route discovery to
    /// the existing scan channel. Keep the receiver alive while ticking; tests
    /// can assert that discovery was attempted without opening native HID.
    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) fn new_isolated_hid_for_test() -> (
        Self,
        mpsc::Receiver<(bool, mpsc::Sender<Result<Vec<Device>, String>>)>,
    ) {
        let mut app = Self::new_inert_for_test();
        let (requests, scans) = mpsc::channel();
        app.test_device_scan_requests = Some(requests);
        assert!(
            matches!(app.update_check, UpdateCheckState::Idle),
            "HID fixtures must not start network/bootstrap services"
        );
        assert!(app.device_manager.devices().is_empty());
        assert!(
            app.test_device_scan_requests.is_some(),
            "HID fixtures must intercept platform discovery"
        );
        (app, scans)
    }

    fn from_bootstrap(bootstrap: AppBootstrap) -> Self {
        let AppBootstrap {
            #[cfg(not(target_arch = "wasm32"))]
            import_backup_namespace,
            app_settings,
            device_manager,
            typing_trainer,
            text_expander_rules_signature,
            last_single_instance_signal,
            layer_names,
            update_check,
        } = bootstrap;
        let dark_mode = app_settings.dark_mode;

        Self {
            #[cfg(not(target_arch = "wasm32"))]
            import_backup_namespace,
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
            sm62_fps: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_layout_undo: false,
            #[cfg(not(target_arch = "wasm32"))]
            deferred_device_load: DeferredDeviceLoadState::default(),
            #[cfg(not(target_arch = "wasm32"))]
            deferred_full_layout_action: None,
            #[cfg(unix)]
            console_export_server: None,
            #[cfg(unix)]
            pending_console_export: None,
            settings_write_queue: SettingsWriteQueueState::default(),
            settings_write_generation: 0,
            #[cfg(not(target_arch = "wasm32"))]
            qmk_hid_hosts: std::collections::HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            rynk_hosts: std::collections::HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            rynk_host_opens: std::collections::HashMap::new(),
            application_layout_editor_active: false,
            #[cfg(not(target_arch = "wasm32"))]
            application_discovery: Default::default(),
            #[cfg(target_os = "linux")]
            gnome_integration_install_task: None,
            #[cfg(target_os = "linux")]
            gnome_integration_install_result: None,
            #[cfg(not(target_arch = "wasm32"))]
            foreground_repaint_subscription: None,
            #[cfg(not(target_arch = "wasm32"))]
            application_layout_foreground: None,
            #[cfg(not(target_arch = "wasm32"))]
            persisted_application_libraries: std::collections::HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            application_layout_edit_owners: std::collections::BTreeMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            application_layout_observed_edits: app_settings.application_layouts.iter()
                .flat_map(|(key, settings)| settings.layouts.values()
                    .map(move |p| ((key.clone(), p.id.clone()), p.user_edit_revision))).collect(),
            #[cfg(not(target_arch = "wasm32"))]
            application_layout_owner_intents: std::collections::BTreeMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            application_layout_manual_override: None,
            #[cfg(not(target_arch = "wasm32"))]
            default_layout_sync_retry_after: None,
            #[cfg(not(target_arch = "wasm32"))]
            default_layout_device_reconciled: false,
            #[cfg(not(target_arch = "wasm32"))]
            default_layout_pending_layers: 0,
            application_picker_open: false,
            application_picker_assign_existing: false,
            application_picker_target_layout_id: None,
            application_picker_search: String::new(),
            application_picker_selected: None,
            application_picker_name: String::new(),
            application_picker_category:
                crate::application_layouts::ApplicationLayoutCategory::Other,
            application_picker_category_changed: false,
            application_picker_custom_category_id: None,
            application_categories_open: false,
            application_categories_selected_id: None,
            application_categories_name: String::new(),
            pending_device_connect: None,
            remembered_device: None,
            pending_takeover: None,
            #[cfg(not(target_arch = "wasm32"))]
            headless: false,
            firmware: FirmwareProtocol::Vial,
            #[cfg(not(target_arch = "wasm32"))]
            supported_qmk_settings: Vec::new(),
            undo_stack: Vec::new(),
            layer_clipboard: None,
            scan_frame: 0,
            last_device_scan_at: 0.0,
            last_full_device_scan_at: 0.0,
            #[cfg(not(target_arch = "wasm32"))]
            next_battery_refresh_at: None,
            hover_layer: None,
            last_layout_geometry: None,
            prev_hovered_key: None,
            prev_hovered_encoder: false,
            prev_hovered_encoder_keycode: None,
            secondary_click_handled: false,
            pending_handed_swap: None,
            pending_middle_click_assignments: Vec::new(),
            hover_layer_progress: 0.0,
            jump_back_stack: Vec::new(),
            device_manager,
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
            entlayout_losses_prompt: None,
            entlayout_losses_accepted: None,
            pending_file_dialog: None,
            pending_entlayout_dialog_target: None,
            #[cfg(not(target_arch = "wasm32"))]
            gui_entlayout_preparation: None,
            connection_generation: 0,
            parent_window_handle: None,
            parent_display_handle: None,
            pending_entsettings_import_path: None,
            #[cfg(target_os = "linux")]
            ibus_registration: Default::default(),
            #[cfg(target_os = "linux")]
            pending_ibus_reload: None,
            #[cfg(target_os = "linux")]
            text_expander_probe: None,
            #[cfg(target_os = "linux")]
            pending_text_expander_probe: None,
            #[cfg(target_os = "linux")]
            text_expander_ibus_modal: None,
            import_progress_started_at: None,
            import_progress_title: String::new(),
            import_progress_body: String::new(),
            #[cfg(target_os = "linux")]
            linux_setup_task: None,
            dark_mode,
            last_applied_theme: None,
            native_time_offset: 0.0,
            app_settings,
            app_clock: std::time::Instant::now(),
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
            #[cfg(target_os = "windows")]
            windows_start_hidden_to_tray_pending: false,
            #[cfg(target_os = "macos")]
            macos_ns_window: None,
            #[cfg(target_os = "macos")]
            macos_window_hidden_to_menu_bar: false,
            #[cfg(target_os = "macos")]
            macos_app_was_hidden: false,
            #[cfg(target_os = "macos")]
            macos_hidden_to_menu_bar_at: None,
            #[cfg(target_os = "linux")]
            linux_background_since: None,
            #[cfg(target_os = "linux")]
            linux_background_focus_left: false,
            #[cfg(target_os = "linux")]
            linux_tray: None,
            #[cfg(target_os = "linux")]
            linux_hidden_to_tray: false,
            #[cfg(target_os = "linux")]
            linux_start_hidden_to_tray_pending: false,
            close_to_tray_prompt_open: false,
            close_to_tray_prompt_remember: false,
            force_close_requested: false,
            #[cfg(not(target_arch = "wasm32"))]
            exit_after_hid_write: false,
            main_menu_tab: MainMenuTab::Keyboard,
            combo_entries: vec![],
            combo_synced_entries: vec![],
            supports_rmk_combo_layers: false,
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
            last_single_instance_signal,
            rgb_settings: RgbSettingsState::default(),
            display_settings: DisplaySettingsState::default(),
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
            text_expander_rule_editor: None,
            text_expander_rule_editor_focus_pending: false,
            text_expander_emoji_search: String::new(),
            text_expander_emoji_group: 0,
            text_expander_emoji_target: None,
            typing_trainer,
            typing_trainer_symbol_pool_source: None,
            typing_trainer_history_open: false,
            selected_key_override: 0,
            key_override_pick_target: None,
            matrix_tester_pressed: Vec::new(),
            matrix_tester_ever_pressed: Vec::new(),
            sticky_layout_prev_pressed: Vec::new(),
            sticky_layout_pressed_key_layers: Vec::new(),
            sticky_layout_toggled_layers: Vec::new(),
            sticky_layout_active_combos: Vec::new(),
            sticky_layout_tap_dance_states: Vec::new(),
            sticky_layout_base_layer: 0,
            sticky_layout_active_layer: 0,
            sticky_layout_last_size: None,
            sticky_layout_resize_opacity_hold_frames: 0,
            sticky_layout_viewport_events: StickyLayoutViewportEventQueue::default(),
            pending_layout_indicator_open_after_unlock: false,
            matrix_tester_last_poll: std::time::Instant::now(),
            matrix_tester_last_lock_check: std::time::Instant::now()
                - MATRIX_TESTER_LOCK_CHECK_INTERVAL,
            matrix_tester_unlock_prompted: false,
            matrix_tester_lock_checked: false,
            macro_auto_unlock_cancelled: false,
            settings_tab: SettingsTab::MatrixTester,
            layer_names,
            confirmed_firmware_layer_names: 0,
            editing_layer: None,
            layer_names_from_firmware: Vec::new(),
            editing_layer_text: String::new(),
            editing_layer_focus_requested: false,
            editing_layer_layout_id: None,
            editing_layer_device_key: None,
            #[cfg(not(target_arch = "wasm32"))]
            editing_layer_generation: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_layer_name_writes: Default::default(),
            #[cfg(not(target_arch = "wasm32"))]
            pending_universal_geometry: None,
            #[cfg(not(target_arch = "wasm32"))]
            uncertain_layer_names: Vec::new(),
            current_device_name: String::new(),
            current_keyboard_id: None,
            current_encoder_visibility_id: String::new(),
            editing_layout_visibility: false,
            device_display_names: std::collections::HashMap::new(),
            device_about_info: None,
            update_check,
            firmware_update_check: FirmwareUpdateCheckState::Unsupported,
            #[cfg(not(target_arch = "wasm32"))]
            update_notifications: Default::default(),
            tour_state: TourState::default(),
            tour_target_rects: Vec::new(),
            unlock_open: false,
            vial_unlocked: None,
            vial_unlock_keys: vec![],
            vial_unlock_session_started: false,
            #[cfg(not(target_arch = "wasm32"))]
            vial_unlock_event_pause: None,
            vial_unlock_polling: false,
            vial_unlock_counter: 0,
            vial_unlock_best: 50,
            vial_unlock_total: 50,
            vial_unlock_last_poll: None,
            vial_unlock_animation_nonce: 0,
            #[cfg(not(target_arch = "wasm32"))]
            connect_state: ConnectState::Idle,
            #[cfg(not(target_arch = "wasm32"))]
            retiring_connects: Vec::new(),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            test_device_scan_requests: None,
            #[cfg(all(test, not(target_arch = "wasm32")))]
            test_connect_requests: None,
            #[cfg(not(target_arch = "wasm32"))]
            connect_hid: None,
            #[cfg(all(test, target_os = "linux"))]
            test_device_scan_udev_access: None,
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
                        binding.is_vial_native() || (kc != 0x0000 && kc != 0x0001)
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

    /// Whether the board has a lock Entropy unlocks: Vial and Rynk boards do.
    pub(crate) fn has_keyboard_lock(&self) -> bool {
        matches!(
            self.firmware,
            FirmwareProtocol::Vial | FirmwareProtocol::Rynk
        )
    }

    /// Whether reading the key matrix waits for an unlock. A Rynk board serves
    /// its matrix only unlocked, as a Vial board does, but takes writes while
    /// locked, so [`Self::is_vial_locked`] stays Vial's.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn is_matrix_locked(&self) -> bool {
        self.has_keyboard_lock()
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
                self.hid_device = Some(hid.into());
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
}
