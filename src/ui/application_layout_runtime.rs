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
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(bridge) = self
            .selected_device
            .and_then(|index| self.device_manager.devices().get(index))
            .and_then(|device| self.qmk_hid_hosts.get_mut(&device.path))
        {
            if bridge.uses_shared_output()
                && bridge.matches_shared_output(self.shared_hid_output.as_ref())
                && self
                    .selected_device
                    .and_then(|index| self.device_manager.devices().get(index))
                    .is_some_and(|device| device.permits_hid_target(bridge.device()))
            {
                bridge.bind_selected_application_layout_device(key.clone());
            }
        }
        let name = self.current_device_name.trim().to_owned();
        let name = (!name.is_empty()).then_some(name);
        let binding_changed = self
            .selected_device
            .and_then(|index| self.device_manager.devices().get(index))
            .filter(|device| !device.serial_number.trim().is_empty())
            .is_some_and(|device| {
                let identity = (
                    device.vendor_id,
                    device.product_id,
                    device.serial_number.clone(),
                );
                if self.app_settings.application_layout_bindings.get(&key) == Some(&identity) {
                    false
                } else {
                    self.app_settings
                        .application_layout_bindings
                        .insert(key.clone(), identity);
                    true
                }
            });
        let changed = binding_changed
            || self
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

    #[cfg(not(target_arch = "wasm32"))]
    fn restore_background_application_layout_bindings(&mut self) {
        let devices = self.device_manager.devices();
        let mut changed = false;
        for bridge in self.qmk_hid_hosts.values_mut() {
            if !bridge.supports_application_layouts() || bridge.uses_shared_output() {
                continue;
            }
            // Only the identified selected owner may retain a serial-less binding.
            // A fresh open revokes this ephemeral evidence in the worker.
            if bridge.has_selected_application_layout_binding()
                && devices
                    .iter()
                    .any(|d| d.permits_hid_target(bridge.device()))
                && bridge
                    .application_layout_device_key()
                    .is_some_and(|key| self.app_settings.application_layouts.contains_key(key))
            {
                continue;
            }
            let Some((key, generation)) = bridge.discovered_application_layout_identity() else {
                bridge.clear_application_layout_binding();
                continue;
            };
            let target = bridge.device();
            // A model UID is not a unit serial. Two endpoints of the same revision
            // are ambiguous even before the second worker has finished discovery.
            if !devices.iter().any(|d| d.permits_hid_target(target))
                || devices
                    .iter()
                    .filter(|d| {
                        d.vendor_id == target.vendor_id && d.product_id == target.product_id
                    })
                    .count()
                    != 1
                || target.serial_number.trim().is_empty()
                || !self.app_settings.application_layouts.contains_key(&key)
            {
                bridge.clear_application_layout_binding();
                continue;
            }
            let identity = (
                target.vendor_id,
                target.product_id,
                target.serial_number.clone(),
            );
            if let Some(saved) = self.app_settings.application_layout_bindings.get(&key) {
                if saved != &identity {
                    bridge.clear_application_layout_binding();
                    continue;
                }
            } else {
                // One-time migration of UID-addressed legacy profiles, only after
                // the real endpoint proves its UID and the scan is unambiguous.
                self.app_settings
                    .application_layout_bindings
                    .insert(key.clone(), identity);
                changed = true;
            }
            bridge.bind_verified_application_layout_device(key, generation);
        }
        if changed {
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
        let Some(device_key) = self.editing_layer_device_key.clone()
            .or_else(|| self.application_layout_device_key()) else { return false; };
        let Some(layout) = self.app_settings.application_layouts.get_mut(&device_key)
            .and_then(|settings| settings.layouts.get_mut(layout_id)) else { return false; };
        let changed = layout.set_layer_name(layer, name.clone());
        if changed {
            save_app_settings(&self.app_settings);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if layout_id == crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID
            && layer < self.layer_count
            && self.firmware == FirmwareProtocol::Vial
            && self.connected_application_layout_device_key().as_ref() == Some(&device_key)
            && self.editing_layer_generation.is_none_or(|g| g == self.connection_generation)
        {
            if let Some(device) = self.selected_device.and_then(|i| self.device_manager.devices().get(i)) {
                self.pending_layer_name_writes.push_back(super::vial_hid_task::LayerNameWrite {
                    device: device.clone(), device_key, layout_id: layout_id.to_owned(),
                    generation: self.connection_generation, layer, name,
                });
            }
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
        self.editing_layer_device_key = None;
        #[cfg(not(target_arch = "wasm32"))]
        { self.editing_layer_generation = None; }
    }

    pub(super) fn application_control_for_key(
        layout: &KeyboardLayout,
        key_index: usize,
    ) -> Option<usize> {
        let key = layout.keys.get(key_index)?;
        match (key.row, key.col) {
            (0, 2) => Some(12),
            (1..=4, 0..=2) => Some((usize::from(key.row) - 1) * 3 + usize::from(key.col)),
            _ => None,
        }
    }

    pub(super) fn application_control_for_encoder(
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

    pub(super) fn application_layout_rendered_copy_for_profile(
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
    pub(super) fn application_layout_bridge_connected(&self) -> bool {
        self.qmk_hid_hosts.values().any(|bridge| {
            device_supports_application_layouts(bridge.device())
                && self
                    .device_manager
                    .devices()
                    .iter()
                    .any(|device| device.permits_hid_target(bridge.device()))
        })
    }

    // The foreground monitor and application scans only feed application
    // layouts. With no device that can use them and their page closed, they
    // would keep querying the desktop for nothing.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn application_layout_runtime_needed(&self) -> bool {
        self.application_layouts_supported()
            || self.application_layout_bridge_connected()
            || self.application_layouts_page_open()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn update_application_layout_runtime(&mut self) {
        if !self.application_layout_runtime_needed() {
            return;
        }
        self.update_application_layout_runtime_with(
            crate::app_discovery::application_discovery_snapshot(),
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn update_application_layout_runtime_with(
        &mut self,
        discovered: crate::app_discovery::ApplicationDiscoverySnapshot,
    ) {
        let foreground_changed = self.application_layout_foreground != discovered.foreground;
        // Discovery can enrich a foreground app's display name/aliases after a
        // catalog scan without changing focus. Such metadata updates must not
        // finish an in-progress rename. Keep title changes meaningful because
        // application profiles can match individual window titles.
        let foreground_context_changed = !same_foreground_application(
            self.application_layout_foreground.as_ref(),
            discovered.foreground.as_ref(),
        ) || self
            .application_layout_foreground
            .as_ref()
            .map(|app| &app.window_title)
            != discovered.foreground.as_ref().map(|app| &app.window_title);
        if foreground_context_changed {
            // Commit while editor_layout_id still points at the source profile.
            // The runtime may select another profile later in this update.
            self.commit_pending_application_layout_edits();
        }
        self.application_layout_foreground = discovered.foreground.clone();
        self.application_discovery = discovered;

        self.remember_connected_application_layout_device();
        self.restore_background_application_layout_bindings();
        // Capture the generation of each validated grant before computing/persisting
        // snapshots. A reopen during that work must reject this publication.
        let publication_generations: std::collections::BTreeMap<_, _> = self
            .qmk_hid_hosts
            .iter()
            .filter_map(|(path, bridge)| {
                bridge
                    .application_layout_publication_generation()
                    .map(|generation| (path.clone(), generation))
            })
            .collect();
        let selected_key = self.application_layout_device_key();
        // The editor can retain an offline key; only a connected selection may
        // publish cached Default through its live reconciliation/sync path.
        let connected_selected_key = self.connected_application_layout_device_key();
        let mut device_keys = std::collections::BTreeSet::new();
        if let Some(key) = selected_key.as_ref() {
            device_keys.insert(key.clone());
        }
        for bridge in self.qmk_hid_hosts.values().filter(|bridge| {
            bridge.supports_application_layouts()
                && self
                    .device_manager
                    .devices()
                    .iter()
                    .any(|device| device.permits_hid_target(bridge.device()))
        }) {
            if let Some(key) = bridge.application_layout_device_key() {
                device_keys.insert(key.to_owned());
            }
        }
        if self
            .application_layout_manual_override
            .as_ref()
            .is_some_and(|(_, foreground)| {
                !same_foreground_application(
                    foreground.as_ref(),
                    self.application_discovery.foreground.as_ref(),
                )
            })
        {
            self.application_layout_manual_override = None;
        }
        for device_key in device_keys {
            let selected = selected_key.as_ref() == Some(&device_key);
            let seed = selected
                .then(|| self.layout.as_ref().map(Self::base_application_layers))
                .flatten();
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

            let installed_presets = crate::app_discovery::installed_builtin_presets();
            let mut persist = false;
            let (active_layout_followed, active_layout_changed) = {
                let settings = self
                    .app_settings
                    .application_layouts
                    .entry(device_key.clone())
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
                (active_layout_followed, active_layout_changed)
            };
            if selected && active_layout_followed {
                // Metadata can change routing or make the editor follow a profile
                // without changing foreground identity/title. Save to the captured
                // source layout before hiding its field or publishing the snapshot.
                self.commit_pending_application_layout_edits();
            }
            let snapshot = self.app_settings.application_layouts[&device_key]
                .active_layout()
                .map(|layout| {
                    // Ownership and semantic identity are separate: background
                    // Default releases the overlay, but is still Default.
                    if connected_selected_key.as_ref() != Some(&device_key)
                        && layout.id == crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID
                    {
                        crate::application_layouts::ApplicationLayoutSnapshot::firmware_default()
                    } else {
                        crate::application_layouts::ApplicationLayoutSnapshot::from_layout(layout)
                    }
                })
                .unwrap_or_else(crate::application_layouts::ApplicationLayoutSnapshot::inactive);
            if selected && active_layout_changed {
                // Firmware releases held keys and starts every newly activated
                // application layout on layer 0. Keep the independent Layout
                // Indicator state on that same source of truth.
                self.reset_matrix_tester_state();
            }
            if selected && active_layout_followed {
                // Firmware starts an application layout on layer 0. Make the
                // editor show that same layer and clear stale controls.
                self.selected_layer = 0;
                self.selected_key = None;
                self.selected_encoder = None;
            }
            if persist {
                save_app_settings(&self.app_settings);
            }

            if selected {
                self.maybe_sync_default_application_layout_to_device();
            }

            for bridge in self.qmk_hid_hosts.values().filter(|bridge| {
                bridge.supports_application_layouts()
                    && bridge.application_layout_device_key() == Some(device_key.as_str())
                    && self
                        .device_manager
                        .devices()
                        .iter()
                        .any(|device| device.permits_hid_target(bridge.device()))
            }) {
                if let Some(generation) = publication_generations.get(&bridge.device().path) {
                    bridge.publish_application_layout_snapshot_for_generation(
                        snapshot.clone(),
                        *generation,
                    );
                }
            }
        }
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

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn application_layout_runtime_stays_off_without_a_layout_device() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let mut app = EntropyApp::new_inert_for_test();
        let mut k04 = crate::device::test_usb_device("3-2", 9, 0x04);
        k04.name = "K:04".into();
        app.device_manager.replace_devices(vec![k04]);
        app.selected_device = Some(0);
        assert!(!app.application_layout_runtime_needed());

        app.update_application_layout_runtime();
        assert_eq!(
            app.application_discovery.foreground_status,
            crate::app_discovery::ForegroundStatus::default()
        );

        // The page lists running applications even without a Macropad.
        app.main_menu_tab = MainMenuTab::Advanced;
        app.settings_tab = SettingsTab::ApplicationLayouts;
        assert!(app.application_layout_runtime_needed());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn application_layout_runtime_follows_a_connected_macropad() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let mut app = EntropyApp::new_inert_for_test();
        let mut macropad = crate::device::test_usb_device("3-3", 5, 0x42);
        macropad.name = "M4CR0Pad v3".into();
        let mut k04 = crate::device::test_usb_device("3-2", 9, 0x04);
        k04.name = "K:04".into();
        app.device_manager
            .replace_devices(vec![macropad.clone(), k04]);
        app.selected_device = Some(1);
        assert!(!app.application_layout_runtime_needed());

        // A background Macropad bridge needs focus changes before it has
        // published its capabilities.
        let bridge = crate::qmk_hid_host::QmkHidHostBridge::test_inert(
            macropad.clone(),
            Default::default(),
            None,
            crate::qmk_hid_host::HostProtocol::Discover,
        );
        app.qmk_hid_hosts.insert(macropad.path.clone(), bridge);
        assert!(app.application_layout_runtime_needed());

        app.device_manager.replace_devices(Vec::new());
        app.selected_device = None;
        assert!(!app.application_layout_runtime_needed());
    }

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

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn background_macropad_autoswitch_survives_k04_selection() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for (select_k04, serial) in [(false, ""), (true, ""), (true, "indicator-test")] {
            let mut app = EntropyApp::new_inert_for_test();
            let mut macropad = m4cr0pad_v3_device();
            macropad.serial_number = serial.into();
            macropad.path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/main.rs")
                .to_string_lossy()
                .into_owned();
            let mut k04 = macropad.clone();
            k04.name = "K:04".into();
            k04.product_id = 0x0004;
            k04.path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/app.rs")
                .to_string_lossy()
                .into_owned();
            k04.instance_token = "k04-routing-instance".into();
            app.device_manager
                .replace_devices(vec![macropad.clone(), k04.clone()]);
            app.selected_device = Some(0);
            app.current_keyboard_id = Some(0x42);
            app.current_device_name = macropad.name.clone();
            app.layout = Some(indicator_test_layout());
            mark_application_layout_protocol_supported(&mut app);
            let (macro_hid, macro_packets) = crate::hid::HidDevice::test_device();
            macro_packets.respond_by(|request| {
                if request[0] == 0xe6 {
                    let mut response = [0; 32];
                    response[..3].copy_from_slice(&[
                        0xe6,
                        crate::application_layouts::APPLICATION_LAYOUT_PROTOCOL_VERSION,
                        0x5a,
                    ]);
                    Some(Ok(response))
                } else {
                    None
                }
            });
            let (k04_hid, k04_packets) = crate::hid::HidDevice::test_device();
            let bridge = crate::qmk_hid_host::test_start_profile_handoff_bridge(
                macropad.clone(),
                macro_hid.shared_output(),
            );
            bridge.set_selected_application_layout_support(true);
            app.qmk_hid_hosts.insert(macropad.path.clone(), bridge);
            app.qmk_hid_hosts.insert(
                k04.path.clone(),
                crate::qmk_hid_host::test_start_bridge(
                    k04.clone(),
                    crate::qmk_hid_host::HostDataMode {
                        time: true,
                        ..Default::default()
                    },
                    k04_hid.shared_output(),
                    None,
                    crate::qmk_hid_host::HostProtocol::Selected(true),
                    || None,
                ),
            );
            let key = app.application_layout_device_key().unwrap();
            let foreground = crate::application_layouts::DetectedApplication {
                executable: "routing-editor".into(),
                display_name: "Routing editor".into(),
                ..Default::default()
            };
            let settings = app
                .app_settings
                .application_layouts
                .entry(key.clone())
                .or_default();
            let profile = settings.create_for_application(&foreground);
            settings
                .layouts
                .get_mut(&profile)
                .unwrap()
                .set_keycode(0, 0, 0x0005);
            app.shared_hid_output = macro_hid.shared_output();
            app.hid_device = Some(macro_hid);
            app.remember_connected_application_layout_device();
            if select_k04 {
                assert!(app.handoff_selected_qmk_hid_host_bridge(&macropad, &k04));
                assert!(!app.qmk_hid_hosts[&macropad.path].uses_shared_output());
                app.selected_device = Some(1);
                app.current_keyboard_id = Some(0x04);
                app.current_device_name = k04.name.clone();
                app.device_about_info = None;
                app.layout.as_mut().unwrap().name = k04.name.clone();
                app.layout.as_mut().unwrap().live_features.time = true;
                app.layout
                    .as_mut()
                    .unwrap()
                    .live_features
                    .extended_host_protocol = true;
                app.shared_hid_output = k04_hid.shared_output();
                app.sync_qmk_hid_host_bridges_with(&mut |_, _, _, _| {
                    panic!("exact owner should be retained")
                });
                assert!(!app.qmk_hid_hosts[&macropad.path].uses_shared_output());
            }
            app.selected_layer = 3;
            app.selected_key = Some((1, 0));
            let original_layout = app.layout.as_ref().unwrap().layers.clone();
            app.update_application_layout_runtime_with(
                crate::app_discovery::ApplicationDiscoverySnapshot {
                    foreground: Some(foreground.clone()),
                    foreground_status: crate::app_discovery::ForegroundStatus {
                        backend: "synthetic OS boundary".into(),
                        state: crate::app_discovery::ForegroundState::Focused(foreground),
                    },
                    ..Default::default()
                },
            );
            assert_eq!(
                app.app_settings.application_layouts[&key].active_layout_id, profile,
                "connected Macropad must follow foreground with K04 selected={select_k04}"
            );
            let expected = crate::application_layouts::ApplicationLayoutSnapshot::from_layout(
                app.app_settings.application_layouts[&key]
                    .active_layout()
                    .unwrap(),
            );
            assert_eq!(
                app.qmk_hid_hosts[&macropad.path].test_application_layout_snapshot(),
                expected
            );
            assert_eq!(
                app.qmk_hid_hosts[&k04.path].test_application_layout_snapshot(),
                crate::application_layouts::ApplicationLayoutSnapshot::inactive()
            );
            assert_eq!(app.layout.as_ref().unwrap().layers, original_layout);
            let expected_packets = expected.packets();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while !expected_packets.iter().all(|packet| {
                macro_packets.requests().iter().any(|request| {
                    request.starts_with(if packet[0] == 0xe3 {
                        &packet[..9]
                    } else {
                        packet
                    })
                })
            }) {
                assert!(
                    std::time::Instant::now() < deadline,
                    "real Macropad worker did not transmit resolved profile"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(
                !k04_packets.requests().iter().any(|request| {
                    expected_packets
                        .iter()
                        .any(|packet| packet[0] == request[0])
                }),
                "K04 received Macropad host profile writes"
            );
            assert_eq!(app.selected_device, Some(if select_k04 { 1 } else { 0 }));
            if select_k04 {
                assert_eq!(app.selected_layer, 3);
                assert_eq!(app.selected_key, Some((1, 0)));
                assert!(app.qmk_hid_hosts[&macropad.path].has_selected_application_layout_binding());
                let reloaded = crate::app::app_storage::load_app_settings();
                if serial.is_empty() {
                    assert!(!reloaded.application_layout_bindings.contains_key(&key));
                }
                // A different physical enumeration at the same path is a fresh owner.
                let mut replacement = macropad.clone();
                replacement.instance_token = "replacement-instance".into();
                app.device_manager
                    .replace_devices(vec![replacement, k04.clone()]);
                app.sync_qmk_hid_host_bridges_with(&mut |d, m, s, p| {
                    crate::qmk_hid_host::QmkHidHostBridge::test_inert(d, m, s, p)
                });
                let fresh = &app.qmk_hid_hosts[&macropad.path];
                assert_eq!(fresh.application_layout_device_key(), None);
                assert!(!fresh.has_selected_application_layout_binding());
                assert_eq!(fresh.discovered_application_layout_key(), None);
                assert_eq!(
                    fresh.test_application_layout_snapshot(),
                    crate::application_layouts::ApplicationLayoutSnapshot::inactive()
                );
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cold_background_macropad_discovers_binding_with_k04_selected() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let mut app = EntropyApp::new_inert_for_test();
        let mut macropad = m4cr0pad_v3_device();
        macropad.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
        let mut k04 = macropad.clone();
        k04.product_id = 0x0071;
        k04.path = format!("{}/src/app.rs", env!("CARGO_MANIFEST_DIR"));
        k04.serial_number = "k04".into();
        app.device_manager
            .replace_devices(vec![macropad.clone(), k04]);
        app.selected_device = Some(1);
        app.current_keyboard_id = Some(4);
        app.layout = Some(indicator_test_layout());
        app.selected_layer = 3;
        app.selected_key = Some((1, 0));
        let original = app.layout.as_ref().unwrap().layers.clone();
        let (k04_hid, k04_packets) = crate::hid::HidDevice::test_device();
        let k04 = app.device_manager.devices()[1].clone();
        app.qmk_hid_hosts.insert(
            k04.path.clone(),
            crate::qmk_hid_host::test_start_bridge(
                k04,
                Default::default(),
                k04_hid.shared_output(),
                None,
                crate::qmk_hid_host::HostProtocol::Selected(true),
                || None,
            ),
        );
        let key = "vial-0000000000000042";
        let foreground = crate::application_layouts::DetectedApplication {
            executable: "cold-editor".into(),
            display_name: "Cold editor".into(),
            ..Default::default()
        };
        let settings = app
            .app_settings
            .application_layouts
            .entry(key.into())
            .or_default();
        let profile = settings.create_for_application(&foreground);
        settings
            .layouts
            .get_mut(&profile)
            .unwrap()
            .set_keycode(0, 0, 5);
        let (hid, packets) = crate::hid::HidDevice::test_device();
        packets.respond_by(|request| {
            if request[0..2] == [0xfe, 0] {
                let mut response = [0; 32];
                response[0..4].copy_from_slice(&6u32.to_le_bytes());
                response[4..12].copy_from_slice(&0x42u64.to_le_bytes());
                Some(Ok(response))
            } else if request[0] == 0xe6 {
                let mut response = [0; 32];
                response[..3].copy_from_slice(&[
                    0xe6,
                    crate::application_layouts::APPLICATION_LAYOUT_PROTOCOL_VERSION,
                    0x5a,
                ]);
                Some(Ok(response))
            } else {
                None
            }
        });
        let bridge = crate::qmk_hid_host::test_start_bridge(
            macropad.clone(),
            Default::default(),
            None,
            Some(hid),
            crate::qmk_hid_host::HostProtocol::Discover,
            || None,
        );
        app.qmk_hid_hosts.insert(macropad.path.clone(), bridge);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            app.update_application_layout_runtime_with(
                crate::app_discovery::ApplicationDiscoverySnapshot {
                    foreground: Some(foreground.clone()),
                    foreground_status: crate::app_discovery::ForegroundStatus {
                        backend: "synthetic OS".into(),
                        state: crate::app_discovery::ForegroundState::Focused(foreground.clone()),
                    },
                    ..Default::default()
                },
            );
            if app.app_settings.application_layouts[key].active_layout_id == profile {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "cold unbound Macropad never followed foreground"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let expected = crate::application_layouts::ApplicationLayoutSnapshot::from_layout(
            app.app_settings.application_layouts[key]
                .active_layout()
                .unwrap(),
        );
        while !expected.packets().iter().all(|packet| {
            packets.requests().iter().any(|r| {
                r.starts_with(if packet[0] == 0xe3 {
                    &packet[..9]
                } else {
                    packet
                })
            })
        }) {
            assert!(
                std::time::Instant::now() < deadline,
                "profile not transmitted"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(app.selected_device, Some(1));
        assert_eq!(app.selected_layer, 3);
        assert_eq!(app.selected_key, Some((1, 0)));
        assert_eq!(app.layout.as_ref().unwrap().layers, original);
        assert!(
            k04_packets.requests().is_empty(),
            "cold runtime wrote to K04"
        );
        // Cold restart persists physical identity, not an OS enumeration token.
        app.app_settings = crate::app::app_storage::load_app_settings();
        assert_eq!(
            app.app_settings.application_layout_bindings[key],
            (
                macropad.vendor_id,
                macropad.product_id,
                macropad.serial_number.clone()
            )
        );
        for (serial, uid, product_id, duplicate, should_bind) in [
            ("indicator-test", 0x42u64, 0x42, false, true),
            ("another-unit", 0x42, 0x42, false, false),
            ("indicator-test", 0x99, 0x42, false, false),
            ("indicator-test", 0x42, 0x41, false, false),
            ("indicator-test", 0x42, 0x42, true, false),
            ("", 0x42, 0x42, false, false),
        ] {
            app.qmk_hid_hosts.clear();
            let mut fresh = macropad.clone();
            fresh.instance_token = format!("fresh-{serial}-{uid}-{duplicate}");
            fresh.serial_number = serial.into();
            fresh.product_id = product_id;
            let k04 = app.device_manager.devices()[1].clone();
            let mut devices = vec![fresh.clone(), k04];
            if duplicate {
                let mut twin = fresh.clone();
                twin.path = "second-macropad".into();
                twin.serial_number = "twin".into();
                devices.push(twin);
            }
            app.device_manager.replace_devices(devices);
            let (hid, packets) = crate::hid::HidDevice::test_device();
            packets.respond_by(move |request| {
                if request[0..2] == [0xfe, 0] {
                    let mut response = [0; 32];
                    response[..4].copy_from_slice(&6u32.to_le_bytes());
                    response[4..12].copy_from_slice(&uid.to_le_bytes());
                    Some(Ok(response))
                } else if request[0] == 0xe6 {
                    let mut response = [0; 32];
                    response[..3].copy_from_slice(&[
                        0xe6,
                        crate::application_layouts::APPLICATION_LAYOUT_PROTOCOL_VERSION,
                        0x5a,
                    ]);
                    Some(Ok(response))
                } else {
                    None
                }
            });
            app.qmk_hid_hosts.insert(
                fresh.path.clone(),
                crate::qmk_hid_host::test_start_bridge(
                    fresh.clone(),
                    Default::default(),
                    None,
                    Some(hid),
                    crate::qmk_hid_host::HostProtocol::Discover,
                    || None,
                ),
            );
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while !app.qmk_hid_hosts[&fresh.path].supports_application_layouts() {
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            app.update_application_layout_runtime_with(
                crate::app_discovery::ApplicationDiscoverySnapshot {
                    foreground: Some(foreground.clone()),
                    foreground_status: crate::app_discovery::ForegroundStatus {
                        backend: "synthetic OS".into(),
                        state: crate::app_discovery::ForegroundState::Focused(foreground.clone()),
                    },
                    ..Default::default()
                },
            );
            assert_eq!(
                app.qmk_hid_hosts[&fresh.path].application_layout_device_key(),
                should_bind.then_some(key),
                "serial={serial} uid={uid} duplicate={duplicate}"
            );
            if should_bind {
                while !expected.packets().iter().all(|packet| {
                    packets.requests().iter().any(|r| {
                        r.starts_with(if packet[0] == 0xe3 {
                            &packet[..9]
                        } else {
                            packet
                        })
                    })
                }) {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "reenumerated owner did not restore profile"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                let mut twin = fresh.clone();
                twin.path = "late-twin".into();
                twin.serial_number = "late-twin".into();
                let mut devices = app.device_manager.devices().to_vec();
                devices.push(twin);
                app.device_manager.replace_devices(devices);
                app.restore_background_application_layout_bindings();
                assert_eq!(
                    app.qmk_hid_hosts[&fresh.path].application_layout_device_key(),
                    None,
                    "late duplicate must revoke an automatic binding"
                );
            } else {
                assert_eq!(
                    app.qmk_hid_hosts[&fresh.path].test_application_layout_snapshot(),
                    crate::application_layouts::ApplicationLayoutSnapshot::inactive()
                );
            }
            assert_eq!(app.selected_device, Some(1));
            assert_eq!(app.layout.as_ref().unwrap().layers, original);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cold_background_default_preserves_external_firmware_edit_and_unchanged_control() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for firmware_code in [0x0005u16, 0x0004] {
            let mut app = EntropyApp::new_inert_for_test();
            let path = tempfile::NamedTempFile::new().unwrap();
            let mut macropad = m4cr0pad_v3_device();
            macropad.path = path.path().to_string_lossy().into_owned();
            let mut k04 = macropad.clone();
            k04.product_id = 0x0004;
            k04.name = "K:04".into();
            k04.path = format!("{}/src/app.rs", env!("CARGO_MANIFEST_DIR"));
            k04.serial_number = "k04-control".into();
            k04.instance_token = "k04-control".into();
            app.device_manager
                .replace_devices(vec![macropad.clone(), k04.clone()]);
            app.selected_device = Some(1);
            app.current_keyboard_id = Some(4);
            app.layout = Some(indicator_test_layout());
            app.layout.as_mut().unwrap().layers[0][0] = crate::keyboard::KeyBinding::Vial(0x0006); // never Macropad Default
            let original = app.layout.as_ref().unwrap().layers.clone();
            app.selected_layer = 3;
            app.selected_key = Some((1, 0));
            let key = "vial-0000000000000042";
            let settings = app
                .app_settings
                .application_layouts
                .entry(key.into())
                .or_default();
            settings
                .layouts
                .get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
                .unwrap()
                .set_keycode(0, 0, 0x0004);
            app.app_settings.application_layout_bindings.insert(
                key.into(),
                (
                    macropad.vendor_id,
                    macropad.product_id,
                    macropad.serial_number.clone(),
                ),
            );
            save_app_settings(&app.app_settings);
            app.app_settings = crate::app::app_storage::load_app_settings(); // cold restore
            let (hid, packets) = crate::hid::HidDevice::test_device();
            // Synthetic firmware state only: production runtime, UID/capability
            // discovery and transfer remain real. An external Vial edit changes
            // the firmware-owned map, not Entropy's saved Default or K04 editor.
            let firmware = std::sync::Arc::new(std::sync::atomic::AtomicU16::new(4));
            firmware.store(firmware_code, std::sync::atomic::Ordering::Release);
            let overlay_active = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let overlay_code = std::sync::Arc::new(std::sync::atomic::AtomicU16::new(u16::MAX));
            let active = overlay_active.clone();
            let code = overlay_code.clone();
            let protocol =
                crate::application_layouts::ApplicationLayoutSnapshot::inactive().packets();
            let commit_type = protocol.last().unwrap()[0];
            let keycodes_type = protocol[1][0];
            packets.on_output(move |packet| {
                if packet[0] == keycodes_type && packet[2..4] == [0, 0] {
                    code.store(
                        u16::from_le_bytes([packet[5], packet[6]]),
                        std::sync::atomic::Ordering::Release,
                    );
                } else if packet[0] == commit_type {
                    active.store(packet[2] != 0, std::sync::atomic::Ordering::Release);
                }
            });
            packets.respond_by(move |request| {
                if request[..2] == [0xfe, 0] {
                    let mut response = [0; 32];
                    response[..4].copy_from_slice(&6u32.to_le_bytes());
                    response[4..12].copy_from_slice(&0x42u64.to_le_bytes());
                    Some(Ok(response))
                } else if request[0] == 0xe6 {
                    let mut response = [0; 32];
                    response[..3].copy_from_slice(&[
                        0xe6,
                        crate::application_layouts::APPLICATION_LAYOUT_PROTOCOL_VERSION,
                        0x5a,
                    ]);
                    Some(Ok(response))
                } else {
                    None
                }
            });
            let (k04_hid, k04_packets) = crate::hid::HidDevice::test_device();
            app.qmk_hid_hosts.insert(
                k04.path.clone(),
                crate::qmk_hid_host::test_start_bridge(
                    k04,
                    Default::default(),
                    k04_hid.shared_output(),
                    None,
                    crate::qmk_hid_host::HostProtocol::Selected(false),
                    || None,
                ),
            );
            app.qmk_hid_hosts.insert(
                macropad.path.clone(),
                crate::qmk_hid_host::test_start_bridge(
                    macropad.clone(),
                    Default::default(),
                    None,
                    Some(hid),
                    crate::qmk_hid_host::HostProtocol::Discover,
                    || None,
                ),
            );
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while app.qmk_hid_hosts[&macropad.path]
                .application_layout_publication_generation()
                .is_none()
            {
                app.update_application_layout_runtime_with(Default::default());
                assert!(
                    std::time::Instant::now() < deadline,
                    "cold UID binding never restored"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            // External Vial edited persistent firmware to firmware_code, while
            // Entropy's saved Default is 4. The synthetic firmware boundary uses
            // its own keymap iff the production worker commits an inactive overlay.
            let expected =
                crate::application_layouts::ApplicationLayoutSnapshot::firmware_default();
            assert_eq!(
                app.qmk_hid_hosts[&macropad.path].test_application_layout_snapshot(),
                expected,
                "background Default replayed saved code 4 over firmware code {firmware_code}"
            );
            let sent_before_resend = packets.requests().len();
            let worker = app.qmk_hid_hosts.get_mut(&macropad.path).unwrap();
            worker.force_application_layout_resend();
            while !packets.requests()[sent_before_resend..]
                .iter()
                .any(|p| p[..9] == expected.packets().last().unwrap()[..9])
            {
                assert!(
                    std::time::Instant::now() < deadline,
                    "inactive Default not committed"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let effective_code = if overlay_active.load(std::sync::atomic::Ordering::Acquire) {
                overlay_code.load(std::sync::atomic::Ordering::Acquire)
            } else {
                firmware.load(std::sync::atomic::Ordering::Acquire)
            };
            assert_eq!(
                effective_code, firmware_code,
                "Default masked the live firmware keymap"
            );
            assert!(
                !packets
                    .requests()
                    .iter()
                    .any(|p| p[0] == expected.packets()[0][0] && p[2] != 0),
                "cold worker activated a stale Default overlay"
            );
            // A normal application overlay must still work, and returning from
            // it to Default must again expose the same live firmware map.
            let foreground = crate::application_layouts::DetectedApplication {
                executable: "default-return-control".into(),
                display_name: "Default return control".into(),
                ..Default::default()
            };
            let settings = app.app_settings.application_layouts.get_mut(key).unwrap();
            let profile = settings.create_for_application(&foreground);
            settings
                .layouts
                .get_mut(&profile)
                .unwrap()
                .set_keycode(0, 0, 7);
            let focused = crate::app_discovery::ApplicationDiscoverySnapshot {
                foreground: Some(foreground.clone()),
                foreground_status: crate::app_discovery::ForegroundStatus {
                    backend: "synthetic OS".into(),
                    state: crate::app_discovery::ForegroundState::Focused(foreground),
                },
                ..Default::default()
            };
            app.update_application_layout_runtime_with(focused);
            let application = app.qmk_hid_hosts[&macropad.path].test_application_layout_snapshot();
            assert!(application.active);
            let application_commit = *application.packets().last().unwrap();
            while !packets
                .requests()
                .iter()
                .any(|p| p[..9] == application_commit[..9])
            {
                assert!(
                    std::time::Instant::now() < deadline,
                    "application control not committed"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(overlay_active.load(std::sync::atomic::Ordering::Acquire));
            assert_eq!(overlay_code.load(std::sync::atomic::Ordering::Acquire), 7);
            let return_start = packets.requests().len();
            app.update_application_layout_runtime_with(
                crate::app_discovery::ApplicationDiscoverySnapshot {
                    foreground_status: crate::app_discovery::ForegroundStatus {
                        backend: "synthetic OS".into(),
                        state: crate::app_discovery::ForegroundState::NoFocusedWindow,
                    },
                    ..Default::default()
                },
            );
            assert_eq!(
                app.qmk_hid_hosts[&macropad.path].test_application_layout_snapshot(),
                expected
            );
            while !packets.requests()[return_start..]
                .iter()
                .any(|p| p[..9] == expected.packets().last().unwrap()[..9])
            {
                assert!(
                    std::time::Instant::now() < deadline,
                    "return to Default not committed"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(!overlay_active.load(std::sync::atomic::Ordering::Acquire));
            assert_eq!(
                firmware.load(std::sync::atomic::Ordering::Acquire),
                firmware_code
            );
            assert!(k04_packets.requests().is_empty());
            assert_eq!(app.layout.as_ref().unwrap().layers, original);
            assert_eq!(app.selected_device, Some(1));
            assert_eq!(app.selected_layer, 3);
            assert_eq!(app.selected_key, Some((1, 0)));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn offline_editor_background_default_preserves_external_firmware_edit() {
        offline_editor_background_default_control(0x0005);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn offline_editor_background_default_preserves_unchanged_firmware() {
        offline_editor_background_default_control(0x0004);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn offline_editor_background_default_control(firmware_code: u16) {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let mut app = EntropyApp::new_inert_for_test();
        let path = tempfile::NamedTempFile::new().unwrap();
        let mut macropad = m4cr0pad_v3_device();
        macropad.path = path.path().to_string_lossy().into_owned();
        app.device_manager.replace_devices(vec![macropad.clone()]);
        let key = "vial-0000000000000042";
        app.app_settings.last_application_layout_device_key = Some(key.into());
        app.app_settings
            .application_layouts
            .entry(key.into())
            .or_default()
            .layouts
            .get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
            .unwrap()
            .set_keycode(0, 0, 0x0004);
        app.app_settings.application_layout_bindings.insert(
            key.into(),
            (
                macropad.vendor_id,
                macropad.product_id,
                macropad.serial_number.clone(),
            ),
        );
        save_app_settings(&app.app_settings);
        app.app_settings = crate::app::app_storage::load_app_settings();
        assert!(app.selected_device.is_none());
        assert!(app.layout.is_none());
        assert!(app.hid_device.is_none());
        assert!(app.shared_hid_output.is_none());
        assert!(app.application_layouts_supported());
        assert_eq!(app.application_layout_device_key().as_deref(), Some(key));

        let (hid, packets) = crate::hid::HidDevice::test_device();
        // Only the OS and firmware boundary are synthetic. Decode real worker
        // packets to determine whether Default exposes firmware or cached keys.
        let overlay_active = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let overlay_code = std::sync::Arc::new(std::sync::atomic::AtomicU16::new(u16::MAX));
        let active = overlay_active.clone();
        let code = overlay_code.clone();
        let protocol = crate::application_layouts::ApplicationLayoutSnapshot::inactive().packets();
        let commit_type = protocol.last().unwrap()[0];
        let keycodes_type = protocol[1][0];
        packets.on_output(move |packet| {
            if packet[0] == keycodes_type && packet[2..4] == [0, 0] {
                code.store(
                    u16::from_le_bytes([packet[5], packet[6]]),
                    std::sync::atomic::Ordering::Release,
                );
            } else if packet[0] == commit_type {
                active.store(packet[2] != 0, std::sync::atomic::Ordering::Release);
            }
        });
        packets.respond_by(|request| {
            if request[..2] == [0xfe, 0] {
                let mut response = [0; 32];
                response[..4].copy_from_slice(&6u32.to_le_bytes());
                response[4..12].copy_from_slice(&0x42u64.to_le_bytes());
                Some(Ok(response))
            } else if request[0] == 0xe6 {
                let mut response = [0; 32];
                response[..3].copy_from_slice(&[
                    0xe6,
                    crate::application_layouts::APPLICATION_LAYOUT_PROTOCOL_VERSION,
                    0x5a,
                ]);
                Some(Ok(response))
            } else {
                None
            }
        });
        app.qmk_hid_hosts.insert(
            macropad.path.clone(),
            crate::qmk_hid_host::test_start_bridge(
                macropad.clone(),
                Default::default(),
                None,
                Some(hid),
                crate::qmk_hid_host::HostProtocol::Discover,
                || None,
            ),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.qmk_hid_hosts[&macropad.path]
            .application_layout_publication_generation()
            .is_none()
        {
            app.update_application_layout_runtime_with(Default::default());
            assert!(
                std::time::Instant::now() < deadline,
                "cold UID binding never restored"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            app.qmk_hid_hosts[&macropad.path].application_layout_device_key(),
            Some(key)
        );
        let published = app.qmk_hid_hosts[&macropad.path].test_application_layout_snapshot();
        let start = packets.requests().len();
        app.qmk_hid_hosts[&macropad.path].force_application_layout_resend();
        while !packets.requests()[start..]
            .iter()
            .any(|p| p[..9] == published.packets().last().unwrap()[..9])
        {
            assert!(
                std::time::Instant::now() < deadline,
                "Default not committed by real worker"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let effective_code = if overlay_active.load(std::sync::atomic::Ordering::Acquire) {
            overlay_code.load(std::sync::atomic::Ordering::Acquire)
        } else {
            firmware_code
        };
        assert_eq!(
            effective_code, firmware_code,
            "offline cached Default masked the live firmware keymap"
        );
        assert_eq!(
            published,
            crate::application_layouts::ApplicationLayoutSnapshot::firmware_default()
        );
        assert!(
            !packets
                .requests()
                .iter()
                .any(|p| p[0] == commit_type && p[2] != 0),
            "offline editor authorized an active cached Default"
        );

        // Offline editing still addresses the saved profile, and configured
        // foreground overlays must remain active on the validated background owner.
        let foreground = crate::application_layouts::DetectedApplication {
            executable: "offline-editor-overlay-control".into(),
            display_name: "Offline editor overlay control".into(),
            ..Default::default()
        };
        let settings = app.application_layout_settings_mut().unwrap();
        let profile = settings.create_for_application(&foreground);
        settings
            .layouts
            .get_mut(&profile)
            .unwrap()
            .set_keycode(0, 0, 7);
        app.update_application_layout_runtime_with(
            crate::app_discovery::ApplicationDiscoverySnapshot {
                foreground: Some(foreground.clone()),
                foreground_status: crate::app_discovery::ForegroundStatus {
                    backend: "synthetic OS".into(),
                    state: crate::app_discovery::ForegroundState::Focused(foreground),
                },
                ..Default::default()
            },
        );
        let application = app.qmk_hid_hosts[&macropad.path].test_application_layout_snapshot();
        assert!(application.active);
        while !packets
            .requests()
            .iter()
            .any(|p| p[..9] == application.packets().last().unwrap()[..9])
        {
            assert!(
                std::time::Instant::now() < deadline,
                "offline application overlay not committed"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(overlay_active.load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(overlay_code.load(std::sync::atomic::Ordering::Acquire), 7);
        let start = packets.requests().len();
        app.update_application_layout_runtime_with(
            crate::app_discovery::ApplicationDiscoverySnapshot {
                foreground_status: crate::app_discovery::ForegroundStatus {
                    backend: "synthetic OS".into(),
                    state: crate::app_discovery::ForegroundState::NoFocusedWindow,
                },
                ..Default::default()
            },
        );
        let inactive = crate::application_layouts::ApplicationLayoutSnapshot::firmware_default();
        assert_eq!(
            app.qmk_hid_hosts[&macropad.path].test_application_layout_snapshot(),
            inactive
        );
        while !packets.requests()[start..]
            .iter()
            .any(|p| p[..9] == inactive.packets().last().unwrap()[..9])
        {
            assert!(
                std::time::Instant::now() < deadline,
                "offline return to Default not committed"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!overlay_active.load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(app.application_layout_device_key().as_deref(), Some(key));
        assert_eq!(
            app.application_layout_settings().unwrap().layouts[&profile].layers[0][0],
            7
        );
        assert_eq!(
            app.application_layout_settings().unwrap().layouts
                [crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID]
                .layers[0][0],
            4
        );
        assert!(app.selected_device.is_none());
        assert!(app.layout.is_none());
        assert!(app.hid_device.is_none());
        assert!(app.shared_hid_output.is_none());
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

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ui_default_layer_rename_persists_to_firmware() {
        use crate::application_layouts::{DetectedApplication, DEFAULT_APPLICATION_LAYOUT_ID};
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for (mode, scenario) in [(1,"ok"), (2,"ok"), (0,"ok"), (0,"borrowed"),
            (0,"switch_profile"), (0,"failed_layer"), (0,"set_error"),
            (0,"get_error"), (0,"mismatch"), (0,"stale_device"),
            (0,"draft_switch_device"), (0,"unicode")] {
            let ctx = egui::Context::default();
            let mut app = EntropyApp::new_inert_for_test();
            let mut device = m4cr0pad_v3_device();
            device.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
            app.device_manager.replace_devices(vec![device]);
            app.selected_device = Some(0);
            app.current_keyboard_id = Some(0x42);
            app.current_device_name = "Macropad".into();
            app.firmware = FirmwareProtocol::Vial;
            mark_application_layout_protocol_supported(&mut app);
            app.layout = Some(indicator_test_layout());
            app.layer_count = 16;
            app.layer_names = crate::application_layouts::default_layer_names();
            app.deferred_device_load = DeferredDeviceLoadState::complete(16);
            let key = app.application_layout_device_key().unwrap();
            let seed = EntropyApp::base_application_layers(app.layout.as_ref().unwrap());
            let settings = app.app_settings.application_layouts.entry(key.clone()).or_default();
            let other = settings.create_for_application(&DetectedApplication {
                executable: "rename-control.exe".into(), ..Default::default()
            });
            let profile = if mode == 1 { other.clone() } else { DEFAULT_APPLICATION_LAYOUT_ID.into() };
            settings.active_layout_id = profile.clone();
            settings.editor_layout_id = profile.clone();
            for layout in settings.layouts.values_mut() { layout.seed_unset_keycodes(seed); }
            let untouched = settings.layouts[if mode == 1 { DEFAULT_APPLICATION_LAYOUT_ID } else { &other }].clone();
            app.application_layout_editor_active = mode != 2;
            let (hid, packets) = crate::hid::HidDevice::test_device();
            let store = std::sync::Arc::new(std::sync::Mutex::new("Factory".to_owned()));
            let firmware = store.clone();
            let (release, gate) = std::sync::mpsc::channel();
            let gate = std::sync::Mutex::new(gate);
            packets.respond_by(move |request| {
                if request[..2] == [0xfe, 0x08] {
                    gate.lock().unwrap().recv_timeout(std::time::Duration::from_secs(3)).unwrap();
                    Some(Ok([0;32]))
                } else if request[..2] == [0xfe, 0x0b] && request[2..4] == 200u16.to_le_bytes() {
                    if scenario == "set_error" { return Some(Ok([0xff;32])); }
                    let end = request[4..].iter().position(|b| *b == 0).unwrap_or(28);
                    *firmware.lock().unwrap() = String::from_utf8(request[4..4+end].to_vec()).unwrap().replace("%%","%");
                    Some(Ok([0;32]))
                } else if request[..2] == [0xfe, 0x0a] && request[2..4] == 200u16.to_le_bytes() {
                    if scenario == "get_error" { return Some(Ok([0xff;32])); }
                    if scenario == "mismatch" { return Some(Ok([0;32])); }
                    let name = firmware.lock().unwrap();
                    let mut response = [0;32];
                    response[1..1+name.len()].copy_from_slice(name.as_bytes());
                    Some(Ok(response))
                } else { None }
            });
            app.hid_device = Some(hid);
            let frame = |app: &mut EntropyApp, events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1100.0, 800.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| app.draw_layout_layer_switcher_and_hints(ui, 0.0, 0.0, 52.0),
                )
                .drop_without_applying_deltas();
            };
            let pos = egui::pos2(550.0,50.0);
            frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true,false] {
                frame(&mut app, vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton {
                    pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE
                }]);
            }
            frame(&mut app, vec![]);
            assert_eq!(app.editing_layer,Some(0));
            frame(&mut app,vec![egui::Event::Text("New".into())]);
            if scenario == "unicode" {
                frame(&mut app,vec![egui::Event::Key { key:egui::Key::A,physical_key:None,pressed:true,repeat:false,
                    modifiers:egui::Modifiers { command:true,ctrl:true,..Default::default() } }]);
                frame(&mut app,vec![egui::Event::Text("Слой%Макро".into())]);
            }
            let proposed = app.editing_layer_text.clone();
            let expected = if scenario == "unicode" { "Слой%Мак".to_owned() } else { proposed.clone() };
            let (unrelated_hid, unrelated_packets) = crate::hid::HidDevice::test_device();
            let unrelated_key = "vial-0000000000000043";
            let unrelated_settings = crate::application_layouts::DeviceApplicationLayouts::default();
            if scenario == "draft_switch_device" {
                app.app_settings.application_layouts.insert(unrelated_key.into(),unrelated_settings.clone());
                app.current_keyboard_id = Some(0x43);
                app.connection_generation += 1;
                app.hid_device = Some(unrelated_hid);
            }
            if scenario == "borrowed" {
                assert_eq!(app.start_vial_hid_operation(&ctx,super::super::vial_hid_task::VialHidOperation::Lock),
                    super::super::vial_hid_task::VialHidTaskStart::Started);
            }
            frame(&mut app,vec![egui::Event::Key { key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:egui::Modifiers::NONE }]);
            assert!(app.editing_layer.is_none());
            if mode == 0 {
                assert!(!packets.requests().iter().any(|p| p[..2]==[0xfe,0x0b]),"UI must not synchronously write HID");
            }
            if scenario == "failed_layer" {
                app.deferred_device_load.set_layer_status(0, DeferredLoadStatus::Failed("synthetic cold-read failure".into()));
            }
            if scenario == "switch_profile" {
                app.activate_application_layout(&other);
            }
            if scenario == "stale_device" {
                app.connection_generation += 1;
                app.device_manager.replace_devices(vec![crate::device::Device {
                    path: "unrelated-owner".into(), ..m4cr0pad_v3_device()
                }]);
            }
            if mode == 0 && scenario != "draft_switch_device" { assert!(app.defer_exit_until_hid_write_returns(&ctx)); }
            // Drain production workers only; no direct setting writer in the probe.
            app.poll_vial_hid_task(&ctx);
            if scenario == "borrowed" {
                assert_eq!(app.pending_layer_name_writes.len(),1,"borrowed owner must retain rename");
                assert!(!packets.requests().iter().any(|p| p[..2]==[0xfe,0x0b]));
                app.application_layout_manual_override = Some((key.clone(), None));
                app.last_device_scan_at = 1.0;
                app.last_full_device_scan_at = 1.0;
                app.tick_layout_import_lifecycle_for_test(&ctx,1.0);
                assert_eq!(app.pending_layer_name_writes.len(),1);
                release.send(()).unwrap();
            }
            let deadline = std::time::Instant::now()+std::time::Duration::from_secs(2);
            while app.vial_hid_task_active() {
                app.poll_vial_hid_task(&ctx);
                assert!(std::time::Instant::now()<deadline);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert!(unrelated_packets.requests().is_empty(),"rename wrote an unrelated HID owner");
            if mode == 0 {
                assert!(app.pending_layer_name_writes.is_empty());
                app.finish_deferred_exit_after_hid_write(&ctx);
                assert!(!app.exit_after_hid_write,"rename must drain before exit");
            }
            if mode != 2 {
                let saved = crate::app::app_storage::load_app_settings();
                assert_eq!(saved.application_layouts[&key].layouts[&profile].layer_names[0],expected);
                assert_eq!(saved.application_layouts[&key].layouts[if mode==1 { DEFAULT_APPLICATION_LAYOUT_ID } else { &other }],untouched);
                if scenario == "draft_switch_device" { assert_eq!(saved.application_layouts[unrelated_key],unrelated_settings); }
            }
            if mode == 1 {
                assert!(packets.requests().is_empty(),"non-Default must remain host-only");
                assert_eq!(*store.lock().unwrap(),"Factory");
            } else if matches!(scenario,"failed_layer"|"set_error"|"stale_device"|"draft_switch_device") {
                assert_eq!(*store.lock().unwrap(),"Factory");
                if scenario != "set_error" {
                    assert!(!packets.requests().iter().any(|p| p[..2]==[0xfe,0x0b]),"unsafe rename was admitted");
                }
            } else {
                assert_eq!(*store.lock().unwrap(),expected,"UI rename left persistent firmware name unchanged (mode={mode})");
                if mode == 0 {
                    assert!(packets.requests().iter().any(|p| p[..2]==[0xfe,0x0a]),"Default requires confirmed firmware readback");
                }
            }
            if matches!(scenario,"get_error"|"mismatch"|"set_error"|"failed_layer") {
                assert!(app.status_msg.contains("Layer name not saved"),"{}",app.status_msg);
                assert_ne!(app.layer_names[0],proposed,"unconfirmed name was published to firmware cache");
                let before = packets.requests().len();
                for _ in 0..3 { app.poll_vial_hid_task(&ctx); frame(&mut app,vec![]); }
                assert_eq!(packets.requests().len(),before,"failure retried on repaint");
            }
            drop(app); // synthetic nonvolatile store outlives all host state
            if mode != 1 && !matches!(scenario,"failed_layer"|"set_error"|"stale_device"|"draft_switch_device") {
                assert_eq!(*store.lock().unwrap(),expected);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ui_name_utf8_whitespace_boundary_confirms_production_worker() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for (proposal, stored, confirmed) in [
            ("абвгдеж x", "абвгдеж ", "абвгдеж"),
            ("Слой%Макро", "Слой%Мак", "Слой%Мак"),
            ("100%", "100%", "100%"),
            ("abcdefghijkl", "abcdefghijkl", "abcdefghijkl"),
        ] {
            let ctx = egui::Context::default();
            let mut app = name_review_app();
            let (hid, packets) = crate::hid::HidDevice::test_device();
            let store = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
            let firmware = store.clone();
            packets.respond_by(move |r| {
                if r[..4] == [0xfe, 0x0b, 200, 0] {
                    let end = r[4..].iter().position(|b| *b == 0).unwrap_or(28);
                    *firmware.lock().unwrap() = String::from_utf8(r[4..4+end].to_vec()).unwrap().replace("%%", "%");
                    Some(Ok([0; 32]))
                } else if r[..4] == [0xfe, 0x0a, 200, 0] {
                    let name = firmware.lock().unwrap();
                    let mut response = [0; 32];
                    response[1..1+name.len()].copy_from_slice(name.as_bytes());
                    Some(Ok(response))
                } else { None }
            });
            app.hid_device = Some(hid);
            name_review_rename(&mut app, &ctx, proposal);
            name_review_drain(&mut app, &ctx);
            assert_eq!(*store.lock().unwrap(), stored, "setter wire budget changed");
            assert_eq!(app.layer_names[0], confirmed, "successful UI SET/GET was not confirmed: {}", app.status_msg);
            assert_eq!(app.layout.as_ref().unwrap().layer_names[0], confirmed);
            assert!(!app.selected_layer_names_uncertain(), "false uncertainty for {proposal}");
            assert!(!app.status_msg.contains("Layer name not saved"), "{}", app.status_msg);
            assert_eq!(crate::app::app_storage::load_layer_names("Macropad")[0], confirmed);
            let before = packets.requests().len();
            name_review_frame(&mut app, &ctx, vec![]);
            app.poll_vial_hid_task(&ctx);
            assert_eq!(packets.requests().len(), before, "repaint issued a new write");
            assert_eq!(packets.requests().iter().filter(|r| r[..2] == [0xfe, 0x0b]).count(), 1);
            assert_eq!(packets.requests().iter().filter(|r| r[..2] == [0xfe, 0x0a]).count(), 1);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn name_review_app() -> EntropyApp {
        let mut app = EntropyApp::new_inert_for_test();
        let mut device = m4cr0pad_v3_device();
        device.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
        app.device_manager.replace_devices(vec![device]);
        app.selected_device = Some(0);
        app.current_keyboard_id = Some(0x42);
        app.current_device_name = "Macropad".into();
        app.firmware = FirmwareProtocol::Vial;
        mark_application_layout_protocol_supported(&mut app);
        app.layout = Some(indicator_test_layout());
        app.layer_count = 16;
        app.layer_names = crate::application_layouts::default_layer_names();
        app.deferred_device_load = DeferredDeviceLoadState::complete(16);
        let key = app.application_layout_device_key().unwrap();
        app.app_settings.application_layouts.entry(key).or_default();
        app.application_layout_editor_active = true;
        app
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn name_review_frame(app: &mut EntropyApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.draw_layout_layer_switcher_and_hints(ui, 0.0, 0.0, 52.0),
        )
        .drop_without_applying_deltas();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn name_review_rename(app: &mut EntropyApp, ctx: &egui::Context, name: &str) {
        let pos = egui::pos2(550.0,50.0);
        name_review_frame(app, ctx, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true,false] {
            name_review_frame(app, ctx, vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton {
                pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE
            }]);
        }
        name_review_frame(app, ctx, vec![]);
        assert_eq!(app.editing_layer, Some(app.selected_layer));
        name_review_frame(app, ctx, vec![egui::Event::Key { key:egui::Key::A,physical_key:None,pressed:true,repeat:false,
            modifiers:egui::Modifiers { command:true,ctrl:true,..Default::default() } }]);
        name_review_frame(app, ctx, vec![egui::Event::Text(name.into())]);
        name_review_frame(app, ctx, vec![egui::Event::Key { key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:egui::Modifiers::NONE }]);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ui_cold_default_name_waits_for_deferred_load_and_exit() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let ctx = egui::Context::default();
        let mut app = name_review_app();
        app.selected_layer = 1;
        app.deferred_device_load = DeferredDeviceLoadState::staged(DeferredDeviceLoadContext {
            json: std::sync::Arc::new(serde_json::json!({})),
            supported_qmk_settings: std::sync::Arc::new(vec![201]),
            definition_fingerprint: 7, layer_count:16, rows:1, cols:1, encoder_count:0,
            macro_count:0, macro_memory_bytes:Some(0), tap_dance_count:0, combo_count:0,
            key_override_count:0, alt_repeat_count:0, modules_supported:false, touchpad_supported:false,
            bluetooth_supported:false, layer_leds_supported:false, rgb_supported:false, lighting_mode:None,
            supports_rmk_native_key_actions:false, supports_universal_symbols:false,
            supports_universal_russian_letters:false, supports_rmk_native_combo_output:false,
            supports_rmk_native_tap_dance_actions:false, supports_rmk_combo_layers:false,
        });
        let (hid, packets) = crate::hid::HidDevice::test_device();
        let (release, gate) = std::sync::mpsc::channel();
        let gate = std::sync::Mutex::new(gate);
        let store = std::sync::Arc::new(std::sync::Mutex::new("Factory".to_owned()));
        let firmware = store.clone();
        packets.respond_by(move |request| {
            if request[0] == 0x12 {
                gate.lock().unwrap().recv_timeout(std::time::Duration::from_secs(3)).unwrap();
                Some(Ok([0;32]))
            } else if request[..2] == [0xfe,0x0b] {
                let end = request[4..].iter().position(|b| *b==0).unwrap_or(28);
                *firmware.lock().unwrap() = String::from_utf8(request[4..4+end].to_vec()).unwrap();
                Some(Ok([0;32]))
            } else if request[..2] == [0xfe,0x0a] {
                let name = firmware.lock().unwrap(); let mut response=[0;32];
                response[1..1+name.len()].copy_from_slice(name.as_bytes()); Some(Ok(response))
            } else { None }
        });
        app.hid_device = Some(hid);
        name_review_rename(&mut app, &ctx, "Cold");
        assert!(app.defer_exit_until_hid_write_returns(&ctx));
        app.poll_vial_hid_task(&ctx);
        assert_eq!(app.pending_layer_name_writes.len(),1,"cold rename was discarded");
        assert!(app.vial_hid_task_active(),"pending name must schedule its required cold read");
        app.poll_vial_hid_task(&ctx);
        assert_eq!(app.pending_layer_name_writes.len(),1,"Loading must retain rename");
        app.finish_deferred_exit_after_hid_write(&ctx);
        assert!(app.exit_after_hid_write);
        release.send(()).unwrap();
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(2);
        while app.hid_write_lifecycle_busy() {
            app.tick_layout_import_lifecycle_for_test(&ctx,1.0);
            assert!(std::time::Instant::now()<deadline,"cold name drain deadlocked: {}",app.status_msg);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        app.finish_deferred_exit_after_hid_write(&ctx);
        assert!(!app.exit_after_hid_write);
        assert_eq!(*store.lock().unwrap(),"Cold");
        assert_eq!(app.layer_names[1],"Cold");
        assert!(packets.requests().iter().any(|r| r[..4]==[0xfe,0x0b,201,0]));
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn name_review_drain(app: &mut EntropyApp, ctx: &egui::Context) {
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(2);
        while app.hid_write_lifecycle_busy() {
            app.poll_vial_hid_task(ctx);
            assert!(std::time::Instant::now()<deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ui_unconfirmed_name_is_read_before_other_name_publication() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for mismatch in [false,true] {
            let ctx=egui::Context::default(); let mut app=name_review_app();
            let (hid,packets)=crate::hid::HidDevice::test_device();
            let store=std::sync::Arc::new(std::sync::Mutex::new(vec!["Old".to_owned();16]));
            let invalid=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let firmware=store.clone(); let fail=invalid.clone();
            packets.respond_by(move |r| {
                let layer=u16::from_le_bytes([r[2],r[3]]).wrapping_sub(200) as usize;
                if layer>=16 {return None;}
                if r[..2]==[0xfe,0x0b] {
                    let end=r[4..].iter().position(|b| *b==0).unwrap_or(28);
                    firmware.lock().unwrap()[layer]=String::from_utf8(r[4..4+end].to_vec()).unwrap();
                    Some(Ok([0;32]))
                } else if r[..2]==[0xfe,0x0a] {
                    if fail.load(std::sync::atomic::Ordering::SeqCst) {
                        return Some(Ok(if mismatch {[0;32]} else {[0xff;32]}));
                    }
                    let store=firmware.lock().unwrap(); let mut response=[0;32];
                    response[1..1+store[layer].len()].copy_from_slice(store[layer].as_bytes()); Some(Ok(response))
                } else {None}
            });
            app.hid_device=Some(hid); app.layer_names[0]="Old".into();
            app.layout.as_mut().unwrap().layer_names[0]="Old".into();
            name_review_rename(&mut app,&ctx,"Changed"); name_review_drain(&mut app,&ctx);
            assert_eq!(store.lock().unwrap()[0],"Changed");
            let before=packets.requests().len();
            for _ in 0..3 {app.poll_vial_hid_task(&ctx);name_review_frame(&mut app,&ctx,vec![]);}
            assert_eq!(packets.requests().len(),before,"repaint retried an uncertain write");
            invalid.store(false,std::sync::atomic::Ordering::SeqCst);
            app.application_layout_editor_active=false; app.selected_layer=1;
            name_review_rename(&mut app,&ctx,"Other");
            assert_eq!(crate::app::app_storage::load_layer_names("Macropad")[0],"Changed",
                "ordinary rename republished a stale firmware name");
            assert_eq!(app.layout.as_ref().unwrap().layer_names[0],"Changed");
            assert_eq!(packets.requests().iter().filter(|r|r[..4]==[0xfe,0x0b,200,0]).count(),1,
                "read-only recovery must not retry SET");
            let key=app.application_layout_device_key().unwrap();
            assert_eq!(app.app_settings.application_layouts[&key].layouts[crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID].layer_names[0],"Changed");
            // Explicitly restoring the previously confirmed value still requires SET+GET.
            app.application_layout_editor_active=true;app.selected_layer=0;
            name_review_rename(&mut app,&ctx,"Old"); name_review_drain(&mut app,&ctx);
            assert_eq!(store.lock().unwrap()[0],"Old");
            assert_eq!(packets.requests().iter().filter(|r|r[..4]==[0xfe,0x0b,200,0]).count(),2);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ui_same_layer_rename_preserves_explicit_edit_after_uncertainty_recovery() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let ctx = egui::Context::default();
        let mut app = name_review_app();
        let (hid, packets) = crate::hid::HidDevice::test_device();
        let store = std::sync::Arc::new(std::sync::Mutex::new(vec!["Old".to_owned();16]));
        let invalid = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let firmware = store.clone();
        let fail = invalid.clone();
        packets.respond_by(move |r| {
            let layer = u16::from_le_bytes([r[2],r[3]]).wrapping_sub(200) as usize;
            if layer >= 16 { return None; }
            if r[..2] == [0xfe,0x0b] {
                let end = r[4..].iter().position(|b| *b == 0).unwrap_or(28);
                firmware.lock().unwrap()[layer] = String::from_utf8(r[4..4+end].to_vec()).unwrap();
                Some(Ok([0;32]))
            } else if r[..2] == [0xfe,0x0a] {
                if fail.load(std::sync::atomic::Ordering::SeqCst) { return Some(Ok([0xff;32])); }
                let store = firmware.lock().unwrap();
                let mut response = [0;32];
                response[1..1+store[layer].len()].copy_from_slice(store[layer].as_bytes());
                Some(Ok(response))
            } else { None }
        });
        app.hid_device = Some(hid);
        app.layer_names[0] = "Old".into();
        app.layout.as_mut().unwrap().layer_names[0] = "Old".into();
        name_review_rename(&mut app, &ctx, "Changed");
        name_review_drain(&mut app, &ctx);
        assert_eq!(store.lock().unwrap()[0], "Changed");
        assert!(app.selected_layer_names_uncertain());
        let key = app.application_layout_device_key().unwrap();
        let draft = app.app_settings.application_layouts[&key].layouts[crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID].clone();
        invalid.store(false, std::sync::atomic::Ordering::SeqCst);
        app.application_layout_editor_active = false;
        name_review_rename(&mut app, &ctx, "Other");
        assert_eq!(store.lock().unwrap()[0], "Other");
        assert_eq!(app.layer_names[0], "Other", "recovery overwrote the explicit same-layer edit");
        assert_eq!(crate::app::app_storage::load_layer_names("Macropad")[0], "Other");
        assert!(!app.selected_layer_names_uncertain());
        assert_eq!(app.app_settings.application_layouts[&key].layouts[crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID], draft,
            "firmware recovery must preserve the host profile draft");
        app.selected_layer = 1;
        name_review_rename(&mut app, &ctx, "Next");
        assert_eq!(store.lock().unwrap()[0], "Other");
        assert_eq!(app.layer_names[0], "Other");
        assert_eq!(crate::app::app_storage::load_layer_names("Macropad")[0], "Other",
            "later ordinary publication restored the recovered old name");
        assert_eq!(packets.requests().iter().filter(|r| r[..4] == [0xfe,0x0b,200,0]).count(), 2,
            "recovery must not add an automatic SET retry");
        assert_eq!(crate::app::app_storage::load_app_settings().application_layouts[&key].layouts[crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID], draft);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ui_name_write_errors_use_selected_language() {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        for (language,prefix,reason) in [
            (crate::i18n::Language::Russian,"Имя слоя не сохранено", "данные слоя не готовы"),
            (crate::i18n::Language::English,"Layer name not saved", "layer data is not ready"),
        ] {
            let ctx=egui::Context::default(); let mut app=name_review_app();
            app.app_settings.language=language;
            let (hid,_)=crate::hid::HidDevice::test_device();app.hid_device=Some(hid);
            name_review_rename(&mut app,&ctx,"Test");
            app.deferred_device_load.set_layer_status(0,DeferredLoadStatus::Failed("read error".into()));
            app.poll_vial_hid_task(&ctx);
            assert_eq!(app.status_msg,format!("{prefix}: {reason}"));
            for (en,ru) in [
                ("device changed","устройство изменилось"),
                ("device is busy","устройство занято"),
                ("device is disconnected","устройство отключено"),
                ("layer data is not ready","данные слоя не готовы"),
                ("layer name readback mismatch","прочитанное имя слоя не совпадает"),
            ] {
                let expected=if language==crate::i18n::Language::Russian {ru} else {en};
                assert_eq!(super::super::vial_hid_task::layer_name_write_error(language,en),format!("{prefix}: {expected}"));
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ui_uncertain_name_gui_export_rechecks_before_writing() {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        let ctx=egui::Context::default(); let mut app=name_review_app();
        let (hid,packets)=crate::hid::HidDevice::test_device();
        packets.respond_by(|r| {
            if r[..2]==[0xfe,0x0b] {Some(Ok([0;32]))}
            else if r[..2]==[0xfe,0x0a] {Some(Ok([0xff;32]))} else {None}
        });
        app.hid_device=Some(hid);
        name_review_rename(&mut app,&ctx,"Draft");name_review_drain(&mut app,&ctx);
        let before=packets.requests().iter().filter(|r|r[..2]==[0xfe,0x0a]).count();
        let path=std::env::temp_dir().join(format!("name-export-{}.entlayout",std::process::id()));
        std::fs::write(&path,b"preserve existing export").unwrap();
        app.write_entlayout_export(&path);
        assert!(packets.requests().iter().filter(|r|r[..2]==[0xfe,0x0a]).count()>before,
            "GUI export consumed uncertainty without a fresh read");
        assert_eq!(std::fs::read(&path).unwrap(),b"preserve existing export");
        assert!(app.entlayout_export_json().unwrap().is_err());
        assert_eq!(packets.requests().iter().filter(|r|r[..2]==[0xfe,0x0b]).count(),1);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ui_successive_name_edits_keep_newer_draft_across_delayed_completion() {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        for cancel_generation in [false,true] {
            let ctx=egui::Context::default(); let mut app=name_review_app();
            let (hid,packets)=crate::hid::HidDevice::test_device();
            let store=std::sync::Arc::new(std::sync::Mutex::new(String::new()));
            let firmware=store.clone();
            let (release,gate)=std::sync::mpsc::channel();let gate=std::sync::Mutex::new(Some(gate));
            let (entered,waiting)=std::sync::mpsc::channel();
            packets.respond_by(move |r| {
                if r[..2]==[0xfe,0x0b] {
                    let end=r[4..].iter().position(|b|*b==0).unwrap_or(28);
                    *firmware.lock().unwrap()=String::from_utf8(r[4..4+end].to_vec()).unwrap();
                    Some(Ok([0;32]))
                } else if r[..2]==[0xfe,0x0a] {
                    if let Some(gate)=gate.lock().unwrap().take() {
                        entered.send(()).unwrap();gate.recv_timeout(std::time::Duration::from_secs(3)).unwrap();
                    }
                    let name=firmware.lock().unwrap();let mut response=[0;32];
                    response[1..1+name.len()].copy_from_slice(name.as_bytes());Some(Ok(response))
                } else {None}
            });
            app.hid_device=Some(hid);
            name_review_rename(&mut app,&ctx,"First");app.poll_vial_hid_task(&ctx);
            waiting.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
            name_review_rename(&mut app,&ctx,"Second");
            let key=app.application_layout_device_key().unwrap();
            let (unrelated,unrelated_packets)=crate::hid::HidDevice::test_device();
            if cancel_generation {
                app.connection_generation+=1;
                app.current_keyboard_id=Some(0x99);
                app.hid_device=Some(unrelated);
            }
            release.send(()).unwrap();name_review_drain(&mut app,&ctx);
            assert_eq!(app.app_settings.application_layouts[&key].layouts[crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID].layer_names[0],"Second");
            assert!(unrelated_packets.requests().is_empty());
            if cancel_generation {
                assert_eq!(*store.lock().unwrap(),"First");
                assert_ne!(app.layer_names[0],"First","stale completion published to new generation");
            } else {
                assert_eq!(*store.lock().unwrap(),"Second");assert_eq!(app.layer_names[0],"Second");
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn pending_cold_name_without_owner_terminates_visibly() {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        let ctx=egui::Context::default();let mut app=name_review_app();
        app.deferred_device_load.set_layer_status(0,DeferredLoadStatus::Loading);
        app.rename_application_layout_layer(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID,0,"Draft".into());
        assert!(app.defer_exit_until_hid_write_returns(&ctx));
        app.poll_vial_hid_task(&ctx);
        assert!(app.pending_layer_name_writes.is_empty(),"orphaned loading state blocked exit forever");
        assert!(!app.status_msg.is_empty());
        app.finish_deferred_exit_after_hid_write(&ctx);assert!(!app.exit_after_hid_write);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn uncertainty_is_scoped_to_device_not_shared_model_uid() {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        let ctx=egui::Context::default();let mut app=name_review_app();
        for fail in [true,false] {
            let (hid,packets)=crate::hid::HidDevice::test_device();
            packets.respond_by(move |r| {
                if r[..2]==[0xfe,0x0b] {Some(Ok([0;32]))}
                else if r[..2]==[0xfe,0x0a] {
                    let mut response=[0;32];response[1..6].copy_from_slice(b"Draft");
                    Some(Ok(if fail {[0xff;32]} else {response}))
                } else {None}
            });
            if !fail {
                let mut other=app.device_manager.devices()[0].clone();other.path="another-unit".into();
                app.device_manager.replace_devices(vec![other]);app.connection_generation+=1;
                mark_application_layout_protocol_supported(&mut app);
            }
            app.hid_device=Some(hid);
            app.rename_application_layout_layer(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID,0,"Draft".into());
            name_review_drain(&mut app,&ctx);
        }
        assert_eq!(app.uncertain_layer_names.len(),1,"another unit's success discarded first unit's uncertainty");
        assert_ne!(app.uncertain_layer_names[0].device.path,"another-unit");
    }

    #[test]
    fn foreground_catalog_refresh_keeps_layer_rename_open() {
        use crate::application_layouts::{DetectedApplication, DeviceApplicationLayouts};
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        // No HID transport or host discovery thread: only the OS snapshot boundary
        // is supplied; real egui clicks/text and the production runtime run below.
        for nondefault in [false, true] {
            let ctx = egui::Context::default();
            let mut app = EntropyApp::new_inert_for_test();
            let key = "rename-catalog-refresh".to_owned();
            let mut settings = DeviceApplicationLayouts::default();
            let profile = if nondefault {
                settings.create_for_application(&DetectedApplication {
                    executable: "calculator.exe".into(),
                    ..Default::default()
                })
            } else {
                crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID.to_owned()
            };
            settings.active_layout_id = profile.clone();
            settings.editor_layout_id = profile.clone();
            app.app_settings
                .application_layouts
                .insert(key.clone(), settings);
            app.app_settings.last_application_layout_device_key = Some(key.clone());
            app.application_layout_editor_active = true;
            let foreground = DetectedApplication {
                executable: "entropy.exe".into(),
                identities: vec!["entropy.exe".into()],
                display_name: "entropy.exe".into(),
                window_title: "Entropy".into(),
            };
            app.application_layout_foreground = Some(foreground.clone());
            // Match the production state after explicitly selecting a profile.
            app.application_layout_manual_override = Some((key.clone(), Some(foreground.clone())));
            let frame = |app: &mut EntropyApp, events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1100.0, 800.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| app.draw_layout_layer_switcher_and_hints(ui, 0.0, 0.0, 52.0),
                )
                .drop_without_applying_deltas();
            };
            let pos = egui::pos2(550.0, 50.0);
            frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                frame(
                    &mut app,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            frame(&mut app, vec![]);
            assert_eq!(app.editing_layer, Some(0), "click must start editing");
            // A catalog scan enriches metadata without changing focus or profile.
            let mut refreshed = foreground;
            refreshed.display_name = "Entropy".into();
            app.update_application_layout_runtime_with(
                crate::app_discovery::ApplicationDiscoverySnapshot {
                    foreground: Some(refreshed.clone()),
                    foreground_status: crate::app_discovery::ForegroundStatus {
                        backend: "synthetic Win32 snapshot + catalog".into(),
                        state: crate::app_discovery::ForegroundState::Focused(refreshed),
                    },
                    ..Default::default()
                },
            );
            assert_eq!(
                app.application_layout_settings().unwrap().active_layout_id,
                profile
            );
            assert_eq!(
                app.editing_layer,
                Some(0),
                "background metadata refresh closed rename (nondefault={nondefault})"
            );
            frame(&mut app, vec![egui::Event::Text("New".into())]);
            assert!(app.editing_layer_text.contains("New"));
            let proposed = app.editing_layer_text.clone();
            frame(
                &mut app,
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            assert!(app.editing_layer.is_none());
            let persisted = crate::app::app_storage::load_app_settings();
            assert_eq!(
                persisted.application_layouts[&key].layouts[&profile].layer_names[0],
                proposed
            );
        }
    }

    #[test]
    fn genuine_foreground_context_changes_commit_layer_rename() {
        use crate::application_layouts::{
            DetectedApplication, DeviceApplicationLayouts, DEFAULT_APPLICATION_LAYOUT_ID,
        };
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for nondefault in [false, true] {
            for title_only in [false, true] {
                let ctx = egui::Context::default();
                let mut app = EntropyApp::new_inert_for_test();
                let key = "rename-genuine-context".to_owned();
                let mut settings = DeviceApplicationLayouts::default();
                let foreground = DetectedApplication {
                    executable: "editor.exe".into(),
                    window_title: "First document".into(),
                    ..Default::default()
                };
                let source = if nondefault {
                    settings.create_for_application(&foreground)
                } else {
                    DEFAULT_APPLICATION_LAYOUT_ID.to_owned()
                };
                settings.active_layout_id = source.clone();
                settings.editor_layout_id = source.clone();
                app.app_settings
                    .application_layouts
                    .insert(key.clone(), settings);
                app.app_settings.last_application_layout_device_key = Some(key.clone());
                app.application_layout_editor_active = true;
                app.application_layout_foreground = Some(foreground.clone());
                app.application_layout_manual_override =
                    Some((key.clone(), Some(foreground.clone())));
                let frame = |app: &mut EntropyApp, events| {
                    ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1100.0, 800.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| app.draw_layout_layer_switcher_and_hints(ui, 0.0, 0.0, 52.0),
                    )
                    .drop_without_applying_deltas();
                };
                let pos = egui::pos2(550.0, 50.0);
                frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
                for pressed in [true, false] {
                    frame(
                        &mut app,
                        vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                    );
                }
                frame(&mut app, vec![]);
                assert_eq!(app.editing_layer, Some(0));
                frame(&mut app, vec![egui::Event::Text("Draft".into())]);
                let draft = app.editing_layer_text.clone();
                assert!(draft.contains("Draft"));
                let changed = if title_only {
                    DetectedApplication {
                        window_title: "Second document".into(),
                        ..foreground
                    }
                } else {
                    DetectedApplication {
                        executable: "other.exe".into(),
                        ..Default::default()
                    }
                };
                app.update_application_layout_runtime_with(
                    crate::app_discovery::ApplicationDiscoverySnapshot {
                        foreground: Some(changed.clone()),
                        foreground_status: crate::app_discovery::ForegroundStatus {
                            backend: "synthetic context transition".into(),
                            state: crate::app_discovery::ForegroundState::Focused(changed),
                        },
                        ..Default::default()
                    },
                );
                assert!(
                    app.editing_layer.is_none(),
                    "nondefault={nondefault}, title_only={title_only}"
                );
                assert!(app.editing_layer_layout_id.is_none());
                let persisted = crate::app::app_storage::load_app_settings();
                assert_eq!(
                    persisted.application_layouts[&key].layouts[&source].layer_names[0],
                    draft
                );
                assert_eq!(app.application_layout_manual_override.is_some(), title_only);
            }
        }
    }

    #[test]
    fn metadata_alias_profile_transition_preserves_layer_rename() {
        metadata_transition_preserves_layer_rename(true);
    }

    #[test]
    fn metadata_editor_follow_preserves_layer_rename() {
        metadata_transition_preserves_layer_rename(false);
    }

    fn metadata_transition_preserves_layer_rename(alias_changes_active_profile: bool) {
        use crate::application_layouts::{
            DetectedApplication, DeviceApplicationLayouts, DEFAULT_APPLICATION_LAYOUT_ID,
        };
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        let key = "rename-metadata-transition".to_owned();
        let mut settings = DeviceApplicationLayouts::default();
        let target = settings.create_for_application(&DetectedApplication {
            executable: "com.example.editor".into(),
            ..Default::default()
        });
        let source = if alias_changes_active_profile {
            DEFAULT_APPLICATION_LAYOUT_ID.to_owned()
        } else {
            target.clone()
        };
        settings.active_layout_id = source.clone();
        settings.editor_layout_id = DEFAULT_APPLICATION_LAYOUT_ID.into();
        app.app_settings
            .application_layouts
            .insert(key.clone(), settings);
        app.app_settings.last_application_layout_device_key = Some(key.clone());
        app.application_layout_editor_active = true;
        app.selected_layer = 3;
        let mut foreground = DetectedApplication {
            executable: "entropy-host.exe".into(),
            identities: vec!["entropy-host.exe".into()],
            display_name: "entropy-host.exe".into(),
            window_title: "Editor".into(),
        };
        if !alias_changes_active_profile {
            foreground.identities.push("com.example.editor".into());
        }
        app.application_layout_foreground = Some(foreground.clone());
        assert!(app.application_layout_manual_override.is_none());
        let frame = |app: &mut EntropyApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.draw_layout_layer_switcher_and_hints(ui, 0.0, 0.0, 52.0),
            )
            .drop_without_applying_deltas();
        };
        let position = egui::pos2(550.0, 50.0);
        frame(&mut app, vec![egui::Event::PointerMoved(position)]);
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        frame(&mut app, vec![]);
        assert_eq!(app.editing_layer, Some(3));
        assert_eq!(
            app.editing_layer_layout_id.as_deref(),
            Some(source.as_str())
        );
        frame(&mut app, vec![egui::Event::Text("Draft".into())]);
        let draft = app.editing_layer_text.clone();
        assert!(draft.contains("Draft"));
        let mut refreshed = foreground.clone();
        if alias_changes_active_profile {
            refreshed.identities.push("com.example.editor".into());
        } else {
            refreshed.display_name = "Editor".into();
        }
        assert!(same_foreground_application(
            Some(&foreground),
            Some(&refreshed)
        ));
        app.update_application_layout_runtime_with(
            crate::app_discovery::ApplicationDiscoverySnapshot {
                foreground: Some(refreshed.clone()),
                foreground_status: crate::app_discovery::ForegroundStatus {
                    backend: "synthetic metadata snapshot".into(),
                    state: crate::app_discovery::ForegroundState::Focused(refreshed),
                },
                ..Default::default()
            },
        );
        let settings = app.application_layout_settings().unwrap();
        assert_eq!(settings.active_layout_id, target);
        assert_eq!(settings.editor_layout_id, target);
        assert_eq!(app.selected_layer, 0);
        assert_eq!(
            settings.layouts[&source].layer_names[3], draft,
            "metadata transition must commit to the captured source profile"
        );
        assert!(app.editing_layer.is_none());
        assert!(app.editing_layer_layout_id.is_none());
        frame(&mut app, vec![]);
        // Starting the next rename must not overwrite the source draft.
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(app.editing_layer, Some(0));
        assert_eq!(
            app.editing_layer_layout_id.as_deref(),
            Some(target.as_str())
        );
        let persisted = crate::app::app_storage::load_app_settings();
        assert_eq!(
            persisted.application_layouts[&key].layouts[&source].layer_names[3],
            draft
        );
        assert_ne!(
            persisted.application_layouts[&key].layouts[&target].layer_names[0],
            draft
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
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "requires MACROPAD_WAKE_EXE production C/LVGL harness"]
    fn autolayout_standby_wake_host_to_firmware() {
        let exe = std::env::var("MACROPAD_WAKE_EXE").expect("build run_macropad_wake.py first");
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let mut app = EntropyApp::new_inert_for_test();
        let mut device = m4cr0pad_v3_device();
        device.path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/main.rs")
            .to_string_lossy()
            .into_owned();
        app.device_manager.replace_devices(vec![device.clone()]);
        app.selected_device = Some(0);
        app.current_keyboard_id = Some(0x42);
        app.current_device_name = device.name.clone();
        app.layout = Some(indicator_test_layout());
        mark_application_layout_protocol_supported(&mut app);
        let (hid, packets) = crate::hid::HidDevice::test_device();
        packets.respond_by(|request| {
            if request[0] == 0xe6 {
                let mut response = [0; 32];
                response[..3].copy_from_slice(&[0xe6, 7, 0x5a]);
                Some(Ok(response))
            } else {
                None
            }
        });
        let bridge = crate::qmk_hid_host::test_start_profile_handoff_bridge(
            device.clone(),
            hid.shared_output(),
        );
        bridge.set_selected_application_layout_support(true);
        app.qmk_hid_hosts.insert(device.path.clone(), bridge);
        app.shared_hid_output = hid.shared_output();
        app.hid_device = Some(hid);
        app.remember_connected_application_layout_device();
        let key = app.application_layout_device_key().unwrap();
        let foreground = |executable: &str| crate::application_layouts::DetectedApplication {
            executable: executable.into(),
            display_name: executable.into(),
            ..Default::default()
        };
        let a = foreground("wake-editor-a");
        let b = foreground("wake-editor-b");
        let settings = app
            .app_settings
            .application_layouts
            .entry(key.clone())
            .or_default();
        let aid = settings.create_for_application(&a);
        let bid = settings.create_for_application(&b);
        settings
            .layouts
            .get_mut(&aid)
            .unwrap()
            .set_keycode(0, 0, 0x0010);
        settings
            .layouts
            .get_mut(&aid)
            .unwrap()
            .set_keycode(0, 14, 0x00a9);
        settings
            .layouts
            .get_mut(&aid)
            .unwrap()
            .set_keycode(0, 13, 0x0010);
        // Distinct identity, identical visible names/bindings/revisions, no content heuristic.
        let mut twin = settings.layouts[&aid].clone();
        twin.id = bid.clone();
        twin.executable = b.executable.clone();
        settings.layouts.insert(bid, twin);
        let mut cursor = 0usize;
        let mut trace = String::new();
        let mut route = |app: &mut EntropyApp,
                         foreground: Option<crate::application_layouts::DetectedApplication>,
                         suffix: &str,
                         force: bool| {
            app.update_application_layout_runtime_with(
                crate::app_discovery::ApplicationDiscoverySnapshot {
                    foreground: foreground.clone(),
                    foreground_status: crate::app_discovery::ForegroundStatus {
                        backend: "injected OS".into(),
                        state: foreground
                            .map(crate::app_discovery::ForegroundState::Focused)
                            .unwrap_or(crate::app_discovery::ForegroundState::NoFocusedWindow),
                    },
                    ..Default::default()
                },
            );
            if force {
                app.qmk_hid_hosts[&device.path].force_application_layout_resend();
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
            loop {
                let requests = packets.requests();
                if let Some(end) = requests[cursor..].iter().position(|p| p[0] == 0xe3) {
                    for p in &requests[cursor..=cursor + end] {
                        if matches!(p[0],0xe1..=0xe5|0xe7..=0xea) {
                            trace.push('P');
                            for byte in &p[..32] {
                                trace.push_str(&format!(" {byte:02x}"));
                            }
                            trace.push('\n');
                        }
                    }
                    cursor += end + 1;
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "worker must commit semantic profile transition"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            trace.push_str(suffix);
            if suffix == "N\n" {
                // Capture an actual worker heartbeat, not a hand-built packet.
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                loop {
                    let requests = packets.requests();
                    if let Some(p) = requests[cursor..].iter().find(|p| p[0] == 0xe5) {
                        trace.push('P');
                        for byte in &p[..32] {
                            trace.push_str(&format!(" {byte:02x}"));
                        }
                        trace.push_str("\nN\n");
                        cursor = requests.len();
                        break;
                    }
                    assert!(
                        std::time::Instant::now() < deadline,
                        "worker heartbeat missing"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            if let Ok(path) = std::env::var("MACROPAD_WAKE_TRACE") {
                std::fs::write(path, &trace).unwrap();
            }
            use std::io::Write;
            let mut child = std::process::Command::new(&exe)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(trace.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            println!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                output.status.success(),
                "production firmware standby regression: {}",
                output.status
            );
        };
        // Establish initial state, then sleep before the reported transition.
        route(&mut app, None, "S\n", true);
        route(&mut app, Some(a.clone()), "W\nS\n", false);
        // Repeated full synchronizations must not reset standby.
        route(&mut app, Some(a.clone()), "N\n", true);
        route(&mut app, Some(b.clone()), "W\nS\n", false);
        let active = app
            .app_settings
            .application_layouts
            .get_mut(&key)
            .unwrap()
            .active_layout_id
            .clone();
        let edited = app
            .app_settings
            .application_layouts
            .get_mut(&key)
            .unwrap()
            .layouts
            .get_mut(&active)
            .unwrap();
        edited.name.push_str(" renamed");
        edited.bump_revision();
        route(&mut app, Some(b.clone()), "N\n", false);
        route(&mut app, None, "W\nS\nD\n", false);
        route(&mut app, Some(a.clone()), "N\nE\nN\nR\nV\nK\nS\nX\n", false);
        route(&mut app, Some(b.clone()), "W\nS\nO\n", true);
        route(&mut app, Some(a.clone()), "F\nK\nS\n", false);
        // UI selection changes ownership, not the foreground's Default identity.
        route(&mut app, None, "W\nS\n", false);
        app.selected_device = None;
        route(&mut app, None, "N\n", true);
        let background = app.qmk_hid_hosts[&device.path].test_application_layout_snapshot();
        assert!(
            !background.active,
            "no authority to replay a cached firmware Default"
        );
        assert_eq!(
            background.profile_id.as_deref(),
            Some(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID),
            "background Default must retain semantic identity"
        );
        app.selected_device = Some(0);
        route(&mut app, None, "N\n", true);
        app.selected_device = None;
        route(&mut app, Some(a), "W\nG\n", false);
        route(&mut app, Some(b.clone()), "Z\n", false);
        // A real application -> background firmware-owned Default must wake.
        route(&mut app, None, "W\nS\nT\n", false);
        route(&mut app, Some(b), "W\nY\n", false);
        drop(route);
        if let Ok(path) = std::env::var("MACROPAD_WAKE_TRACE") {
            std::fs::write(path, &trace).unwrap();
        }
        use std::io::Write;
        let mut child = std::process::Command::new(exe)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(trace.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        println!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.status.success(),
            "production firmware standby regression: {}",
            output.status
        );
    }
}
