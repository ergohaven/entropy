use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub(super) enum DeferredLoadRequest {
    Layer {
        layer: usize,
        context: std::sync::Arc<DeferredDeviceLoadContext>,
    },
    BackgroundLayerStep {
        layer: usize,
        step: BackgroundLayerStep,
        context: std::sync::Arc<DeferredDeviceLoadContext>,
    },
    Section {
        section: DeferredLoadSection,
        context: std::sync::Arc<DeferredDeviceLoadContext>,
    },
}

#[cfg(not(target_arch = "wasm32"))]
impl DeferredLoadRequest {
    pub(super) fn layer(&self) -> Option<usize> {
        match self {
            Self::Layer { layer, .. } | Self::BackgroundLayerStep { layer, .. } => Some(*layer),
            Self::Section { .. } => None,
        }
    }

    pub(super) fn section(&self) -> Option<DeferredLoadSection> {
        match self {
            Self::Layer { .. } | Self::BackgroundLayerStep { .. } => None,
            Self::Section { section, .. } => Some(*section),
        }
    }

    pub(super) fn is_background_layer(&self) -> bool {
        matches!(self, Self::BackgroundLayerStep { .. })
    }

    pub(super) fn blocks_keyboard(&self) -> bool {
        !self.is_background_layer()
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) enum DeferredLoadPayload {
    Layer {
        layer: usize,
        keymap: Vec<u16>,
        encoders: Vec<(u16, u16)>,
        firmware_name: Option<String>,
    },
    BackgroundLayerStep {
        layer: usize,
        result: BackgroundLayerStepResult,
    },
    Macros(Vec<Vec<u8>>),
    Combos(Vec<ComboEntry>),
    TapDance(Vec<crate::keycode_picker::TapDanceEntry>),
    KeyOverrides(Vec<KeyOverrideEntry>),
    AltRepeat(Vec<AltRepeatKeyEntry>),
    BehaviorSettings(BehaviorSettingsState),
    Modules(ModuleSettingsState),
    Touchpad(TouchpadSettingsState),
    Bluetooth(BluetoothSettingsState),
    LayerLeds(LayerLedSettingsState),
    Rgb(RgbSettingsState),
}

#[cfg(not(target_arch = "wasm32"))]
const ENTLAYOUT_EXPORT_SECTIONS: [DeferredLoadSection; 5] = [
    DeferredLoadSection::Macros,
    DeferredLoadSection::Combos,
    DeferredLoadSection::TapDance,
    DeferredLoadSection::KeyOverrides,
    DeferredLoadSection::AltRepeat,
];

#[cfg(not(target_arch = "wasm32"))]
enum DeferredOverlayTarget {
    Layer(usize, DeferredLoadStatus),
    Section(DeferredLoadSection, DeferredLoadStatus),
}

#[cfg(not(target_arch = "wasm32"))]
fn event_defers_automatic_background_load(event: &egui::Event) -> bool {
    match event {
        egui::Event::Copy
        | egui::Event::Cut
        | egui::Event::Paste(_)
        | egui::Event::Text(_)
        | egui::Event::Zoom(_)
        | egui::Event::Rotate(_)
        | egui::Event::MouseWheel { .. }
        | egui::Event::AccessKitActionRequest(_) => true,
        egui::Event::Key { pressed, .. } | egui::Event::PointerButton { pressed, .. } => *pressed,
        egui::Event::Ime(egui::ImeEvent::Preedit(_) | egui::ImeEvent::Commit(_)) => true,
        egui::Event::Touch { phase, .. } => {
            matches!(phase, egui::TouchPhase::Start | egui::TouchPhase::Move)
        }
        egui::Event::PointerMoved(_)
        | egui::Event::MouseMoved(_)
        | egui::Event::PointerGone
        | egui::Event::Ime(egui::ImeEvent::Enabled | egui::ImeEvent::Disabled)
        | egui::Event::WindowFocused(_)
        | egui::Event::Screenshot { .. } => false,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn run_deferred_load(
    hid: &crate::hid::HidDevice,
    request: &DeferredLoadRequest,
) -> anyhow::Result<DeferredLoadPayload> {
    match request {
        DeferredLoadRequest::Layer { layer, context } => {
            let keymap =
                hid.get_keymap_layer(*layer, context.layer_count, context.rows, context.cols)?;
            let mut encoders = Vec::with_capacity(context.encoder_count);
            for encoder in 0..context.encoder_count {
                encoders.push(
                    hid.get_encoder(*layer as u8, encoder as u8)
                        .unwrap_or_else(|error| {
                            log::warn!(
                                "get_encoder(layer={layer}, idx={encoder}) during staged load: {error}"
                            );
                            (0, 0)
                        }),
                );
            }
            let qsid = 200 + *layer as u16;
            let firmware_name = if context.supported_qmk_settings.contains(&qsid) {
                match hid.get_qmk_setting_string(qsid) {
                    Ok(name) if !name.trim().is_empty() => Some(name),
                    Ok(_) => None,
                    Err(error) => {
                        log::warn!(
                            "get_qmk_setting_string(layer name qsid {qsid}) during staged load: {error}"
                        );
                        None
                    }
                }
            } else {
                None
            };
            Ok(DeferredLoadPayload::Layer {
                layer: *layer,
                keymap,
                encoders,
                firmware_name,
            })
        }
        DeferredLoadRequest::BackgroundLayerStep {
            layer,
            step,
            context,
        } => {
            let result = match *step {
                BackgroundLayerStep::Keymap { local_offset } => BackgroundLayerStepResult::Keymap {
                    local_offset,
                    keycodes: hid.get_keymap_layer_chunk(
                        *layer,
                        context.layer_count,
                        context.rows,
                        context.cols,
                        local_offset,
                    )?,
                },
                BackgroundLayerStep::Encoder { encoder_index } => {
                    let keycodes = hid
                        .get_encoder(*layer as u8, encoder_index as u8)
                        .unwrap_or_else(|error| {
                            log::warn!(
                                "get_encoder(layer={layer}, idx={encoder_index}) during background load: {error}"
                            );
                            (0, 0)
                        });
                    BackgroundLayerStepResult::Encoder {
                        encoder_index,
                        keycodes,
                    }
                }
                BackgroundLayerStep::FirmwareName => {
                    let qsid = 200 + *layer as u16;
                    let firmware_name = match hid.get_qmk_setting_string(qsid) {
                        Ok(name) if !name.trim().is_empty() => Some(name),
                        Ok(_) => None,
                        Err(error) => {
                            log::warn!(
                                "get_qmk_setting_string(layer name qsid {qsid}) during background load: {error}"
                            );
                            None
                        }
                    };
                    BackgroundLayerStepResult::FirmwareName(firmware_name)
                }
            };
            Ok(DeferredLoadPayload::BackgroundLayerStep {
                layer: *layer,
                result,
            })
        }
        DeferredLoadRequest::Section { section, context } => {
            let payload: anyhow::Result<DeferredLoadPayload> = match section {
                DeferredLoadSection::Macros => {
                    let count = context.macro_count;
                    let size = match context.macro_memory_bytes {
                        Some(size) => size,
                        None => hid.get_macro_buffer_size()?,
                    };
                    let buffer = hid.get_macro_buffer(size, count)?;
                    let texts = crate::hid::HidDevice::parse_macros(&buffer, count);
                    Ok(DeferredLoadPayload::Macros(texts))
                }
                DeferredLoadSection::Combos => {
                    let entries = (0..context.combo_count)
                        .map(|index| match hid.get_combo(index) {
                            Ok((keys, output)) => Ok(ComboEntry { keys, output }),
                            Err(error) if crate::hid::is_disconnect_error(&error) => Err(error),
                            Err(error) => {
                                log::warn!("get_combo({index}) during staged load: {error}");
                                Ok(ComboEntry::default())
                            }
                        })
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    Ok(DeferredLoadPayload::Combos(entries))
                }
                DeferredLoadSection::TapDance => {
                    let entries = (0..context.tap_dance_count)
                        .map(|index| match hid.get_tap_dance(index) {
                            Ok((on_tap, on_hold, on_double_tap, on_tap_hold, tapping_term)) => {
                                Ok(crate::keycode_picker::TapDanceEntry {
                                    on_tap,
                                    on_hold,
                                    on_double_tap,
                                    on_tap_hold,
                                    tapping_term,
                                })
                            }
                            Err(error) if crate::hid::is_disconnect_error(&error) => Err(error),
                            Err(error) => {
                                log::warn!("get_tap_dance({index}) during staged load: {error}");
                                Ok(Default::default())
                            }
                        })
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    Ok(DeferredLoadPayload::TapDance(entries))
                }
                DeferredLoadSection::KeyOverrides => {
                    let entries = (0..context.key_override_count)
                        .map(|index| match hid.get_key_override(index) {
                            Ok((
                                trigger,
                                replacement,
                                layers,
                                trigger_mods,
                                negative_mod_mask,
                                suppressed_mods,
                                options,
                            )) => Ok(KeyOverrideEntry {
                                trigger,
                                replacement,
                                layers,
                                trigger_mods,
                                negative_mod_mask,
                                suppressed_mods,
                                options: KeyOverrideOptionsState::from_bits(options),
                            }),
                            Err(error) if crate::hid::is_disconnect_error(&error) => Err(error),
                            Err(error) => {
                                log::warn!("get_key_override({index}) during staged load: {error}");
                                Ok(KeyOverrideEntry::default())
                            }
                        })
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    Ok(DeferredLoadPayload::KeyOverrides(entries))
                }
                DeferredLoadSection::AltRepeat => {
                    let entries = (0..context.alt_repeat_count)
                        .map(|index| match hid.get_alt_repeat_key(index) {
                            Ok((keycode, alt_keycode, allowed_mods, options)) => {
                                Ok(AltRepeatKeyEntry {
                                    keycode,
                                    alt_keycode,
                                    allowed_mods,
                                    options: AltRepeatKeyOptionsState::from_bits(options),
                                })
                            }
                            Err(error) if crate::hid::is_disconnect_error(&error) => Err(error),
                            Err(error) => {
                                log::warn!(
                                    "get_alt_repeat_key({index}) during staged load: {error}"
                                );
                                Ok(AltRepeatKeyEntry::default())
                            }
                        })
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    Ok(DeferredLoadPayload::AltRepeat(entries))
                }
                DeferredLoadSection::BehaviorSettings => Ok(DeferredLoadPayload::BehaviorSettings(
                    EntropyApp::read_behavior_settings(&context.supported_qmk_settings, hid),
                )),
                DeferredLoadSection::Modules => Ok(DeferredLoadPayload::Modules(
                    EntropyApp::read_module_settings(
                        &context.json,
                        &context.supported_qmk_settings,
                        hid,
                    ),
                )),
                DeferredLoadSection::Touchpad => Ok(DeferredLoadPayload::Touchpad(
                    EntropyApp::read_touchpad_settings(
                        &context.json,
                        &context.supported_qmk_settings,
                        hid,
                    ),
                )),
                DeferredLoadSection::Bluetooth => Ok(DeferredLoadPayload::Bluetooth(
                    EntropyApp::read_bluetooth_settings(
                        &context.json,
                        &context.supported_qmk_settings,
                        hid,
                    ),
                )),
                DeferredLoadSection::LayerLeds => Ok(DeferredLoadPayload::LayerLeds(
                    EntropyApp::read_layer_led_settings(
                        &context.json,
                        &context.supported_qmk_settings,
                        context.layer_count,
                        hid,
                    ),
                )),
                DeferredLoadSection::Rgb => Ok(DeferredLoadPayload::Rgb(
                    load_rgb_settings_for_mode(hid, context.lighting_mode.as_deref()),
                )),
            };
            let payload = payload?;
            hid.get_protocol_version().map_err(|error| {
                anyhow::anyhow!("deferred Bluetooth transport check: {error:#}")
            })?;
            Ok(payload)
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn settings_tab_deferred_sections(tab: SettingsTab) -> &'static [DeferredLoadSection] {
    match tab {
        SettingsTab::Combo => &[
            DeferredLoadSection::Combos,
            DeferredLoadSection::BehaviorSettings,
        ],
        SettingsTab::AutoShift
        | SettingsTab::Magic
        | SettingsTab::TapHold
        | SettingsTab::GraveEscape
        | SettingsTab::MouseKeys => &[DeferredLoadSection::BehaviorSettings],
        SettingsTab::KeyOverrides => &[DeferredLoadSection::KeyOverrides],
        SettingsTab::AltRepeat => &[DeferredLoadSection::AltRepeat],
        SettingsTab::Modules => &[DeferredLoadSection::Modules],
        SettingsTab::Touchpad => &[DeferredLoadSection::Touchpad],
        SettingsTab::Bluetooth => &[DeferredLoadSection::Bluetooth],
        SettingsTab::LayerLeds => &[DeferredLoadSection::LayerLeds],
        SettingsTab::Rgb => &[DeferredLoadSection::Rgb],
        _ => &[],
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn deferred_full_layout_data_ready(
    state: &DeferredDeviceLoadState,
    action: DeferredFullLayoutAction,
) -> bool {
    state.all_layers_ready()
        && match action {
            DeferredFullLayoutAction::ImportEntlayout
            | DeferredFullLayoutAction::ExportEntlayout => ENTLAYOUT_EXPORT_SECTIONS
                .into_iter()
                .all(|section| state.section_status(section).ready()),
            DeferredFullLayoutAction::OpenImageExport => true,
        }
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn deferred_section_available(&self, section: DeferredLoadSection) -> bool {
        self.deferred_device_load.section_supported(section)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn selected_layer_data_ready(&self) -> bool {
        self.deferred_device_load
            .layer_status(self.selected_layer)
            .ready()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn sticky_layout_deferred_data_ready(&self) -> bool {
        !self.app_settings.sticky_layout_window
            || ([DeferredLoadSection::Combos, DeferredLoadSection::TapDance]
                .into_iter()
                .all(|section| self.deferred_device_load.section_status(section).ready())
                && self
                    .deferred_device_load
                    .layer_status(self.sticky_layout_active_layer)
                    .ready())
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn selected_layer_data_ready(&self) -> bool {
        true
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn picker_macro_data_state(&self) -> DeferredPickerDataState {
        self.deferred_picker_data_state(DeferredLoadSection::Macros)
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn picker_macro_data_state(&self) -> DeferredPickerDataState {
        DeferredPickerDataState::Ready
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn picker_tap_dance_data_state(&self) -> DeferredPickerDataState {
        self.deferred_picker_data_state(DeferredLoadSection::TapDance)
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn picker_tap_dance_data_state(&self) -> DeferredPickerDataState {
        DeferredPickerDataState::Ready
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn deferred_picker_data_state(&self, section: DeferredLoadSection) -> DeferredPickerDataState {
        match self.deferred_device_load.section_status(section) {
            DeferredLoadStatus::Loaded | DeferredLoadStatus::NotNeeded => {
                DeferredPickerDataState::Ready
            }
            DeferredLoadStatus::Failed(_) => DeferredPickerDataState::Failed,
            DeferredLoadStatus::NotLoaded | DeferredLoadStatus::Loading => {
                DeferredPickerDataState::Loading
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn retry_picker_deferred_data(&mut self, tab: KeycodeTab) {
        let section = match tab {
            KeycodeTab::Macro => Some(DeferredLoadSection::Macros),
            KeycodeTab::TapDance => Some(DeferredLoadSection::TapDance),
            _ => None,
        };
        if let Some(section) = section {
            self.deferred_device_load
                .set_section_status(section, DeferredLoadStatus::NotLoaded);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn deferred_request_for_current_view(
        &self,
        allow_automatic_background_layer: bool,
    ) -> Option<DeferredLoadRequest> {
        let context = self.deferred_device_load.context.as_ref()?.clone();

        if matches!(
            self.deferred_device_load.layer_status(self.selected_layer),
            DeferredLoadStatus::NotLoaded
        ) {
            return Some(DeferredLoadRequest::Layer {
                layer: self.selected_layer,
                context,
            });
        }

        if self.deferred_full_layout_action.is_some() {
            if let Some(layer) = self.deferred_device_load.next_unloaded_layer() {
                return Some(DeferredLoadRequest::Layer { layer, context });
            }
            if matches!(
                self.deferred_full_layout_action,
                Some(
                    DeferredFullLayoutAction::ImportEntlayout
                        | DeferredFullLayoutAction::ExportEntlayout
                )
            ) {
                for section in ENTLAYOUT_EXPORT_SECTIONS {
                    if matches!(
                        self.deferred_device_load.section_status(section),
                        DeferredLoadStatus::NotLoaded
                    ) {
                        return Some(DeferredLoadRequest::Section { section, context });
                    }
                }
            }
        }

        if self.keycode_picker.open {
            let picker_section = match self.keycode_picker.selected_tab {
                KeycodeTab::Macro => Some(DeferredLoadSection::Macros),
                KeycodeTab::TapDance => Some(DeferredLoadSection::TapDance),
                _ => None,
            };
            if let Some(section) = picker_section {
                if matches!(
                    self.deferred_device_load.section_status(section),
                    DeferredLoadStatus::NotLoaded
                ) {
                    return Some(DeferredLoadRequest::Section { section, context });
                }
            }
        }

        if self.app_settings.sticky_layout_window {
            for section in [DeferredLoadSection::Combos, DeferredLoadSection::TapDance] {
                if matches!(
                    self.deferred_device_load.section_status(section),
                    DeferredLoadStatus::NotLoaded
                ) {
                    return Some(DeferredLoadRequest::Section { section, context });
                }
            }
            if matches!(
                self.deferred_device_load
                    .layer_status(self.sticky_layout_active_layer),
                DeferredLoadStatus::NotLoaded
            ) {
                return Some(DeferredLoadRequest::Layer {
                    layer: self.sticky_layout_active_layer,
                    context,
                });
            }
        }

        if self.main_menu_tab != MainMenuTab::Keyboard {
            for &section in settings_tab_deferred_sections(self.settings_tab) {
                if matches!(
                    self.deferred_device_load.section_status(section),
                    DeferredLoadStatus::NotLoaded
                ) {
                    return Some(DeferredLoadRequest::Section { section, context });
                }
            }
        }

        if allow_automatic_background_layer {
            if let Some((layer, step)) = self.deferred_device_load.next_background_layer_step() {
                return Some(DeferredLoadRequest::BackgroundLayerStep {
                    layer,
                    step,
                    context,
                });
            }
        }

        None
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn automatic_background_layer_load_allowed(&mut self, ctx: &egui::Context) -> bool {
        if ctx.input(|input| {
            input.pointer.any_down()
                || input
                    .events
                    .iter()
                    .any(event_defers_automatic_background_load)
        }) {
            self.deferred_device_load.defer_background_for_user_input();
        }
        !(self.main_menu_tab == MainMenuTab::Settings
            && self.settings_tab == SettingsTab::MatrixTester)
            && !self.keycode_picker.open
            && self.keycode_picker.result.is_none()
            && self.editing_layer.is_none()
            && self.pending_handed_swap.is_none()
            && !self.pending_layout_undo
            && !self.import_pending()
            && !self.top_dropdown_open(ctx)
            && !egui::Popup::is_any_open(ctx)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn request_entlayout_import_after_full_load(&mut self) {
        if self.deferred_full_layout_action_ready(DeferredFullLayoutAction::ImportEntlayout) {
            self.import_entlayout_dialog();
        } else {
            self.deferred_full_layout_action = Some(DeferredFullLayoutAction::ImportEntlayout);
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "connection.loading_current_layout_for_import",
            )
            .into();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn request_entlayout_export_after_full_load(&mut self) {
        if self.deferred_full_layout_action_ready(DeferredFullLayoutAction::ExportEntlayout) {
            self.export_entlayout_dialog();
        } else {
            self.deferred_full_layout_action = Some(DeferredFullLayoutAction::ExportEntlayout);
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "connection.loading_all_layers_for_export",
            )
            .into();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn request_image_export_after_full_load(&mut self) {
        if self.deferred_full_layout_action_ready(DeferredFullLayoutAction::OpenImageExport) {
            self.open_layout_image_export_page();
        } else {
            self.deferred_full_layout_action = Some(DeferredFullLayoutAction::OpenImageExport);
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "connection.loading_all_layers_for_export",
            )
            .into();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn finish_deferred_full_layout_action(&mut self, ctx: &egui::Context) {
        let Some(action) = self.deferred_full_layout_action else {
            return;
        };
        if !self.deferred_full_layout_action_ready(action) {
            return;
        }
        self.deferred_full_layout_action = None;
        match action {
            DeferredFullLayoutAction::ImportEntlayout => self.import_entlayout_dialog(),
            DeferredFullLayoutAction::ExportEntlayout => self.export_entlayout_dialog(),
            DeferredFullLayoutAction::OpenImageExport => self.open_layout_image_export_page(),
        }
        ctx.request_repaint();
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn deferred_full_layout_action_pending(&self) -> bool {
        self.deferred_full_layout_action.is_some()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn deferred_full_layout_action_ready(&self, action: DeferredFullLayoutAction) -> bool {
        deferred_full_layout_data_ready(&self.deferred_device_load, action)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn maybe_start_deferred_device_load(
        &mut self,
        ctx: &egui::Context,
        main_window_hidden_to_tray: bool,
    ) {
        if main_window_hidden_to_tray
            || self.bluetooth_reconnect_active()
            || self.unlock_open
            || self.vial_unlock_polling
            || matches!(self.connect_state, ConnectState::Loading { .. })
        {
            return;
        }
        let allow_automatic_background_layer = self.automatic_background_layer_load_allowed(ctx);
        let Some(request) =
            self.deferred_request_for_current_view(allow_automatic_background_layer)
        else {
            return;
        };
        if request.is_background_layer() {
            if let Some(delay) = self.deferred_device_load.background_layer_resume_delay() {
                ctx.request_repaint_after(delay);
                return;
            }
        }

        match self.start_vial_hid_operation(
            ctx,
            super::vial_hid_task::VialHidOperation::Deferred(request.clone()),
        ) {
            super::vial_hid_task::VialHidTaskStart::Started => {
                if !request.is_background_layer() {
                    if let Some(layer) = request.layer() {
                        self.deferred_device_load
                            .set_layer_status(layer, DeferredLoadStatus::Loading);
                    }
                }
                if let Some(section) = request.section() {
                    self.deferred_device_load
                        .set_section_status(section, DeferredLoadStatus::Loading);
                }
            }
            super::vial_hid_task::VialHidTaskStart::Busy => {
                ctx.request_repaint_after(std::time::Duration::from_millis(80));
            }
            super::vial_hid_task::VialHidTaskStart::NoDevice => {}
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn finish_deferred_device_load(&mut self, payload: DeferredLoadPayload) {
        match payload {
            DeferredLoadPayload::Layer {
                layer,
                keymap,
                encoders,
                firmware_name,
            } => {
                self.deferred_device_load
                    .clear_background_layer_progress(layer);
                self.apply_deferred_layer(layer, keymap, encoders, firmware_name);
            }
            DeferredLoadPayload::BackgroundLayerStep { layer, result } => {
                match self
                    .deferred_device_load
                    .record_background_layer_step(layer, result)
                {
                    Ok(Some(data)) => {
                        self.apply_deferred_layer(
                            data.layer,
                            data.keymap,
                            data.encoders,
                            data.firmware_name,
                        );
                    }
                    Ok(None) => {}
                    Err(error) => {
                        log::warn!("Background Bluetooth layer assembly failed: {error}");
                        self.deferred_device_load
                            .clear_background_layer_progress(layer);
                        self.deferred_device_load
                            .set_layer_status(layer, DeferredLoadStatus::Failed(error));
                    }
                }
            }
            DeferredLoadPayload::Macros(texts) => {
                self.keycode_picker.macro_count = texts.len();
                self.keycode_picker.macro_texts = texts.clone();
                self.keycode_picker.macro_actions = texts
                    .iter()
                    .map(|bytes| crate::keycode_picker::decode_macro_actions(bytes))
                    .collect();
                self.keycode_picker.macros_dirty = false;
                self.deferred_device_load
                    .set_section_status(DeferredLoadSection::Macros, DeferredLoadStatus::Loaded);
            }
            DeferredLoadPayload::Combos(entries) => {
                self.combo_entries = entries.clone();
                self.combo_synced_entries = entries;
                self.combo_dirty = false;
                self.combo_edit_revision = self.combo_edit_revision.wrapping_add(1);
                self.combo_attempted_revision = None;
                self.combo_names
                    .resize(self.combo_entries.len(), String::new());
                normalize_combo_colors(&mut self.combo_colors, self.combo_entries.len());
                let highest_used = self
                    .combo_entries
                    .iter()
                    .enumerate()
                    .filter(|(index, combo)| {
                        combo.output != 0
                            || combo.keys.iter().any(|keycode| *keycode != 0)
                            || self
                                .combo_names
                                .get(*index)
                                .map(|name| !name.trim().is_empty())
                                .unwrap_or(false)
                    })
                    .map(|(index, _)| index + 1)
                    .max()
                    .unwrap_or(1);
                self.combo_visible_count = highest_used.min(self.combo_entries.len().max(1));
                self.selected_combo = self
                    .selected_combo
                    .min(self.combo_visible_count.saturating_sub(1));
                self.sticky_layout_active_combos = vec![false; self.combo_entries.len()];
                self.deferred_device_load
                    .set_section_status(DeferredLoadSection::Combos, DeferredLoadStatus::Loaded);
            }
            DeferredLoadPayload::TapDance(entries) => {
                self.keycode_picker.tap_dance_entries = entries.clone();
                self.keycode_picker.tap_dance_synced_entries = entries;
                self.keycode_picker.tap_dance_dirty = false;
                self.sticky_layout_tap_dance_states.clear();
                self.deferred_device_load
                    .set_section_status(DeferredLoadSection::TapDance, DeferredLoadStatus::Loaded);
            }
            DeferredLoadPayload::KeyOverrides(entries) => {
                self.key_override_entries = entries;
                self.key_override_names
                    .resize(self.key_override_entries.len(), String::new());
                self.key_override_visible_count = 1;
                self.selected_key_override = 0;
                self.key_override_dirty = false;
                self.key_override_undo_stack.clear();
                self.deferred_device_load.set_section_status(
                    DeferredLoadSection::KeyOverrides,
                    DeferredLoadStatus::Loaded,
                );
            }
            DeferredLoadPayload::AltRepeat(entries) => {
                self.alt_repeat_entries = entries;
                self.alt_repeat_names
                    .resize(self.alt_repeat_entries.len(), String::new());
                self.alt_repeat_visible_count = if self.alt_repeat_entries.is_empty() {
                    1
                } else {
                    1.min(self.alt_repeat_entries.len())
                };
                self.selected_alt_repeat = 0;
                self.alt_repeat_undo_stack.clear();
                self.deferred_device_load
                    .set_section_status(DeferredLoadSection::AltRepeat, DeferredLoadStatus::Loaded);
            }
            DeferredLoadPayload::BehaviorSettings(settings) => {
                self.combo_term = settings.combo_term.or(Some(50));
                self.combo_term_dirty = false;
                self.auto_shift_options = settings.auto_shift_options;
                self.auto_shift_timeout = settings.auto_shift_timeout;
                self.auto_shift_timeout_text = settings
                    .auto_shift_timeout
                    .map(|timeout| timeout.to_string())
                    .unwrap_or_default();
                self.mouse_keys_settings = settings.mouse_keys;
                self.tap_hold_settings = settings.tap_hold;
                self.magic_settings = settings.magic;
                self.one_shot_settings = settings.one_shot;
                self.grave_escape_settings = settings.grave_escape;
                self.keycode_picker.supports_auto_shift = self.supported_qmk_settings.contains(&4);
                self.deferred_device_load.set_section_status(
                    DeferredLoadSection::BehaviorSettings,
                    DeferredLoadStatus::Loaded,
                );
            }
            DeferredLoadPayload::Modules(settings) => {
                self.module_settings = settings;
                if let Some(layout) = self.layout.as_ref() {
                    let encoder_count = layout.encoder_count();
                    self.encoder_visibility = Self::resolve_initial_encoder_visibility(
                        layout,
                        self.layout_options_value,
                        load_saved_encoder_visibility(
                            &self.current_encoder_visibility_id,
                            encoder_count,
                        ),
                        self.module_settings_include_encoder_visibility(layout),
                    );
                }
                self.deferred_device_load
                    .set_section_status(DeferredLoadSection::Modules, DeferredLoadStatus::Loaded);
            }
            DeferredLoadPayload::Touchpad(settings) => {
                self.touchpad_settings = settings;
                self.deferred_device_load
                    .set_section_status(DeferredLoadSection::Touchpad, DeferredLoadStatus::Loaded);
            }
            DeferredLoadPayload::Bluetooth(settings) => {
                self.bluetooth_settings = settings;
                self.deferred_device_load
                    .set_section_status(DeferredLoadSection::Bluetooth, DeferredLoadStatus::Loaded);
            }
            DeferredLoadPayload::LayerLeds(settings) => {
                self.layer_led_settings = settings;
                self.deferred_device_load
                    .set_section_status(DeferredLoadSection::LayerLeds, DeferredLoadStatus::Loaded);
            }
            DeferredLoadPayload::Rgb(settings) => {
                self.rgb_settings = settings;
                self.keycode_picker.supports_rgb = self
                    .layout
                    .as_ref()
                    .map(|layout| layout.supports_rgb)
                    .unwrap_or(false)
                    || self.rgb_settings.supported;
                self.deferred_device_load
                    .set_section_status(DeferredLoadSection::Rgb, DeferredLoadStatus::Loaded);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn apply_deferred_layer(
        &mut self,
        layer: usize,
        keymap: Vec<u16>,
        encoders: Vec<(u16, u16)>,
        firmware_name: Option<String>,
    ) {
        if let Some(layout) = self.layout.as_mut() {
            if let Some(layer_keycodes) = layout.layers.get_mut(layer) {
                for (key_index, key) in layout.keys.iter().enumerate() {
                    let matrix_index = key.row as usize * layout.cols + key.col as usize;
                    if let Some(keycode) = keymap.get(matrix_index) {
                        layer_keycodes[key_index] = *keycode;
                    }
                }
            }
            if let Some(encoder_layer) = layout.encoder_layers.get_mut(layer) {
                for (visual_index, encoder) in layout.encoders.iter().enumerate() {
                    if let Some((ccw, cw)) = encoders.get(encoder.encoder_idx as usize) {
                        encoder_layer[visual_index] =
                            if encoder.direction == 0 { *ccw } else { *cw };
                    }
                }
            }
            if let Some(name) = firmware_name.filter(|name| !name.trim().is_empty()) {
                if let Some(layer_name) = layout.layer_names.get_mut(layer) {
                    *layer_name = name.clone();
                }
                if let Some(layer_name) = self.layer_names.get_mut(layer) {
                    *layer_name = name;
                }
                self.keycode_picker.layer_names = self.layer_names.clone();
            }
        }
        self.deferred_device_load
            .set_layer_status(layer, DeferredLoadStatus::Loaded);
        self.refresh_layer_picker_content_flags();
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn fail_deferred_device_load(
        &mut self,
        request: &DeferredLoadRequest,
        error: String,
    ) {
        if let Some(layer) = request.layer() {
            self.deferred_device_load
                .clear_background_layer_progress(layer);
            self.deferred_device_load
                .set_layer_status(layer, DeferredLoadStatus::Failed(error));
        } else if let Some(section) = request.section() {
            self.deferred_device_load
                .set_section_status(section, DeferredLoadStatus::Failed(error));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn preserve_deferred_snapshot_on_reconnect(&self, result: &mut ConnectResult) {
        if !result.deferred_load.is_staged() {
            return;
        }
        result
            .deferred_load
            .merge_loaded_from(&self.deferred_device_load);

        if let Some(previous_layout) = self.layout.as_ref() {
            for layer in 1..result.layer_count {
                if !matches!(
                    result.deferred_load.layer_status(layer),
                    DeferredLoadStatus::Loaded
                ) {
                    continue;
                }
                if let (Some(current), Some(previous)) = (
                    result.layout.layers.get_mut(layer),
                    previous_layout.layers.get(layer),
                ) {
                    *current = previous.clone();
                }
                if let (Some(current), Some(previous)) = (
                    result.layout.encoder_layers.get_mut(layer),
                    previous_layout.encoder_layers.get(layer),
                ) {
                    *current = previous.clone();
                }
                if let (Some(current), Some(previous)) = (
                    result.layout.layer_names.get_mut(layer),
                    self.layer_names.get(layer),
                ) {
                    *current = previous.clone();
                }
            }
        }

        if matches!(
            result
                .deferred_load
                .section_status(DeferredLoadSection::Macros),
            DeferredLoadStatus::Loaded
        ) {
            result.macro_texts = self.keycode_picker.macro_texts.clone();
        }
        if matches!(
            result
                .deferred_load
                .section_status(DeferredLoadSection::Combos),
            DeferredLoadStatus::Loaded
        ) {
            result.combo_entries = self.combo_entries.clone();
        }
        if matches!(
            result
                .deferred_load
                .section_status(DeferredLoadSection::TapDance),
            DeferredLoadStatus::Loaded
        ) {
            result.tap_dance_entries = self.keycode_picker.tap_dance_entries.clone();
        }
        if matches!(
            result
                .deferred_load
                .section_status(DeferredLoadSection::KeyOverrides),
            DeferredLoadStatus::Loaded
        ) {
            result.key_override_entries = self.key_override_entries.clone();
        }
        if matches!(
            result
                .deferred_load
                .section_status(DeferredLoadSection::AltRepeat),
            DeferredLoadStatus::Loaded
        ) {
            result.alt_repeat_entries = self.alt_repeat_entries.clone();
        }
        if matches!(
            result
                .deferred_load
                .section_status(DeferredLoadSection::BehaviorSettings),
            DeferredLoadStatus::Loaded
        ) {
            result.combo_term = self.combo_term;
            result.auto_shift_options = self.auto_shift_options;
            result.auto_shift_timeout = self.auto_shift_timeout;
            result.mouse_keys_settings = self.mouse_keys_settings;
            result.tap_hold_settings = self.tap_hold_settings;
            result.magic_settings = self.magic_settings;
            result.one_shot_settings = self.one_shot_settings;
            result.grave_escape_settings = self.grave_escape_settings;
        }
        if matches!(
            result
                .deferred_load
                .section_status(DeferredLoadSection::Modules),
            DeferredLoadStatus::Loaded
        ) {
            result.module_settings = self.module_settings.clone();
        }
        if matches!(
            result
                .deferred_load
                .section_status(DeferredLoadSection::Touchpad),
            DeferredLoadStatus::Loaded
        ) {
            result.touchpad_settings = self.touchpad_settings.clone();
        }
        if matches!(
            result
                .deferred_load
                .section_status(DeferredLoadSection::Bluetooth),
            DeferredLoadStatus::Loaded
        ) {
            result.bluetooth_settings = self.bluetooth_settings.clone();
        }
        if matches!(
            result
                .deferred_load
                .section_status(DeferredLoadSection::LayerLeds),
            DeferredLoadStatus::Loaded
        ) {
            result.layer_led_settings = self.layer_led_settings.clone();
        }
        if matches!(
            result
                .deferred_load
                .section_status(DeferredLoadSection::Rgb),
            DeferredLoadStatus::Loaded
        ) {
            result.rgb_settings = self.rgb_settings.clone();
        }
        if let Some(previous) = self.device_about_info.as_ref() {
            result.about_info.battery_halves = previous.battery_halves;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn draw_deferred_settings_gate(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) -> bool {
        if self.settings_tab == SettingsTab::LayoutImageExport {
            let Some((layer, status)) = self.deferred_device_load.first_incomplete_layer() else {
                return false;
            };
            let retry = self.draw_deferred_gate_body(
                ui,
                content_rect,
                &status,
                "connection.loading_device_data",
                "connection.layer_load_failed",
            );
            if retry {
                self.deferred_device_load
                    .set_layer_status(layer, DeferredLoadStatus::NotLoaded);
            }
            return true;
        }

        let Some(section) = settings_tab_deferred_sections(self.settings_tab)
            .iter()
            .copied()
            .find(|section| !self.deferred_device_load.section_status(*section).ready())
        else {
            return false;
        };
        let status = self.deferred_device_load.section_status(section);
        if status.ready() {
            return false;
        }

        let retry = self.draw_deferred_gate_body(
            ui,
            content_rect,
            &status,
            "connection.loading_device_data",
            "connection.device_data_load_failed",
        );
        if retry {
            self.deferred_device_load
                .set_section_status(section, DeferredLoadStatus::NotLoaded);
        }
        true
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn draw_deferred_gate_body(
        &self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
        status: &DeferredLoadStatus,
        loading_key: &'static str,
        failed_key: &'static str,
    ) -> bool {
        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            let mut retry = false;
            ui.vertical_centered(|ui| {
                ui.add_space((content_rect.height() * 0.42).max(24.0));
                match status {
                    DeferredLoadStatus::Failed(error) => {
                        ui.label(
                            RichText::new(crate::i18n::tr_catalog(
                                self.app_settings.language,
                                failed_key,
                            ))
                            .size(14.0)
                            .color(app_muted_text(self.dark_mode)),
                        );
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(error)
                                .size(11.0)
                                .color(app_muted_text(self.dark_mode)),
                        );
                        ui.add_space(12.0);
                        if crate::ui_style::modern_button(
                            ui,
                            crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "connection.retry_device_data",
                            ),
                            egui::vec2(120.0, 32.0),
                            true,
                        )
                        .clicked()
                        {
                            retry = true;
                        }
                    }
                    _ => {
                        ui.add(egui::Spinner::new().size(18.0));
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(crate::i18n::tr_catalog(
                                self.app_settings.language,
                                loading_key,
                            ))
                            .size(14.0)
                            .color(app_muted_text(self.dark_mode)),
                        );
                    }
                }
            });
            retry
        })
        .inner
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn draw_deferred_keyboard_overlay(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let selected_status = self.deferred_device_load.layer_status(self.selected_layer);
        let target = if !selected_status.ready() {
            DeferredOverlayTarget::Layer(self.selected_layer, selected_status)
        } else if !self.sticky_layout_deferred_data_ready() {
            [DeferredLoadSection::Combos, DeferredLoadSection::TapDance]
                .into_iter()
                .find_map(|section| {
                    let status = self.deferred_device_load.section_status(section);
                    (!status.ready()).then_some(DeferredOverlayTarget::Section(section, status))
                })
                .unwrap_or_else(|| {
                    DeferredOverlayTarget::Layer(
                        self.sticky_layout_active_layer,
                        self.deferred_device_load
                            .layer_status(self.sticky_layout_active_layer),
                    )
                })
        } else if self.deferred_full_layout_action.is_some() {
            if let Some((layer, status)) = self.deferred_device_load.first_incomplete_layer() {
                DeferredOverlayTarget::Layer(layer, status)
            } else if matches!(
                self.deferred_full_layout_action,
                Some(
                    DeferredFullLayoutAction::ImportEntlayout
                        | DeferredFullLayoutAction::ExportEntlayout
                )
            ) {
                ENTLAYOUT_EXPORT_SECTIONS
                    .into_iter()
                    .find_map(|section| {
                        let status = self.deferred_device_load.section_status(section);
                        (!status.ready()).then_some(DeferredOverlayTarget::Section(section, status))
                    })
                    .unwrap_or_else(|| {
                        DeferredOverlayTarget::Layer(
                            self.selected_layer,
                            DeferredLoadStatus::Loaded,
                        )
                    })
            } else {
                DeferredOverlayTarget::Layer(self.selected_layer, DeferredLoadStatus::Loaded)
            }
        } else {
            DeferredOverlayTarget::Layer(self.selected_layer, selected_status)
        };
        let target_status = match &target {
            DeferredOverlayTarget::Layer(_, status) | DeferredOverlayTarget::Section(_, status) => {
                status
            }
        };
        let failed = match target_status {
            DeferredLoadStatus::Failed(error) => Some(error.clone()),
            _ => None,
        };
        let target_is_section = matches!(&target, DeferredOverlayTarget::Section(_, _));
        let loading_key = if target_is_section {
            "connection.loading_device_data"
        } else if matches!(
            self.deferred_full_layout_action,
            Some(DeferredFullLayoutAction::ImportEntlayout)
        ) {
            "connection.loading_current_layout_for_import"
        } else if self.deferred_full_layout_action.is_some() {
            "connection.loading_all_layers_for_export"
        } else {
            "connection.loading_layer_data"
        };
        let failed_key = if target_is_section {
            "connection.device_data_load_failed"
        } else {
            "connection.layer_load_failed"
        };
        let overlay_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(360.0, 110.0));
        crate::ui_style::allocate_ui_at_rect(ui, overlay_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(12.0);
                if let Some(error) = failed.as_deref() {
                    ui.label(
                        RichText::new(crate::i18n::tr_catalog(
                            self.app_settings.language,
                            failed_key,
                        ))
                        .size(14.0)
                        .color(app_muted_text(self.dark_mode)),
                    );
                    ui.add_space(5.0);
                    ui.label(
                        RichText::new(error)
                            .size(11.0)
                            .color(app_muted_text(self.dark_mode)),
                    );
                    ui.add_space(9.0);
                    if crate::ui_style::modern_button(
                        ui,
                        crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "connection.retry_device_data",
                        ),
                        egui::vec2(120.0, 32.0),
                        true,
                    )
                    .clicked()
                    {
                        match target {
                            DeferredOverlayTarget::Layer(layer, _) => self
                                .deferred_device_load
                                .set_layer_status(layer, DeferredLoadStatus::NotLoaded),
                            DeferredOverlayTarget::Section(section, _) => self
                                .deferred_device_load
                                .set_section_status(section, DeferredLoadStatus::NotLoaded),
                        }
                    }
                } else {
                    ui.add(egui::Spinner::new().size(18.0));
                    ui.add_space(7.0);
                    ui.label(
                        RichText::new(crate::i18n::tr_catalog(
                            self.app_settings.language,
                            loading_key,
                        ))
                        .size(14.0)
                        .color(app_muted_text(self.dark_mode)),
                    );
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> DeferredDeviceLoadContext {
        DeferredDeviceLoadContext {
            json: std::sync::Arc::new(serde_json::json!({})),
            supported_qmk_settings: std::sync::Arc::new(Vec::new()),
            definition_fingerprint: 7,
            layer_count: 4,
            rows: 2,
            cols: 3,
            encoder_count: 0,
            macro_count: 2,
            macro_memory_bytes: Some(64),
            tap_dance_count: 3,
            combo_count: 4,
            key_override_count: 0,
            alt_repeat_count: 0,
            modules_supported: false,
            touchpad_supported: false,
            bluetooth_supported: true,
            layer_leds_supported: false,
            rgb_supported: false,
            lighting_mode: None,
        }
    }

    fn context_with_behavior_settings() -> DeferredDeviceLoadContext {
        let mut context = context();
        context.supported_qmk_settings = std::sync::Arc::new(vec![2, 4, 21]);
        context
    }

    #[test]
    fn staged_state_marks_only_first_layer_ready() {
        let state = DeferredDeviceLoadState::staged(context());

        assert!(state.layer_status(0).ready());
        assert_eq!(state.layer_status(1), DeferredLoadStatus::NotLoaded);
        assert_eq!(
            state.section_status(DeferredLoadSection::Combos),
            DeferredLoadStatus::NotLoaded
        );
        assert_eq!(
            state.section_status(DeferredLoadSection::Modules),
            DeferredLoadStatus::NotNeeded
        );
    }

    #[test]
    fn staged_state_defers_behavior_values_when_the_schema_supports_them() {
        let state = DeferredDeviceLoadState::staged(context_with_behavior_settings());

        assert_eq!(
            state.section_status(DeferredLoadSection::BehaviorSettings),
            DeferredLoadStatus::NotLoaded
        );
    }

    #[test]
    fn staged_state_does_not_treat_none_lighting_as_rgb_support() {
        let mut context = context();
        context.lighting_mode = Some("none".to_owned());
        context.rgb_supported = false;

        let state = DeferredDeviceLoadState::staged(context);

        assert_eq!(
            state.section_status(DeferredLoadSection::Rgb),
            DeferredLoadStatus::NotNeeded
        );
    }

    #[test]
    fn staged_state_defers_rgb_values_for_supported_lighting() {
        let mut context = context();
        context.lighting_mode = Some("qmk_rgblight".to_owned());
        context.rgb_supported = true;

        let state = DeferredDeviceLoadState::staged(context);

        assert_eq!(
            state.section_status(DeferredLoadSection::Rgb),
            DeferredLoadStatus::NotLoaded
        );
    }

    #[test]
    fn reconnect_reuses_only_loaded_values_from_compatible_schema() {
        let mut previous = DeferredDeviceLoadState::staged(context());
        previous.set_layer_status(2, DeferredLoadStatus::Loaded);
        previous.set_section_status(DeferredLoadSection::Combos, DeferredLoadStatus::Loaded);
        let mut current = DeferredDeviceLoadState::staged(context());

        current.merge_loaded_from(&previous);

        assert!(current.layer_status(2).ready());
        assert!(current.section_status(DeferredLoadSection::Combos).ready());
        assert_eq!(
            current.section_status(DeferredLoadSection::TapDance),
            DeferredLoadStatus::NotLoaded
        );
    }

    #[test]
    fn reconnect_does_not_reuse_values_from_an_incompatible_schema() {
        let mut previous = DeferredDeviceLoadState::staged(context());
        previous.set_layer_status(2, DeferredLoadStatus::Loaded);
        previous.set_section_status(DeferredLoadSection::Combos, DeferredLoadStatus::Loaded);
        let mut changed_context = context();
        changed_context.combo_count += 1;
        let mut current = DeferredDeviceLoadState::staged(changed_context);

        current.merge_loaded_from(&previous);

        assert_eq!(current.layer_status(2), DeferredLoadStatus::NotLoaded);
        assert_eq!(
            current.section_status(DeferredLoadSection::Combos),
            DeferredLoadStatus::NotLoaded
        );
    }

    #[test]
    fn all_layers_ready_changes_only_after_every_staged_layer_is_loaded() {
        let mut state = DeferredDeviceLoadState::staged(context());

        assert!(!state.all_layers_ready());
        for layer in 1..4 {
            state.set_layer_status(layer, DeferredLoadStatus::Loaded);
        }

        assert!(state.all_layers_ready());
    }

    #[test]
    fn entlayout_export_waits_for_dynamic_data_while_image_export_only_waits_for_layers() {
        let mut state = DeferredDeviceLoadState::staged(context());
        for layer in 1..4 {
            state.set_layer_status(layer, DeferredLoadStatus::Loaded);
        }

        assert!(deferred_full_layout_data_ready(
            &state,
            DeferredFullLayoutAction::OpenImageExport
        ));
        assert!(!deferred_full_layout_data_ready(
            &state,
            DeferredFullLayoutAction::ExportEntlayout
        ));
        assert!(!deferred_full_layout_data_ready(
            &state,
            DeferredFullLayoutAction::ImportEntlayout
        ));

        for section in ENTLAYOUT_EXPORT_SECTIONS {
            if state.section_supported(section) {
                state.set_section_status(section, DeferredLoadStatus::Loaded);
            }
        }
        assert!(deferred_full_layout_data_ready(
            &state,
            DeferredFullLayoutAction::ExportEntlayout
        ));
        assert!(deferred_full_layout_data_ready(
            &state,
            DeferredFullLayoutAction::ImportEntlayout
        ));
    }

    #[test]
    fn deferred_layer_reader_fetches_only_requested_layer() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let context = std::sync::Arc::new(context());
        let request = DeferredLoadRequest::Layer { layer: 2, context };

        let payload = run_deferred_load(&hid, &request).unwrap();

        assert!(matches!(
            payload,
            DeferredLoadPayload::Layer { layer: 2, .. }
        ));
        let requests = recorder.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(&requests[0][1..3], &[0x00, 0x18]);
    }

    #[test]
    fn deferred_dynamic_reader_propagates_disconnect_without_filling_default_entries() {
        let (hid, recorder) = crate::hid::HidDevice::test_device_with_fault_after_requests(Some((
            0,
            crate::hid::TestHidFault::Disconnect,
        )));
        let request = DeferredLoadRequest::Section {
            section: DeferredLoadSection::Combos,
            context: std::sync::Arc::new(context()),
        };

        let error = match run_deferred_load(&hid, &request) {
            Ok(_) => panic!("disconnect must fail the deferred load"),
            Err(error) => error,
        };

        assert!(crate::hid::is_disconnect_error(&error));
        assert_eq!(recorder.requests().len(), 1);
    }

    #[test]
    fn deferred_behavior_reader_fetches_only_supported_values() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let request = DeferredLoadRequest::Section {
            section: DeferredLoadSection::BehaviorSettings,
            context: std::sync::Arc::new(context_with_behavior_settings()),
        };

        let payload = run_deferred_load(&hid, &request).unwrap();

        assert!(matches!(
            payload,
            DeferredLoadPayload::BehaviorSettings(BehaviorSettingsState {
                combo_term: Some(0),
                auto_shift_timeout: Some(0),
                magic: MagicSettingsState {
                    supported: true,
                    ..
                },
                ..
            })
        ));
        let qsids = recorder
            .requests()
            .iter()
            .filter(|request| request[..2] == [0xFE, 0x0A])
            .map(|request| u16::from_le_bytes([request[2], request[3]]))
            .collect::<Vec<_>>();
        assert_eq!(qsids, vec![2, 4, 21]);
    }

    fn staged_app() -> EntropyApp {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        app.deferred_device_load = DeferredDeviceLoadState::staged(context());
        app.main_menu_tab = MainMenuTab::Keyboard;
        app.selected_layer = 0;
        app.keycode_picker.open = false;
        app.keycode_picker.result = None;
        app.app_settings.sticky_layout_window = false;
        app
    }

    #[test]
    fn automatic_background_load_selects_the_next_layer_on_the_keyboard_page() {
        let app = staged_app();

        let request = app.deferred_request_for_current_view(true).unwrap();

        assert_eq!(request.layer(), Some(1));
        assert!(request.is_background_layer());
        assert!(!request.blocks_keyboard());
    }

    #[test]
    fn selected_unloaded_layer_preempts_the_automatic_background_order() {
        let mut app = staged_app();
        app.selected_layer = 3;

        let request = app.deferred_request_for_current_view(true).unwrap();

        assert_eq!(request.layer(), Some(3));
        assert!(!request.is_background_layer());
        assert!(request.blocks_keyboard());
    }

    #[test]
    fn automatic_background_layer_is_split_at_every_hid_request_boundary() {
        let mut context = context();
        context.rows = 10;
        context.cols = 6;
        context.encoder_count = 2;
        context.supported_qmk_settings = std::sync::Arc::new(vec![201]);
        let mut state = DeferredDeviceLoadState::staged(context);
        let expected_steps = [
            BackgroundLayerStep::Keymap { local_offset: 0 },
            BackgroundLayerStep::Keymap { local_offset: 28 },
            BackgroundLayerStep::Keymap { local_offset: 56 },
            BackgroundLayerStep::Keymap { local_offset: 84 },
            BackgroundLayerStep::Keymap { local_offset: 112 },
            BackgroundLayerStep::Encoder { encoder_index: 0 },
            BackgroundLayerStep::Encoder { encoder_index: 1 },
            BackgroundLayerStep::FirmwareName,
        ];

        for (index, expected_step) in expected_steps.into_iter().enumerate() {
            let (layer, step) = state.next_background_layer_step().unwrap();
            assert_eq!(layer, 1);
            assert_eq!(step, expected_step);
            let result = match step {
                BackgroundLayerStep::Keymap { local_offset } => {
                    let remaining = 120 - local_offset;
                    BackgroundLayerStepResult::Keymap {
                        local_offset,
                        keycodes: vec![0; remaining.min(28) / 2],
                    }
                }
                BackgroundLayerStep::Encoder { encoder_index } => {
                    BackgroundLayerStepResult::Encoder {
                        encoder_index,
                        keycodes: (0, 0),
                    }
                }
                BackgroundLayerStep::FirmwareName => {
                    BackgroundLayerStepResult::FirmwareName(Some("Layer 1".to_owned()))
                }
            };
            let completed = state.record_background_layer_step(layer, result).unwrap();
            if index + 1 == expected_steps.len() {
                let completed = completed.expect("last request completes the layer");
                assert_eq!(completed.layer, 1);
                assert_eq!(completed.keymap.len(), 60);
                assert_eq!(completed.encoders.len(), 2);
                assert_eq!(completed.firmware_name.as_deref(), Some("Layer 1"));
            } else {
                assert!(completed.is_none());
            }
        }
    }

    #[test]
    fn foreground_layer_request_discards_partial_background_data() {
        let mut context = context();
        context.rows = 10;
        context.cols = 6;
        let mut state = DeferredDeviceLoadState::staged(context);

        state
            .record_background_layer_step(
                1,
                BackgroundLayerStepResult::Keymap {
                    local_offset: 0,
                    keycodes: vec![0; 14],
                },
            )
            .unwrap();
        state.set_layer_status(1, DeferredLoadStatus::Loading);
        state.set_layer_status(1, DeferredLoadStatus::NotLoaded);

        assert_eq!(
            state.next_background_layer_step(),
            Some((1, BackgroundLayerStep::Keymap { local_offset: 0 }))
        );
    }

    #[test]
    fn settings_page_behavior_values_preempt_background_layers() {
        let mut app = staged_app();
        app.deferred_device_load =
            DeferredDeviceLoadState::staged(context_with_behavior_settings());
        app.main_menu_tab = MainMenuTab::Advanced;
        app.settings_tab = SettingsTab::AutoShift;

        let request = app.deferred_request_for_current_view(true).unwrap();

        assert_eq!(
            request.section(),
            Some(DeferredLoadSection::BehaviorSettings)
        );
        assert!(request.blocks_keyboard());
    }

    #[test]
    fn combo_page_loads_entries_before_shared_behavior_values() {
        let mut app = staged_app();
        app.deferred_device_load =
            DeferredDeviceLoadState::staged(context_with_behavior_settings());
        app.main_menu_tab = MainMenuTab::Settings;
        app.settings_tab = SettingsTab::Combo;

        let entries_request = app.deferred_request_for_current_view(true).unwrap();
        assert_eq!(entries_request.section(), Some(DeferredLoadSection::Combos));

        app.deferred_device_load
            .set_section_status(DeferredLoadSection::Combos, DeferredLoadStatus::Loaded);
        let behavior_request = app.deferred_request_for_current_view(true).unwrap();
        assert_eq!(
            behavior_request.section(),
            Some(DeferredLoadSection::BehaviorSettings)
        );
    }

    #[test]
    fn automatic_background_layers_leave_a_real_idle_gap_between_requests() {
        let mut state = DeferredDeviceLoadState::default();

        assert!(state.background_layer_resume_delay().is_none());
        state.mark_background_layer_finished();
        assert!(state.background_layer_resume_delay().is_some());
        std::thread::sleep(std::time::Duration::from_millis(90));
        assert!(state.background_layer_resume_delay().is_none());
        state.mark_background_layer_finished();
        assert!(state.background_layer_resume_delay().is_some());
    }

    #[test]
    fn user_input_postpones_the_next_automatic_background_layer() {
        let mut state = DeferredDeviceLoadState::default();

        state.defer_background_for_user_input();

        assert!(state
            .background_layer_resume_delay()
            .is_some_and(|delay| delay > std::time::Duration::from_millis(500)));
    }

    #[test]
    fn background_completion_does_not_shorten_user_input_pause() {
        let mut state = DeferredDeviceLoadState::default();

        state.defer_background_for_user_input();
        state.mark_background_layer_finished();

        assert!(state
            .background_layer_resume_delay()
            .is_some_and(|delay| delay > std::time::Duration::from_millis(500)));
    }

    #[test]
    fn passive_pointer_motion_does_not_defer_automatic_background_load() {
        assert!(!event_defers_automatic_background_load(
            &egui::Event::PointerMoved(egui::pos2(100.0, 100.0))
        ));
        assert!(!event_defers_automatic_background_load(
            &egui::Event::WindowFocused(true)
        ));
    }

    #[test]
    fn deliberate_input_defers_automatic_background_load() {
        assert!(event_defers_automatic_background_load(
            &egui::Event::PointerButton {
                pos: egui::pos2(100.0, 100.0),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }
        ));
        assert!(event_defers_automatic_background_load(
            &egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, 10.0),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
            }
        ));
    }

    #[test]
    fn automatic_background_queue_loads_every_remaining_layer_in_order() {
        let ctx = egui::Context::default();
        let mut app = staged_app();
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid);
        // This test covers queue order rather than the separate initial-idle policy.
        app.deferred_device_load
            .allow_background_layer_now_for_test();

        for _ in 0..500 {
            app.poll_vial_hid_task(&ctx);
            app.maybe_start_deferred_device_load(&ctx, false);
            if app.deferred_device_load.all_layers_ready() && !app.vial_hid_task_active() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        assert!(app.deferred_device_load.all_layers_ready());
        assert!(!app.vial_hid_task_active());
        let offsets = recorder
            .requests()
            .iter()
            .map(|request| u16::from_be_bytes([request[1], request[2]]))
            .collect::<Vec<_>>();
        assert_eq!(offsets, vec![12, 24, 36]);
    }
}
