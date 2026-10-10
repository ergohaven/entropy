use super::*;

#[cfg(not(target_arch = "wasm32"))]
fn reconcile_default_profile_from_device(
    profile: &mut crate::application_layouts::ApplicationLayout,
    device_layers: &[[u16; crate::application_layouts::APPLICATION_LAYOUT_CONTROL_COUNT];
         crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT],
    pending_edits: u16,
) -> bool {
    let mut changed = false;
    for (layer, keycodes) in device_layers.iter().enumerate() {
        if pending_edits & (1u16 << layer) != 0 {
            continue;
        }
        for (control, keycode) in keycodes.iter().enumerate() {
            changed |= profile.set_keycode(layer, control, *keycode);
        }
    }
    changed
}

pub(crate) fn application_layout_category_label(
    category: crate::application_layouts::ApplicationLayoutCategory,
    language: crate::i18n::Language,
) -> &'static str {
    use crate::application_layouts::ApplicationLayoutCategory as Category;
    let (ru, en) = match category {
        Category::Browsers => ("Браузеры", "Browsers"),
        Category::Development => ("Разработка", "Development"),
        Category::Graphics => ("Графика и 3D", "Graphics & 3D"),
        Category::Video => ("Видео и стриминг", "Video & streaming"),
        Category::Audio => ("Аудио", "Audio"),
        Category::Communication => ("Общение", "Communication"),
        Category::Other => ("Другие", "Other"),
    };
    app_layout_text(language, ru, en)
}

pub(crate) fn application_layout_category_for_executable(
    executable: &str,
) -> crate::application_layouts::ApplicationLayoutCategory {
    crate::application_layouts::application_layout_category_for_executable(executable)
}

pub(super) fn device_supports_application_layouts(device: &crate::device::Device) -> bool {
    device.firmware == FirmwareProtocol::Vial && device.is_ergohaven_display_macropad()
}

impl EntropyApp {
    fn connected_application_layout_device_key(&self) -> Option<String> {
        let device = self
            .selected_device
            .and_then(|index| self.device_manager.devices().get(index))
            .filter(|device| {
                device_supports_application_layouts(device)
                    && self.device_about_info.as_ref().is_some_and(|info| {
                        info.supports_application_layouts
                            && info.vendor_id == device.vendor_id
                            && info.product_id == device.product_id
                            && info.path == device.path
                    })
            })?;
        // v2 and v3 currently advertise the same Vial UID, but their
        // application profiles must not overwrite one another. Keep the
        // original v3 key so existing v3 profiles remain available.
        let key = match self.current_keyboard_id {
            Some(id) => format!("vial-{id:016x}"),
            None => device_id_slug(&self.current_device_name),
        };
        Some(if device.is_m4cr0pad_v3() {
            key
        } else {
            format!("{key}-m4cr0pad-v2")
        })
    }

    pub(super) fn application_layouts_supported(&self) -> bool {
        self.connected_application_layout_device_key().is_some()
            || (self.selected_device.is_none()
                && self
                    .app_settings
                    .last_application_layout_device_key
                    .as_ref()
                    .is_some_and(|key| self.app_settings.application_layouts.contains_key(key)))
    }

    pub(super) fn application_layout_device_key(&self) -> Option<String> {
        if let Some(key) = self.connected_application_layout_device_key() {
            return Some(key);
        }
        self.selected_device
            .is_none()
            .then(|| self.app_settings.last_application_layout_device_key.clone())?
    }

    pub(super) fn remember_connected_application_layout_device(&mut self) {
        let Some(key) = self.connected_application_layout_device_key() else {
            return;
        };
        let name = self.current_device_name.trim().to_owned();
        let name = (!name.is_empty()).then_some(name);
        let changed = self
            .app_settings
            .last_application_layout_device_key
            .as_deref()
            != Some(key.as_str())
            || self.app_settings.last_application_layout_device_name != name;
        if changed {
            self.app_settings.last_application_layout_device_key = Some(key);
            self.app_settings.last_application_layout_device_name = name;
            save_app_settings(&self.app_settings);
        }
    }

    pub(super) fn offline_application_layouts_available(&self) -> bool {
        self.selected_device.is_none()
            && self
                .app_settings
                .last_application_layout_device_key
                .as_ref()
                .is_some_and(|key| self.app_settings.application_layouts.contains_key(key))
    }

    pub(super) fn application_layout_settings(
        &self,
    ) -> Option<&crate::application_layouts::DeviceApplicationLayouts> {
        let key = self.application_layout_device_key()?;
        self.app_settings.application_layouts.get(&key)
    }

    pub(super) fn application_layout_settings_mut(
        &mut self,
    ) -> Option<&mut crate::application_layouts::DeviceApplicationLayouts> {
        let key = self.application_layout_device_key()?;
        Some(
            self.app_settings
                .application_layouts
                .entry(key)
                .or_default(),
        )
    }

    pub(super) fn application_layout_editor_options(&self) -> Vec<(String, String)> {
        let mut layouts = self
            .application_layout_settings()
            .map(|settings| {
                settings
                    .layouts
                    .values()
                    .map(|layout| (layout.id.clone(), layout.name.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        layouts.sort_by(|left, right| {
            let left_default = left.0 == crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID;
            let right_default =
                right.0 == crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID;
            right_default
                .cmp(&left_default)
                .then_with(|| left.1.to_lowercase().cmp(&right.1.to_lowercase()))
        });
        layouts
    }

    pub(super) fn application_layout_editor_grouped_options(
        &self,
        options: &[(String, String)],
    ) -> Vec<(String, Vec<(String, String)>)> {
        let Some(settings) = self.application_layout_settings() else {
            return Vec::new();
        };
        self.application_layout_category_choices(self.app_settings.language)
            .into_iter()
            .filter_map(|(category_id, _)| {
                let entries = options
                    .iter()
                    .filter(|(id, _)| {
                        id != crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID
                    })
                    .filter(|(id, _)| {
                        settings.layouts.get(id).is_some_and(|layout| {
                            settings.category_id_for_layout(layout) == category_id
                        })
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                (!entries.is_empty()).then_some((category_id, entries))
            })
            .collect()
    }

    pub(super) fn application_layout_editor_labeled_groups(
        &self,
        options: &[(String, String)],
        language: crate::i18n::Language,
    ) -> Vec<(String, Vec<(String, String)>)> {
        let labels = self
            .application_layout_category_choices(language)
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        self.application_layout_editor_grouped_options(options)
            .into_iter()
            .filter_map(|(id, entries)| labels.get(&id).map(|label| (label.clone(), entries)))
            .collect()
    }

    pub(super) fn application_layout_category_choices(
        &self,
        language: crate::i18n::Language,
    ) -> Vec<(String, String)> {
        use crate::application_layouts::ApplicationLayoutCategory as Category;
        let Some(settings) = self.application_layout_settings() else {
            return Vec::new();
        };
        Category::ALL
            .iter()
            .filter(|category| settings.category_exists(category.id()))
            .map(|category| {
                (
                    category.id().to_owned(),
                    settings
                        .category_names
                        .get(category.id())
                        .cloned()
                        .unwrap_or_else(|| {
                            application_layout_category_label(*category, language).to_owned()
                        }),
                )
            })
            .chain(
                settings
                    .category_names
                    .iter()
                    .filter(|(id, _)| id.starts_with("custom:"))
                    .map(|(id, name)| (id.clone(), name.clone())),
            )
            .collect()
    }

    pub(super) fn activate_application_layout(&mut self, id: &str) -> bool {
        // Finish drafts against the profile where editing started before the
        // editor selection changes. Otherwise an application-focus transition
        // can redirect a layer-name draft to the newly selected profile.
        self.commit_pending_application_layout_edits();
        let Some(device_key) = self.application_layout_device_key() else {
            return false;
        };
        let (changed, active_changed) = self
            .application_layout_settings_mut()
            .map(|settings| {
                if !settings.layouts.contains_key(id) {
                    return (false, false);
                }
                let active_changed = settings.active_layout_id != id;
                let editor_changed = settings.editor_layout_id != id;
                if active_changed {
                    settings.active_layout_id = id.to_owned();
                }
                if editor_changed {
                    settings.editor_layout_id = id.to_owned();
                }
                (active_changed || editor_changed, active_changed)
            })
            .unwrap_or((false, false));
        if changed {
            self.selected_layer = 0;
            self.selected_key = None;
            self.selected_encoder = None;
            if active_changed {
                self.reset_matrix_tester_state();
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.application_layout_manual_override =
                    Some((device_key, self.application_discovery.foreground.clone()));
            }
            save_app_settings(&self.app_settings);
        }
        changed
    }

    pub(super) fn select_application_layout_for_editing(&mut self, id: &str) -> bool {
        self.commit_pending_application_layout_edits();
        let changed = self
            .application_layout_settings_mut()
            .is_some_and(|settings| {
                if !settings.layouts.contains_key(id) || settings.editor_layout_id == id {
                    return false;
                }
                settings.editor_layout_id = id.to_owned();
                true
            });
        if changed {
            save_app_settings(&self.app_settings);
        }
        changed
    }

    pub(super) fn application_layout_editor_layer_names(&self) -> Vec<String> {
        self.application_layout_settings()
            .and_then(|settings| settings.active_layout())
            .map(|layout| layout.layer_names.clone())
            .unwrap_or_else(crate::application_layouts::default_layer_names)
    }

    pub(super) fn rename_application_layout_layer(
        &mut self,
        layout_id: &str,
        layer: usize,
        name: String,
    ) -> bool {
        let changed = self
            .application_layout_settings_mut()
            .and_then(|settings| settings.layouts.get_mut(layout_id))
            .is_some_and(|layout| layout.set_layer_name(layer, name));
        if changed {
            save_app_settings(&self.app_settings);
        }
        changed
    }

    pub(super) fn commit_pending_application_layout_edits(&mut self) {
        let (Some(layer), Some(layout_id)) =
            (self.editing_layer, self.editing_layer_layout_id.clone())
        else {
            return;
        };
        let name = self.editing_layer_text.trim().to_owned();
        if !name.is_empty() {
            self.rename_application_layout_layer(&layout_id, layer, name);
        }
        self.editing_layer = None;
        self.editing_layer_text.clear();
        self.editing_layer_focus_requested = false;
        self.editing_layer_layout_id = None;
    }

    fn application_control_for_key(layout: &KeyboardLayout, key_index: usize) -> Option<usize> {
        let key = layout.keys.get(key_index)?;
        match (key.row, key.col) {
            (0, 2) => Some(12),
            (1..=4, 0..=2) => Some((usize::from(key.row) - 1) * 3 + usize::from(key.col)),
            _ => None,
        }
    }

    fn application_control_for_encoder(
        layout: &KeyboardLayout,
        visual_index: usize,
    ) -> Option<usize> {
        let encoder = layout.encoders.get(visual_index)?;
        if encoder.encoder_idx != 0 || encoder.direction > 1 {
            return None;
        }
        Some(13 + usize::from(encoder.direction))
    }

    fn base_application_layers(
        layout: &KeyboardLayout,
    ) -> [[u16; crate::application_layouts::APPLICATION_LAYOUT_CONTROL_COUNT];
           crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT] {
        let mut layers = [[0u16; crate::application_layouts::APPLICATION_LAYOUT_CONTROL_COUNT];
            crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT];
        for layer in 0..crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT {
            for (key_index, _) in layout.keys.iter().enumerate() {
                if let Some(control) = Self::application_control_for_key(layout, key_index) {
                    layers[layer][control] =
                        layout.get_key_binding(layer, key_index).vial_keycode();
                }
            }
            for (visual_index, _) in layout.encoders.iter().enumerate() {
                if let Some(control) = Self::application_control_for_encoder(layout, visual_index) {
                    layers[layer][control] = layout.get_encoder_keycode(layer, visual_index);
                }
            }
        }
        layers
    }

    fn application_layout_rendered_copy_for_profile(
        layout: &KeyboardLayout,
        profile: Option<&crate::application_layouts::ApplicationLayout>,
    ) -> KeyboardLayout {
        let mut rendered = layout.clone();
        let Some(profile) = profile else {
            return rendered;
        };
        for layer in 0..crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT {
            let Some(keycodes) = profile.layers.get(layer) else {
                continue;
            };
            for key_index in 0..rendered.keys.len() {
                if let Some(control) = Self::application_control_for_key(&rendered, key_index) {
                    rendered.set_key_binding(
                        layer,
                        key_index,
                        crate::keyboard::KeyBinding::Vial(keycodes[control]),
                    );
                }
            }
            for visual_index in 0..rendered.encoders.len() {
                if let Some(control) =
                    Self::application_control_for_encoder(&rendered, visual_index)
                {
                    rendered.set_encoder_keycode(layer, visual_index, keycodes[control]);
                }
            }
        }
        rendered
    }

    pub(super) fn application_layout_rendered_copy(
        &self,
        layout: &KeyboardLayout,
    ) -> KeyboardLayout {
        Self::application_layout_rendered_copy_for_profile(
            layout,
            self.application_layout_settings()
                .and_then(|settings| settings.active_layout()),
        )
    }

    /// Write only a user-edited Default layer. A cached profile is never
    /// authority for a device keymap that may have changed in Vial while
    /// Entropy was closed.
    #[cfg(not(target_arch = "wasm32"))]
    fn write_default_application_layer_to_device(&mut self, layer: usize) -> bool {
        use crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID;

        if self.hid_device.is_none() || self.hid_user_action_busy() {
            return false;
        }
        let Some(profile) = self
            .application_layout_settings()
            .and_then(|settings| settings.layouts.get(DEFAULT_APPLICATION_LAYOUT_ID))
        else {
            return false;
        };
        let Some(layout) = self.layout.as_ref() else {
            return false;
        };
        let Some(keycodes) = profile.layers.get(layer) else {
            return false;
        };
        let snapshot = Self::default_application_layer_snapshot(layout, layer, keycodes);
        if snapshot
            .keycodes
            .iter()
            .enumerate()
            .all(|(index, binding)| layout.get_key_binding(layer, index) == *binding)
            && snapshot
                .encoder_keycodes
                .iter()
                .enumerate()
                .all(|(index, keycode)| layout.get_encoder_keycode(layer, index) == *keycode)
        {
            return false;
        }
        self.apply_layer_snapshot(layer, snapshot, "layer_actions.save_default_to_device");
        true
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn default_application_layer_snapshot(
        layout: &KeyboardLayout,
        layer: usize,
        keycodes: &[u16; crate::application_layouts::APPLICATION_LAYOUT_CONTROL_COUNT],
    ) -> super::layer_operations::LayerSnapshot {
        super::layer_operations::LayerSnapshot {
            keycodes: layout
                .keys
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    Self::application_control_for_key(layout, index)
                        .map(|control| crate::keyboard::KeyBinding::Vial(keycodes[control]))
                        .unwrap_or_else(|| layout.get_key_binding(layer, index))
                })
                .collect(),
            encoder_keycodes: layout
                .encoders
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    Self::application_control_for_encoder(layout, index)
                        .map(|control| keycodes[control])
                        .unwrap_or_else(|| layout.get_encoder_keycode(layer, index))
                })
                .collect(),
        }
    }

    /// On reconnect the firmware keymap wins over the locally cached Default
    /// profile. Only an explicit Entropy edit may write a layer back.
    #[cfg(not(target_arch = "wasm32"))]
    fn maybe_sync_default_application_layout_to_device(&mut self) {
        if self.hid_device.is_none() || !self.deferred_device_load.all_layers_ready() {
            return;
        }
        if !self.default_layout_device_reconciled {
            let Some(layout) = self.layout.as_ref() else {
                return;
            };
            let device_layers = Self::base_application_layers(layout);
            let pending = self.default_layout_pending_layers;
            let changed = self
                .application_layout_settings_mut()
                .and_then(|settings| {
                    settings
                        .layouts
                        .get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
                })
                .is_some_and(|profile| {
                    reconcile_default_profile_from_device(profile, &device_layers, pending)
                });
            self.default_layout_device_reconciled = true;
            if changed {
                save_app_settings(&self.app_settings);
            }
        }
        if self.hid_user_action_busy()
            || self
                .default_layout_sync_retry_after
                .is_some_and(|retry_after| std::time::Instant::now() < retry_after)
        {
            return;
        }
        for layer in 0..crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT {
            let bit = 1u16 << layer;
            if self.default_layout_pending_layers & bit == 0 {
                continue;
            }
            if self.write_default_application_layer_to_device(layer) {
                break;
            }
            // No difference remains, so the explicit edit is already present.
            self.default_layout_pending_layers &= !bit;
        }
    }

    pub(super) fn application_layout_active_rendered_copy(
        &self,
        layout: &KeyboardLayout,
    ) -> KeyboardLayout {
        Self::application_layout_rendered_copy_for_profile(
            layout,
            self.application_layout_settings()
                .and_then(|settings| settings.active_layout()),
        )
    }

    pub(super) fn application_layout_active_layer_names(&self) -> Vec<String> {
        self.application_layout_settings()
            .and_then(|settings| settings.active_layout())
            .map(|layout| layout.layer_names.clone())
            .unwrap_or_else(crate::application_layouts::default_layer_names)
    }

    pub(super) fn application_layout_current_key_binding(
        &self,
        key_index: usize,
    ) -> Option<crate::keyboard::KeyBinding> {
        if !self.application_layout_editor_active {
            return None;
        }
        let layout = self.layout.as_ref()?;
        let control = Self::application_control_for_key(layout, key_index)?;
        self.application_layout_settings()
            .and_then(|settings| settings.active_layout())
            .and_then(|profile| profile.layers.get(self.selected_layer))
            .map(|keycodes| crate::keyboard::KeyBinding::Vial(keycodes[control]))
    }

    pub(super) fn application_layout_current_encoder_keycode(
        &self,
        visual_index: usize,
    ) -> Option<u16> {
        if !self.application_layout_editor_active {
            return None;
        }
        let layout = self.layout.as_ref()?;
        let control = Self::application_control_for_encoder(layout, visual_index)?;
        self.application_layout_settings()
            .and_then(|settings| settings.active_layout())
            .and_then(|profile| profile.layers.get(self.selected_layer))
            .map(|keycodes| keycodes[control])
    }

    pub(super) fn assign_application_layout_key(
        &mut self,
        key_index: usize,
        keycode: u16,
    ) -> Option<bool> {
        if !self.application_layout_editor_active {
            return None;
        }
        let control = Self::application_control_for_key(self.layout.as_ref()?, key_index)?;
        Some(self.assign_application_layout_control(self.selected_layer, control, keycode))
    }

    pub(super) fn assign_application_layout_encoder(
        &mut self,
        visual_index: usize,
        keycode: u16,
    ) -> Option<bool> {
        if !self.application_layout_editor_active {
            return None;
        }
        let control = Self::application_control_for_encoder(self.layout.as_ref()?, visual_index)?;
        Some(self.assign_application_layout_control(self.selected_layer, control, keycode))
    }

    fn assign_application_layout_control(
        &mut self,
        layer: usize,
        control: usize,
        keycode: u16,
    ) -> bool {
        let previous = self.application_layout_control_undo_state(layer, control);
        #[cfg(not(target_arch = "wasm32"))]
        let is_default = self.application_layout_settings().is_some_and(|settings| {
            settings.active_layout_id == crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID
        });
        let changed = self
            .application_layout_settings_mut()
            .and_then(|settings| settings.layouts.get_mut(&settings.active_layout_id))
            .is_some_and(|layout| layout.set_keycode(layer, control, keycode));
        if changed {
            if let Some(action) = previous {
                self.undo_stack.push(action);
            }
            save_app_settings(&self.app_settings);
            self.status_msg = app_layout_text(
                self.app_settings.language,
                "Раскладка приложения сохранена",
                "Application layout saved",
            )
            .to_owned();
            #[cfg(not(target_arch = "wasm32"))]
            if is_default {
                self.default_layout_pending_layers |= 1u16 << layer;
                self.default_layout_sync_retry_after = None;
                self.maybe_sync_default_application_layout_to_device();
            }
        }
        true
    }

    fn application_layout_control_undo_state(
        &self,
        layer: usize,
        control: usize,
    ) -> Option<UndoAction> {
        let device_key = self.application_layout_device_key()?;
        let settings = self.app_settings.application_layouts.get(&device_key)?;
        let layout_id = settings.active_layout_id.clone();
        let old_keycode = *settings
            .layouts
            .get(&layout_id)?
            .layers
            .get(layer)?
            .get(control)?;
        Some(UndoAction::ApplicationLayoutControl {
            device_key,
            layout_id,
            layer,
            control,
            old_keycode,
        })
    }

    pub(super) fn undo_application_layout_control(
        &mut self,
        device_key: &str,
        layout_id: &str,
        layer: usize,
        control: usize,
        old_keycode: u16,
    ) {
        let changed = self
            .app_settings
            .application_layouts
            .get_mut(device_key)
            .and_then(|settings| settings.layouts.get_mut(layout_id))
            .is_some_and(|layout| layout.set_keycode(layer, control, old_keycode));
        if changed {
            save_app_settings(&self.app_settings);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn update_application_layout_runtime(&mut self) {
        let discovered = crate::app_discovery::application_discovery_snapshot();
        let foreground_changed = self.application_layout_foreground != discovered.foreground;
        if foreground_changed {
            // Commit while editor_layout_id still points at the source profile.
            // The runtime may select another profile later in this update.
            self.commit_pending_application_layout_edits();
        }
        self.application_layout_foreground = discovered.foreground.clone();
        self.application_discovery = discovered;

        let Some(device_key) = self.application_layout_device_key() else {
            let inactive = crate::application_layouts::ApplicationLayoutSnapshot::inactive();
            for bridge in self
                .qmk_hid_hosts
                .values()
                .filter(|bridge| bridge.supports_application_layouts())
            {
                bridge.set_application_layout_snapshot(inactive.clone());
            }
            return;
        };
        let seed = self.layout.as_ref().map(Self::base_application_layers);
        let foreground_status = self.application_discovery.foreground_status.clone();
        let manual_override_active = self
            .application_layout_manual_override
            .as_ref()
            .is_some_and(|(override_device_key, override_foreground)| {
                override_device_key == &device_key
                    && same_foreground_application(
                        override_foreground.as_ref(),
                        self.application_discovery.foreground.as_ref(),
                    )
            });
        if !manual_override_active {
            self.application_layout_manual_override = None;
        }
        let installed_presets = crate::app_discovery::installed_builtin_presets();
        let mut persist = false;
        let (snapshot, active_layout_followed, active_layout_changed) = {
            let settings = self
                .app_settings
                .application_layouts
                .entry(device_key)
                .or_default();
            persist |= settings.normalize();
            persist |= settings.provision_builtin_presets(installed_presets.as_ref().ok());
            persist |=
                settings.enrich_application_identities(&self.application_discovery.available);
            if let Some(seed) = seed {
                for layout in settings.layouts.values_mut() {
                    persist |= layout.seed_unset_keycodes(seed);
                }
            }
            let resolved = resolve_layout_for_runtime(
                settings,
                &foreground_status.state,
                manual_override_active,
            );
            let focused_layout_is_configured =
                should_follow_focused_layout(settings, &resolved, &foreground_status.state);
            let previous_active_layout_id = settings.active_layout_id.clone();
            let active_layout_followed = apply_resolved_layout(
                settings,
                resolved,
                foreground_changed && focused_layout_is_configured,
            );
            let active_layout_changed = settings.active_layout_id != previous_active_layout_id;
            let snapshot = settings
                .active_layout()
                .map(crate::application_layouts::ApplicationLayoutSnapshot::from_layout)
                .unwrap_or_else(crate::application_layouts::ApplicationLayoutSnapshot::inactive);
            (snapshot, active_layout_followed, active_layout_changed)
        };
        if active_layout_changed {
            // Firmware releases held keys and starts every newly activated
            // application layout on layer 0. Keep the independent Layout
            // Indicator state on that same source of truth.
            self.reset_matrix_tester_state();
        }
        if active_layout_followed {
            // Firmware starts an application layout on layer 0. Make the
            // editor show that same layer and clear stale controls.
            self.selected_layer = 0;
            self.selected_key = None;
            self.selected_encoder = None;
        }
        if persist {
            save_app_settings(&self.app_settings);
        }

        self.maybe_sync_default_application_layout_to_device();

        self.publish_application_layout_snapshot(snapshot, false);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn publish_application_layout_snapshot(
        &self,
        snapshot: crate::application_layouts::ApplicationLayoutSnapshot,
        force_resend: bool,
    ) {
        let Some(path) = self
            .selected_device
            .and_then(|index| self.device_manager.devices().get(index))
            .map(|device| device.path.as_str())
        else {
            return;
        };
        let Some(bridge) = self.qmk_hid_hosts.get(path) else {
            return;
        };
        bridge.set_application_layout_snapshot(snapshot);
        if force_resend {
            bridge.force_application_layout_resend();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn force_current_application_layout_resend(&self) {
        let snapshot = self
            .application_layout_settings()
            .and_then(|settings| settings.active_layout())
            .map(crate::application_layouts::ApplicationLayoutSnapshot::from_layout)
            .unwrap_or_else(crate::application_layouts::ApplicationLayoutSnapshot::inactive);
        self.publish_application_layout_snapshot(snapshot, true);
    }
}

fn same_foreground_application(
    left: Option<&crate::application_layouts::DetectedApplication>,
    right: Option<&crate::application_layouts::DetectedApplication>,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => crate::application_layouts::application_identities_match(
            std::iter::once(left.executable.as_str())
                .chain(left.identities.iter().map(String::as_str)),
            std::iter::once(right.executable.as_str())
                .chain(right.identities.iter().map(String::as_str)),
        ),
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
    }
}

fn resolve_layout_for_runtime(
    settings: &crate::application_layouts::DeviceApplicationLayouts,
    foreground: &crate::app_discovery::ForegroundState,
    manual_override_active: bool,
) -> String {
    if manual_override_active {
        settings.active_layout_id.clone()
    } else {
        resolve_layout_for_foreground(settings, foreground)
    }
}

fn resolve_layout_for_foreground(
    settings: &crate::application_layouts::DeviceApplicationLayouts,
    foreground: &crate::app_discovery::ForegroundState,
) -> String {
    match foreground {
        crate::app_discovery::ForegroundState::Focused(application) => {
            settings.resolve(Some(application))
        }
        crate::app_discovery::ForegroundState::UnidentifiedWindow(_)
        | crate::app_discovery::ForegroundState::NoFocusedWindow => settings.resolve(None),
        crate::app_discovery::ForegroundState::BackendUnavailable(_) => {
            // A detector failure is not an unknown application. Keep the last
            // confirmed layout until the backend recovers.
            settings.active_layout_id.clone()
        }
    }
}

fn should_follow_focused_layout(
    settings: &crate::application_layouts::DeviceApplicationLayouts,
    resolved: &str,
    foreground: &crate::app_discovery::ForegroundState,
) -> bool {
    if !settings.automatic_switching_enabled {
        return false;
    }
    match foreground {
        crate::app_discovery::ForegroundState::Focused(application) => settings
            .layouts
            .get(resolved)
            .is_some_and(|layout| layout.automatic_switching && layout.matches(application)),
        _ => false,
    }
}

fn apply_resolved_layout(
    settings: &mut crate::application_layouts::DeviceApplicationLayouts,
    resolved: String,
    follow_editor_for_foreground_transition: bool,
) -> bool {
    let active_layout_changed = settings.active_layout_id != resolved;
    if active_layout_changed {
        settings.active_layout_id = resolved.clone();
    }

    // Keep the Layout page in sync once when the foreground application
    // changes. Re-applying this on every runtime tick would immediately undo
    // a layout the user selected manually for editing.
    let should_follow_editor = active_layout_changed || follow_editor_for_foreground_transition;
    let editor_layout_changed = should_follow_editor && settings.editor_layout_id != resolved;
    if editor_layout_changed {
        settings.editor_layout_id = resolved;
    }
    active_layout_changed || editor_layout_changed
}

pub(super) fn app_layout_text(
    language: crate::i18n::Language,
    russian: &'static str,
    english: &'static str,
) -> &'static str {
    match language {
        crate::i18n::Language::Russian => russian,
        crate::i18n::Language::English => english,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_keeps_external_vial_edit_instead_of_replaying_cached_default() {
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let profile = settings
            .layouts
            .get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
            .unwrap();
        profile.set_keycode(0, 0, 0x0004); // Cached before Entropy closed.
        profile.set_keycode(1, 0, 0x0006); // A new Entropy edit still pending.
        let mut device_layers = [[0u16;
            crate::application_layouts::APPLICATION_LAYOUT_CONTROL_COUNT];
            crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT];
        device_layers[0][0] = 0x0005; // Independently changed in Vial.
        device_layers[1][0] = 0x0007;

        assert!(reconcile_default_profile_from_device(
            profile,
            &device_layers,
            1u16 << 1
        ));
        assert_eq!(profile.layers[0][0], 0x0005);
        assert_eq!(profile.layers[1][0], 0x0006);
    }
    use crate::keyboard::PhysicalKey;

    fn indicator_test_layout() -> KeyboardLayout {
        KeyboardLayout {
            name: "M4CR0Pad v3".to_owned(),
            rows: 5,
            cols: 3,
            keys: vec![PhysicalKey {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
                row: 1,
                col: 0,
                label: "1,0".to_owned(),
                rotation: 0.0,
                rotation_x: 0.0,
                rotation_y: 0.0,
                layout_condition: None,
            }],
            encoders: Vec::new(),
            layers: (0..crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT)
                .map(|_| vec![crate::keyboard::KeyBinding::Vial(0x0027)])
                .collect(),
            encoder_layers: Vec::new(),
            layer_names: crate::application_layouts::default_layer_names(),
            custom_keycodes: Vec::new(),
            layout_options: Vec::new(),
            live_features: Default::default(),
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn existing_default_tap_dance_is_selected_for_standalone_keymap_write() {
        let layout = indicator_test_layout();
        let mut default_keycodes =
            [0xffff; crate::application_layouts::APPLICATION_LAYOUT_CONTROL_COUNT];
        default_keycodes[0] = 0x5700; // Vial TD0

        let snapshot =
            EntropyApp::default_application_layer_snapshot(&layout, 0, &default_keycodes);

        assert_eq!(
            snapshot.keycodes[0],
            crate::keyboard::KeyBinding::Vial(0x5700)
        );
        assert_eq!(layout.get_keycode(0, 0), 0x0027);
    }

    fn m4cr0pad_v3_device() -> crate::device::Device {
        crate::device::Device {
            name: "M4CR0Pad v3".to_owned(),
            vendor_id: 0xE126,
            product_id: 0x0042,
            manufacturer: "Ergohaven".to_owned(),
            serial_number: "indicator-test".to_owned(),
            bus_type: "Usb".to_owned(),
            path: "/dev/hidraw-indicator-test".to_owned(),
            instance_token: "indicator-test-instance".to_owned(),
            firmware: FirmwareProtocol::Vial,
        }
    }

    fn mark_application_layout_protocol_supported(app: &mut EntropyApp) {
        let device = &app.device_manager.devices()[app.selected_device.unwrap()];
        app.device_about_info = Some(DeviceAboutInfo {
            supports_application_layouts: true,
            vendor_id: device.vendor_id,
            product_id: device.product_id,
            path: device.path.clone(),
            ..Default::default()
        });
    }

    #[test]
    fn grouped_options_use_saved_override_and_legacy_preset_fallback() {
        use crate::application_layouts::{
            ApplicationLayoutCategory as Category, DetectedApplication,
        };

        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        let key = "test-grouping".to_owned();
        app.app_settings.last_application_layout_device_key = Some(key.clone());
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let add = |settings: &mut crate::application_layouts::DeviceApplicationLayouts,
                   executable: &str,
                   name: &str| {
            settings.create_for_application(&DetectedApplication {
                executable: executable.to_owned(),
                display_name: name.to_owned(),
                ..Default::default()
            })
        };
        let firefox = add(&mut settings, "firefox", "Firefox");
        let code = add(&mut settings, "code", "VS Code");
        let custom = add(&mut settings, "unknown-custom", "Custom");
        app.app_settings
            .application_layouts
            .insert(key.clone(), settings);
        let options = app.application_layout_editor_options();
        let groups = app.application_layout_editor_grouped_options(&options);
        assert!(groups
            .iter()
            .any(|(category, entries)| category == Category::Browsers.id()
                && entries.iter().any(|(id, _)| id == &firefox)));
        assert!(groups
            .iter()
            .any(|(category, entries)| category == Category::Development.id()
                && entries.iter().any(|(id, _)| id == &code)));
        assert!(groups
            .iter()
            .any(|(category, entries)| category == Category::Other.id()
                && entries.iter().any(|(id, _)| id == &custom)));
        assert!(groups
            .iter()
            .all(|(_, entries)| entries.iter().all(|(id, _)| id != "default")));

        assert!(app
            .app_settings
            .application_layouts
            .get_mut(&key)
            .unwrap()
            .set_layout_category(&firefox, Category::Audio));
        assert!(app
            .app_settings
            .application_layouts
            .get_mut(&key)
            .unwrap()
            .set_layout_category(&custom, Category::Browsers));
        let serialized =
            serde_json::to_string(&app.app_settings.application_layouts[&key]).unwrap();
        app.app_settings
            .application_layouts
            .insert(key, serde_json::from_str(&serialized).unwrap());
        let groups = app.application_layout_editor_grouped_options(&options);
        assert!(groups
            .iter()
            .any(|(category, entries)| category == Category::Audio.id()
                && entries.iter().any(|(id, _)| id == &firefox)));
        assert!(groups
            .iter()
            .any(|(category, entries)| category == Category::Browsers.id()
                && entries.iter().any(|(id, _)| id == &custom)));
        assert!(groups
            .iter()
            .any(|(category, entries)| category == Category::Development.id()
                && entries.iter().any(|(id, _)| id == &code)));
        assert_eq!(Category::ALL.len(), 7);
        assert_eq!(
            application_layout_category_label(Category::Audio, crate::i18n::Language::English),
            "Audio"
        );
        assert_eq!(
            application_layout_category_label(Category::Audio, crate::i18n::Language::Russian),
            "Аудио"
        );
        let labeled =
            app.application_layout_editor_labeled_groups(&options, crate::i18n::Language::English);
        assert!(labeled.iter().any(
            |(label, entries)| label == "Audio" && entries.iter().any(|(id, _)| id == &firefox)
        ));
    }

    #[test]
    fn editable_categories_reach_both_grouped_selectors_after_reload() {
        use crate::application_layouts::{DetectedApplication, DeviceApplicationLayouts};
        let mut app = EntropyApp::new_inert_for_test();
        let key = "offline-macropad-category-crud".to_owned();
        let mut settings = DeviceApplicationLayouts::default();
        let firefox = settings.create_for_application(&DetectedApplication {
            executable: "firefox".to_owned(),
            display_name: "Firefox".to_owned(),
            ..Default::default()
        });
        let code = settings.create_for_application(&DetectedApplication {
            executable: "code".to_owned(),
            display_name: "VS Code".to_owned(),
            ..Default::default()
        });
        let custom_id = settings.create_category("Work").unwrap();
        assert!(settings.set_layout_category_id(&firefox, &custom_id));
        assert!(settings.rename_category(&custom_id, "Projects"));
        assert!(settings.remove_category("development"));
        let encoded = serde_json::to_string(&settings).unwrap();
        app.app_settings
            .application_layouts
            .insert(key.clone(), serde_json::from_str(&encoded).unwrap());
        app.app_settings.last_application_layout_device_key = Some(key);
        let choices = app.application_layout_category_choices(crate::i18n::Language::English);
        assert!(choices
            .iter()
            .any(|(id, label)| id == &custom_id && label == "Projects"));
        assert!(!choices.iter().any(|(id, _)| id == "development"));
        let groups = app.application_layout_editor_labeled_groups(
            &app.application_layout_editor_options(),
            crate::i18n::Language::English,
        );
        assert!(groups
            .iter()
            .any(|(label, entries)| label == "Projects"
                && entries.iter().any(|(id, _)| id == &firefox)));
        assert!(groups
            .iter()
            .any(|(label, entries)| label == "Other" && entries.iter().any(|(id, _)| id == &code)));
    }

    #[test]
    fn corrupt_product_string_does_not_hide_application_layouts() {
        let device = crate::device::Device {
            name: "Ль".to_owned(),
            vendor_id: 0xE126,
            product_id: 0x0042,
            manufacturer: "Ergohaven".to_owned(),
            serial_number: "test-pad".to_owned(),
            bus_type: "Usb".to_owned(),
            path: "/dev/hidraw4".to_owned(),
            instance_token: "test-instance".to_owned(),
            firmware: FirmwareProtocol::Vial,
        };

        assert!(device_supports_application_layouts(&device));
    }

    #[test]
    fn saved_macropad_profiles_remain_available_while_device_is_offline() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        let key = "vial-0000000000000042".to_owned();
        app.app_settings.application_layouts.insert(
            key.clone(),
            crate::application_layouts::DeviceApplicationLayouts::default(),
        );
        app.app_settings.last_application_layout_device_key = Some(key.clone());
        app.app_settings.last_application_layout_device_name = Some("Macropad v3".to_owned());
        app.selected_device = None;
        app.current_keyboard_id = None;
        app.current_device_name.clear();

        assert!(app.application_layouts_supported());
        assert!(app.offline_application_layouts_available());
        assert_eq!(app.application_layout_device_key(), Some(key));
        assert!(app.application_layout_settings().is_some());
    }

    #[test]
    fn offline_macropad_profiles_do_not_override_an_incompatible_selected_device() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        let key = "vial-0000000000000042".to_owned();
        app.app_settings.application_layouts.insert(
            key.clone(),
            crate::application_layouts::DeviceApplicationLayouts::default(),
        );
        app.app_settings.last_application_layout_device_key = Some(key);
        let mut incompatible = m4cr0pad_v3_device();
        incompatible.product_id = 0x9999;
        app.device_manager.replace_devices(vec![incompatible]);
        app.selected_device = Some(0);

        assert!(!app.application_layouts_supported());
        assert!(!app.offline_application_layouts_available());
        assert!(app.application_layout_device_key().is_none());
    }

    #[test]
    fn shared_vial_uid_does_not_merge_v2_and_v3_application_profiles() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        let mut device = m4cr0pad_v3_device();
        app.current_keyboard_id = Some(0xBB17F05B02801D1D);
        app.device_manager.replace_devices(vec![device.clone()]);
        app.selected_device = Some(0);
        assert!(app.application_layout_device_key().is_none());
        app.device_about_info = Some(DeviceAboutInfo {
            supports_application_layouts: true,
            vendor_id: device.vendor_id,
            product_id: device.product_id,
            path: device.path.clone(),
            ..Default::default()
        });
        let v3_key = app.application_layout_device_key().unwrap();
        assert_eq!(v3_key, "vial-bb17f05b02801d1d");

        device.product_id = 0x0041;
        app.device_manager.replace_devices(vec![device.clone()]);
        assert!(app.application_layout_device_key().is_none());
        app.device_about_info.as_mut().unwrap().product_id = device.product_id;
        let v2_key = app.application_layout_device_key().unwrap();
        assert_eq!(v2_key, "vial-bb17f05b02801d1d-m4cr0pad-v2");
        assert_ne!(v2_key, v3_key);
    }

    #[test]
    fn v2_supports_application_layouts_without_enabling_other_devices() {
        let mut device = m4cr0pad_v3_device();
        device.name = "M4CR0Pad v2".to_owned();
        device.product_id = 0x0041;
        assert!(device_supports_application_layouts(&device));
        device.product_id = 0x0040;
        assert!(!device_supports_application_layouts(&device));
    }

    #[test]
    fn detector_failure_keeps_last_confirmed_layout() {
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let application = crate::application_layouts::DetectedApplication {
            executable: "code".to_owned(),
            identities: vec!["com.visualstudio.code".to_owned()],
            display_name: "Visual Studio Code".to_owned(),
            window_title: String::new(),
        };
        let layout_id = settings.create_for_application(&application);
        settings.active_layout_id = layout_id.clone();

        let resolved = resolve_layout_for_foreground(
            &settings,
            &crate::app_discovery::ForegroundState::BackendUnavailable("test failure".to_owned()),
        );

        assert_eq!(resolved, layout_id);
    }

    #[test]
    fn off_mode_preserves_editor_during_matching_focus_change() {
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let application = crate::application_layouts::DetectedApplication {
            executable: "code".to_owned(),
            identities: Vec::new(),
            display_name: "Code".to_owned(),
            window_title: String::new(),
        };
        let layout_id = settings.create_for_application(&application);
        settings.active_layout_id = layout_id.clone();
        settings.editor_layout_id =
            crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID.to_owned();
        let focused = crate::app_discovery::ForegroundState::Focused(application);
        settings.automatic_switching_enabled = false;
        let resolved = resolve_layout_for_foreground(&settings, &focused);
        assert_eq!(resolved, layout_id);
        assert!(!should_follow_focused_layout(
            &settings, &resolved, &focused
        ));
        assert!(!apply_resolved_layout(
            &mut settings,
            resolved.clone(),
            false
        ));
        assert_eq!(
            settings.editor_layout_id,
            crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID
        );
        settings.automatic_switching_enabled = true;
        assert!(should_follow_focused_layout(&settings, &resolved, &focused));
        assert!(apply_resolved_layout(&mut settings, resolved, true));
        assert_eq!(settings.editor_layout_id, layout_id);
    }

    #[test]
    fn no_focused_window_uses_configured_fallback_policy() {
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let application = crate::application_layouts::DetectedApplication {
            executable: "code".to_owned(),
            identities: Vec::new(),
            display_name: "Visual Studio Code".to_owned(),
            window_title: String::new(),
        };
        let layout_id = settings.create_for_application(&application);
        settings.active_layout_id = layout_id.clone();

        assert_eq!(
            resolve_layout_for_foreground(
                &settings,
                &crate::app_discovery::ForegroundState::NoFocusedWindow,
            ),
            crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID
        );
        settings.automatically_return_to_default = false;
        assert_eq!(
            resolve_layout_for_foreground(
                &settings,
                &crate::app_discovery::ForegroundState::NoFocusedWindow,
            ),
            layout_id
        );
    }

    #[test]
    fn active_application_transition_updates_the_layout_editor() {
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let vscode =
            settings.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: "code".to_owned(),
                identities: vec!["com.visualstudio.code".to_owned()],
                display_name: "Visual Studio Code".to_owned(),
                window_title: String::new(),
            });
        let telegram =
            settings.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: "telegram-desktop".to_owned(),
                identities: vec!["org.telegram.desktop".to_owned()],
                display_name: "Telegram".to_owned(),
                window_title: String::new(),
            });
        settings.active_layout_id = vscode.clone();
        settings.editor_layout_id = vscode;

        assert!(apply_resolved_layout(&mut settings, telegram.clone(), true));
        assert_eq!(settings.active_layout_id, telegram);
        assert_eq!(settings.editor_layout_id, settings.active_layout_id);
    }

    #[test]
    fn manual_editor_selection_is_kept_without_an_active_layout_transition() {
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let vscode =
            settings.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: "code".to_owned(),
                identities: Vec::new(),
                display_name: "Visual Studio Code".to_owned(),
                window_title: String::new(),
            });
        let telegram =
            settings.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: "telegram-desktop".to_owned(),
                identities: Vec::new(),
                display_name: "Telegram".to_owned(),
                window_title: String::new(),
            });
        settings.active_layout_id = vscode.clone();
        settings.editor_layout_id = telegram.clone();

        assert!(!apply_resolved_layout(&mut settings, vscode.clone(), false));
        assert_eq!(settings.active_layout_id, vscode);
        assert_eq!(settings.editor_layout_id, telegram);
    }

    #[test]
    fn focused_configured_app_repairs_a_stale_editor_selection_on_startup() {
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let vscode =
            settings.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: "code".to_owned(),
                identities: Vec::new(),
                display_name: "Visual Studio Code".to_owned(),
                window_title: String::new(),
            });
        settings.active_layout_id = vscode.clone();
        settings.editor_layout_id =
            crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID.to_owned();

        assert!(apply_resolved_layout(&mut settings, vscode.clone(), true));
        assert_eq!(settings.active_layout_id, vscode);
        assert_eq!(settings.editor_layout_id, settings.active_layout_id);
    }

    #[test]
    fn configured_app_follows_once_then_keeps_a_manual_editor_selection() {
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let ticktick =
            settings.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: "ticktick".to_owned(),
                identities: vec!["ticktick_ticktick.desktop".to_owned()],
                display_name: "TickTick".to_owned(),
                window_title: String::new(),
            });
        let telegram =
            settings.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: "telegram-desktop".to_owned(),
                identities: Vec::new(),
                display_name: "Telegram".to_owned(),
                window_title: String::new(),
            });
        settings.active_layout_id =
            crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID.to_owned();
        settings.editor_layout_id =
            crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID.to_owned();

        // A real foreground transition follows TickTick once.
        assert!(apply_resolved_layout(&mut settings, ticktick.clone(), true));
        assert_eq!(settings.active_layout_id, ticktick);
        assert_eq!(settings.editor_layout_id, settings.active_layout_id);

        // Selecting another profile for editing must survive later runtime
        // ticks while TickTick remains the focused application.
        settings.editor_layout_id = telegram.clone();
        let still_active = settings.active_layout_id.clone();
        assert!(!apply_resolved_layout(&mut settings, still_active, false));
        assert_eq!(settings.editor_layout_id, telegram);
    }

    #[test]
    fn main_layout_selection_activates_editor_indicator_and_runtime_snapshot() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        app.device_manager
            .replace_devices(vec![m4cr0pad_v3_device()]);
        app.selected_device = Some(0);
        app.current_device_name = "M4CR0Pad v3".to_owned();
        mark_application_layout_protocol_supported(&mut app);
        app.selected_layer = 7;
        app.application_discovery.foreground =
            Some(crate::application_layouts::DetectedApplication {
                executable: "entropy".to_owned(),
                identities: vec!["works.eh.Entropy".to_owned()],
                display_name: "Entropy".to_owned(),
                window_title: "Layout".to_owned(),
            });

        let device_key = app
            .application_layout_device_key()
            .expect("M4CR0Pad v3 must support application layouts");
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let calculator =
            settings.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: "calculator".to_owned(),
                identities: vec!["org.gnome.Calculator".to_owned()],
                display_name: "Calculator".to_owned(),
                window_title: String::new(),
            });
        settings
            .layouts
            .get_mut(&calculator)
            .unwrap()
            .set_layer_name(0, "каль".to_owned());
        app.app_settings
            .application_layouts
            .insert(device_key.clone(), settings);

        assert!(app.activate_application_layout(&calculator));
        let settings = app
            .app_settings
            .application_layouts
            .get(&device_key)
            .unwrap();
        assert_eq!(settings.editor_layout_id, calculator);
        assert_eq!(settings.active_layout_id, calculator);
        assert_eq!(app.selected_layer, 0);
        assert_eq!(app.application_layout_active_layer_names()[0], "каль");
        assert_eq!(
            app.application_layout_manual_override,
            Some((device_key, app.application_discovery.foreground.clone()))
        );
    }

    #[test]
    fn profile_change_commits_layer_drafts_to_the_profile_where_editing_started() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        app.device_manager
            .replace_devices(vec![m4cr0pad_v3_device()]);
        app.selected_device = Some(0);
        app.current_device_name = "M4CR0Pad v3".to_owned();
        mark_application_layout_protocol_supported(&mut app);

        let device_key = app.application_layout_device_key().unwrap();
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let telegram =
            settings.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: "telegram".to_owned(),
                identities: vec!["org.telegram.desktop".to_owned()],
                display_name: "Telegram".to_owned(),
                window_title: String::new(),
            });
        let calculator =
            settings.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: "calculator".to_owned(),
                identities: vec!["org.gnome.Calculator".to_owned()],
                display_name: "Calculator".to_owned(),
                window_title: String::new(),
            });
        settings.editor_layout_id = telegram.clone();
        settings.active_layout_id = telegram.clone();
        app.app_settings
            .application_layouts
            .insert(device_key.clone(), settings);

        app.editing_layer = Some(3);
        app.editing_layer_text = "Calls".to_owned();
        app.editing_layer_layout_id = Some(telegram.clone());

        assert!(app.activate_application_layout(&calculator));

        let settings = &app.app_settings.application_layouts[&device_key];
        assert_eq!(settings.layouts[&telegram].layer_names[3], "Calls");
        assert_ne!(settings.layouts[&calculator].layer_names[3], "Calls");
        assert_eq!(settings.editor_layout_id, calculator);
        assert!(app.editing_layer.is_none());
        assert!(app.editing_layer_layout_id.is_none());
    }

    #[test]
    fn manual_activation_is_kept_until_a_different_application_gets_focus() {
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let calculator =
            settings.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: "calculator".to_owned(),
                identities: vec!["org.gnome.Calculator".to_owned()],
                display_name: "Calculator".to_owned(),
                window_title: String::new(),
            });
        let telegram =
            settings.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: "telegram".to_owned(),
                identities: vec!["org.telegram.desktop".to_owned()],
                display_name: "Telegram".to_owned(),
                window_title: String::new(),
            });
        settings.active_layout_id = calculator.clone();

        let telegram_foreground = crate::app_discovery::ForegroundState::Focused(
            crate::application_layouts::DetectedApplication {
                executable: "telegram".to_owned(),
                identities: vec!["org.telegram.desktop".to_owned()],
                display_name: "Telegram".to_owned(),
                window_title: "Chat".to_owned(),
            },
        );
        assert_eq!(
            resolve_layout_for_runtime(&settings, &telegram_foreground, true),
            calculator
        );
        assert_eq!(
            resolve_layout_for_runtime(&settings, &telegram_foreground, false),
            telegram
        );
    }

    #[test]
    fn changing_only_the_window_title_does_not_end_a_manual_override() {
        let first = crate::application_layouts::DetectedApplication {
            executable: "com.apple.finder".to_owned(),
            identities: vec!["Finder".to_owned()],
            display_name: "Finder".to_owned(),
            window_title: "Downloads".to_owned(),
        };
        let second = crate::application_layouts::DetectedApplication {
            window_title: "Documents".to_owned(),
            ..first.clone()
        };
        let other = crate::application_layouts::DetectedApplication {
            executable: "com.apple.Safari".to_owned(),
            identities: vec!["Safari".to_owned()],
            display_name: "Safari".to_owned(),
            window_title: String::new(),
        };

        assert!(same_foreground_application(Some(&first), Some(&second)));
        assert!(!same_foreground_application(Some(&first), Some(&other)));
    }

    #[test]
    fn settings_selection_does_not_change_main_or_device_layout() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        app.device_manager
            .replace_devices(vec![m4cr0pad_v3_device()]);
        app.selected_device = Some(0);
        app.current_device_name = "M4CR0Pad v3".to_owned();
        mark_application_layout_protocol_supported(&mut app);

        let device_key = app
            .application_layout_device_key()
            .expect("M4CR0Pad v3 must support application layouts");
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let editor =
            settings.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: "code".to_owned(),
                identities: Vec::new(),
                display_name: "VS Code".to_owned(),
                window_title: String::new(),
            });
        let active =
            settings.create_for_application(&crate::application_layouts::DetectedApplication {
                executable: "gnome-calculator".to_owned(),
                identities: Vec::new(),
                display_name: "Calculator".to_owned(),
                window_title: String::new(),
            });
        settings
            .layouts
            .get_mut(&editor)
            .unwrap()
            .set_keycode(0, 0, 0x0004);
        let active_profile = settings.layouts.get_mut(&active).unwrap();
        active_profile.set_keycode(0, 0, 0x0005);
        active_profile.set_layer_name(0, "каль".to_owned());
        settings.editor_layout_id =
            crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID.to_owned();
        settings.active_layout_id = active.clone();
        app.app_settings
            .application_layouts
            .insert(device_key.clone(), settings);

        app.selected_layer = 3;
        assert!(app.select_application_layout_for_editing(&editor));
        let settings = &app.app_settings.application_layouts[&device_key];
        assert_eq!(settings.editor_layout_id, editor);
        assert_eq!(settings.active_layout_id, active);
        assert_eq!(app.selected_layer, 3);
        assert!(app.application_layout_manual_override.is_none());

        let base = indicator_test_layout();
        let main_rendered = app.application_layout_rendered_copy(&base);
        let indicator_rendered = app.application_layout_active_rendered_copy(&base);

        assert_eq!(main_rendered.get_keycode(0, 0), 0x0005);
        assert_eq!(indicator_rendered.get_keycode(0, 0), 0x0005);
        assert_eq!(app.application_layout_active_layer_names()[0], "каль");

        assert!(app.assign_application_layout_control(0, 0, 0x0006));
        assert!(app.assign_application_layout_control(0, 0, 0x0007));
        let settings = &app.app_settings.application_layouts[&device_key];
        assert_eq!(settings.layouts[&active].layers[0][0], 0x0007);
        assert_eq!(settings.layouts[&editor].layers[0][0], 0x0004);

        app.undo(&egui::Context::default());
        assert_eq!(
            app.app_settings.application_layouts[&device_key].layouts[&active].layers[0][0],
            0x0006
        );
        app.app_settings
            .application_layouts
            .get_mut(&device_key)
            .unwrap()
            .active_layout_id = editor.clone();
        app.undo(&egui::Context::default());
        let settings = &app.app_settings.application_layouts[&device_key];
        assert_eq!(settings.layouts[&active].layers[0][0], 0x0005);
        assert_eq!(settings.layouts[&editor].layers[0][0], 0x0004);
        assert_eq!(settings.active_layout_id, editor);
    }
}
