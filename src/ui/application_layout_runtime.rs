use super::*;

/// A successful durable library save can be reused by input-driven repaint ticks.
/// Any library change, storage path/identity change or metadata failure revokes it.
#[cfg(not(target_arch = "wasm32"))]
pub(super) struct PersistedApplicationLibrary {
    settings: crate::application_layouts::DeviceApplicationLayouts,
    file: ApplicationSettingsFileStamp,
}
#[cfg(not(target_arch = "wasm32"))]
#[derive(PartialEq, Eq)]
struct ApplicationSettingsFileStamp {
    path: std::path::PathBuf,
    len: u64,
    modified: std::time::SystemTime,
    created: Option<std::time::SystemTime>,
    #[cfg(unix)]
    identity: (u64,u64,i64,i64),
}
#[cfg(not(target_arch = "wasm32"))]
impl ApplicationSettingsFileStamp {
    fn current()->Option<Self> {
        let path=super::app_storage::app_settings_path();
        let metadata=std::fs::metadata(&path).ok()?;
        if !metadata.is_file() {return None;}
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Some(Self {path,len:metadata.len(),modified:metadata.modified().ok()?,created:metadata.created().ok(),
            #[cfg(unix)]
            identity:(metadata.dev(),metadata.ino(),metadata.ctime(),metadata.ctime_nsec()),
        })
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl PersistedApplicationLibrary {
    fn matches(&self,settings:&crate::application_layouts::DeviceApplicationLayouts)->bool {
        self.settings==*settings && ApplicationSettingsFileStamp::current().as_ref()==Some(&self.file)
    }
    fn capture(device_key:&str,settings:&crate::application_layouts::DeviceApplicationLayouts)->Option<Self> {
        // Do not bless a replacement file that appeared between save and stat.
        let file=ApplicationSettingsFileStamp::current()?;
        let bytes=std::fs::read(&file.path).ok()?;
        let stored:AppSettings=serde_json::from_slice(&bytes).ok()?;
        if stored.application_layouts.get(device_key)!=Some(settings)
            || ApplicationSettingsFileStamp::current().as_ref()!=Some(&file) {return None;}
        Some(Self {settings:settings.clone(),file})
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn reconcile_default_profile_from_device<T: AsRef<[u16]>>(
    profile: &mut crate::application_layouts::ApplicationLayout,
    device_layers: &[T;
         crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT],
    pending_edits: u16,
) -> bool {
    let mut changed = false;
    for (layer, keycodes) in device_layers.iter().enumerate() {
        if pending_edits & (1u16 << layer) != 0 {
            continue;
        }
        for (control, keycode) in keycodes.as_ref().iter().enumerate() {
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
        Category::Games => ("Игры", "Games"),
    };
    app_layout_text(language, ru, en)
}

pub(crate) fn application_layout_category_for_executable(
    executable: &str,
) -> crate::application_layouts::ApplicationLayoutCategory {
    crate::application_layouts::application_layout_category_for_executable(executable)
}

pub(super) fn device_supports_application_layouts(device: &crate::device::Device) -> bool {
    device.application_layout_protocol_version().is_some()
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
        Some(device.application_layout_storage_key(key))
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
                && (!bridge.device().is_ergohaven_sm62()
                    || devices.iter().filter(|d| d.may_share_physical_device(bridge.device())).count() == 1)
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
            // SM62 intentionally shares one model library across distinct units.
            // Repeated VID/PID is not ambiguity when OS/serial evidence proves
            // separate physical owners. Aliases/unknown same-model identity must
            // still fail closed; discovery UID alone never grants an endpoint.
            let unique_owner = if target.is_ergohaven_sm62() {
                devices.iter().filter(|d| d.may_share_physical_device(target)).count() == 1
                    && devices.iter().any(|d| d.path == target.path
                        && d.instance_token == target.instance_token
                        && d.permits_hid_target(target))
            } else {
                devices.iter().filter(|d| d.vendor_id == target.vendor_id
                    && d.product_id == target.product_id).count() == 1
            };
            if !devices.iter().any(|d| d.permits_hid_target(target))
                || !unique_owner
                || (target.serial_number.trim().is_empty() && !target.is_ergohaven_sm62())
                || !self.app_settings.application_layouts.contains_key(&key)
            {
                bridge.clear_application_layout_binding();
                continue;
            }
            // One common SM62 library, independent of the replaceable peer or
            // which half currently owns USB. This grants only this newly proven
            // owner/generation; endpoint presence, UID, v8 capability, uniqueness
            // and stale-publication fences above remain mandatory. Macropad keeps
            // its existing unit-serial policy.
            if target.is_ergohaven_sm62() {
                // Retain the legacy binding record for compatibility, but it
                // must not prevent a verified replacement USB half from using
                // this intentionally common model library.
                if !self.app_settings.application_layout_bindings.contains_key(&key) {
                    self.app_settings.application_layout_bindings.insert(key.clone(), (
                        target.vendor_id, target.product_id, target.serial_number.clone()));
                    changed = true;
                }
                bridge.bind_verified_application_layout_device(key, generation);
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

    fn application_layer_seed(&self, layout: &KeyboardLayout) -> [Vec<u16>; crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT] {
        let mut seed=Self::base_application_layers(layout);
        #[cfg(not(target_arch = "wasm32"))]
        if seed[0].len()==62 {
            for (layer,codes) in seed.iter_mut().enumerate() {
                if !self.deferred_device_load.layer_status(layer).ready() {
                    codes.fill(crate::application_layouts::APPLICATION_LAYOUT_UNSET_KEYCODE);
                }
            }
        }
        seed
    }

    pub(super) fn application_layout_settings_mut(
        &mut self,
    ) -> Option<&mut crate::application_layouts::DeviceApplicationLayouts> {
        let key = self.application_layout_device_key()?;
        // Capture the owner context before a caller can change an edit counter.
        #[cfg(not(target_arch = "wasm32"))]
        self.refresh_application_layout_edit_owners(&key);
        let seed = self.layout.as_ref().filter(|layout| layout.keys.len() == 62 && layout.rows == 10 && layout.cols == 7)
            .map(|layout| self.application_layer_seed(layout));
        let settings = self.app_settings.application_layouts.entry(key).or_insert_with(|| {
            let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
            if let Some(seed) = seed {
                settings.layouts.get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID).unwrap().layers = seed.to_vec();
            }
            settings
        });
        Some(settings)
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
                let layout = settings.layouts.get_mut(id).unwrap();
                layout.user_edit_revision = layout.user_edit_revision.wrapping_add(1).max(1);
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
        #[cfg(not(target_arch = "wasm32"))]
        self.refresh_application_layout_edit_owners(&device_key);
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
            && self
                .editing_layer_generation
                .is_none_or(|g| g == self.connection_generation)
        {
            if let Some(device) = self
                .selected_device
                .and_then(|i| self.device_manager.devices().get(i))
            {
                self.pending_layer_name_writes
                    .push_back(super::vial_hid_task::LayerNameWrite {
                        device: device.clone(),
                        device_key,
                        layout_id: layout_id.to_owned(),
                        generation: self.connection_generation,
                        layer,
                        name,
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
        self.rename_application_layout_layer(&layout_id, layer, name);
        self.editing_layer = None;
        self.editing_layer_text.clear();
        self.editing_layer_focus_requested = false;
        self.editing_layer_layout_id = None;
        self.editing_layer_device_key = None;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.editing_layer_generation = None;
        }
    }

    pub(super) fn application_control_for_key(
        layout: &KeyboardLayout,
        key_index: usize,
    ) -> Option<usize> {
        let key = layout.keys.get(key_index)?;
        if layout.rows == 10 && layout.cols == 7 && layout.keys.len() == 62 {
            return crate::application_layouts::SM62_APPLICATION_POSITIONS
                .iter().position(|&position| position == (key.row, key.col));
        }
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

    pub(super) fn base_application_layers(
        layout: &KeyboardLayout,
    ) -> [Vec<u16>;
           crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT] {
        let controls = if layout.rows == 10 && layout.cols == 7 && layout.keys.len() == 62 {
            62
        } else { crate::application_layouts::APPLICATION_LAYOUT_CONTROL_COUNT };
        let mut layers: [Vec<u16>; crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT] =
            std::array::from_fn(|_| vec![0; controls]);
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
        let controls = if layout.rows == 10 && layout.cols == 7 && layout.keys.len() == 62 {
            62
        } else {
            crate::application_layouts::APPLICATION_LAYOUT_CONTROL_COUNT
        };
        // Old persisted profiles may predate the import geometry fence. Refuse
        // the whole incompatible overlay rather than panic or partially map it.
        if profile.layers.len() != crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT
            || profile.layers.iter().any(|layer| layer.len() != controls)
            || (controls == 62 && profile.encoder_stacks.iter().any(|stack| !stack.is_empty()))
        {
            return rendered;
        }
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
        keycodes: &[u16],
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
            let device_names = (layout.rows == 10 && layout.cols == 7 && layout.keys.len() == 62)
                .then(|| layout.layer_names.clone());
            let mut name_mask = self.confirmed_firmware_layer_names;
            let device_key = self.application_layout_device_key();
            for request in self.pending_layer_name_writes.iter().chain(self.uncertain_layer_names.iter()) {
                if request.layer < 16 && request.generation == self.connection_generation
                    && device_key.as_ref() == Some(&request.device_key) {
                    name_mask &= !(1u16 << request.layer);
                }
            }
            if self.editing_layer_layout_id.as_deref() == Some(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
                && self.editing_layer_device_key.as_ref() == device_key.as_ref() {
                if let Some(layer) = self.editing_layer.filter(|layer| *layer < 16) {
                    name_mask &= !(1u16 << layer);
                }
            }
            let pending = self.default_layout_pending_layers;
            // Preserve an older host-only Default before reconciling its labels
            // with the confirmed keyboard. Pending edits are excluded above.
            if let Some(previous) = self.application_layout_settings().cloned() {
                let differs = previous.layouts.get(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
                    .zip(device_names.as_ref()).is_some_and(|(profile, names)| names.iter().take(16).enumerate().any(|(layer, name)|
                        name_mask & (1u16 << layer) != 0 && profile.layer_names.get(layer) != Some(name)));
                if differs {
                    let backup = (|| -> anyhow::Result<()> {
                        let directory = app_settings_path().parent().ok_or_else(|| anyhow::anyhow!("settings directory unavailable"))?.join("backups");
                        std::fs::create_dir_all(&directory)?;
                        let mut file = tempfile::Builder::new().prefix("sm62-default-names-").suffix(".json").tempfile_in(directory)?;
                        serde_json::to_writer_pretty(file.as_file_mut(), &previous)?;
                        file.as_file().sync_all()?;
                        file.keep()?;
                        Ok(())
                    })();
                    if let Err(error) = backup {
                        self.status_msg = format!("{}: {error}", app_layout_text(self.app_settings.language,
                            "Имена Default не сверены: не удалось сохранить резервную копию", "Default names not reconciled: backup failed"));
                        name_mask = 0;
                    }
                }
            }
            let changed = self
                .application_layout_settings_mut()
                .and_then(|settings| {
                    settings
                        .layouts
                        .get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
                })
                .is_some_and(|profile| {
                    let mut changed = reconcile_default_profile_from_device(profile, &device_layers, pending);
                    // SM62 Default is the keyboard's persistent layout, not a
                    // second host-only profile. Reconcile names just like keys,
                    // but only from successful native reads and never over a
                    // pending edit/uncertain SET. No firmware writes here.
                    if let Some(names) = device_names.as_ref() {
                        for (layer, name) in names.iter().take(16).enumerate() {
                            if name_mask & (1u16 << layer) != 0 {
                                changed |= profile.set_layer_name(layer, name.clone());
                            }
                        }
                    }
                    changed
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
        #[cfg(not(target_arch = "wasm32"))]
        self.refresh_application_layout_edit_owners(device_key);
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
    pub(crate) fn refresh_application_layout_edit_owners(&mut self, key: &str) {
        let owners: std::collections::BTreeSet<_> = self.qmk_hid_hosts.iter().filter_map(|(path, bridge)| {
            if bridge.application_layout_device_key() != Some(key)
                || !self.device_manager.devices().iter().any(|d| d.permits_hid_target(bridge.device())) { return None; }
            Some((path.clone(), bridge.application_layout_owner_incarnation(), bridge.application_layout_publication_generation()?))
        }).collect();
        if self.application_layout_edit_owners.get(key) == Some(&owners) { return; }
        // Pending deltas belong to the prior context, not to its successor.
        // Preserve the draft; consume its counter as provenance only.
        self.application_layout_observed_edits.retain(|(k, _), _| k != key);
        if let Some(settings) = self.app_settings.application_layouts.get(key) {
            self.application_layout_observed_edits.extend(settings.layouts.values()
                .map(|p| ((key.to_owned(), p.id.clone()), p.user_edit_revision)));
        }
        self.application_layout_edit_owners.insert(key.to_owned(), owners);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn sm62_rule_owner_key(&self, bridge: &crate::qmk_hid_host::QmkHidHostBridge) -> Option<String> {
        bridge.application_layout_publication_generation()?;
        let key = bridge.device().sm62_host_rule_key()?;
        let devices = self.device_manager.devices();
        if !devices.iter().any(|d| d.permits_hid_target(bridge.device()))
            || devices.iter().filter(|d| d.sm62_host_rule_key().as_ref() == Some(&key)).count() != 1 {
            return None;
        }
        Some(key)
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
        self.application_layout_owner_intents.retain(|(path, incarnation, generation), _| {
            self.qmk_hid_hosts.get(path).is_some_and(|bridge|
                bridge.application_layout_owner_incarnation() == *incarnation
                    && bridge.application_layout_publication_generation() == Some(*generation))
        });
        for key in self.app_settings.application_layouts.keys().cloned().collect::<Vec<_>>() {
            self.refresh_application_layout_edit_owners(&key);
        }
        for (key, settings) in &self.app_settings.application_layouts {
            for layout in settings.layouts.values() {
                let previous = self.application_layout_observed_edits
                    .insert((key.clone(), layout.id.clone()), layout.user_edit_revision).unwrap_or(0);
                if previous == layout.user_edit_revision { continue; }
                for (path, bridge) in &self.qmk_hid_hosts {
                    if bridge.application_layout_device_key() != Some(key.as_str()) { continue; }
                    if let Some(generation) = bridge.application_layout_publication_generation() {
                        self.application_layout_owner_intents.entry((path.clone(), bridge.application_layout_owner_incarnation(), generation))
                            .or_default().insert(layout.id.clone());
                    }
                }
            }
        }
        for bridge in self.qmk_hid_hosts.values() {
            for record in bridge.resident_recoveries() {
                let value=serde_json::json!({"version":1,"device_library":bridge.application_layout_device_key(),"owner_path":bridge.device().path,"owner_serial":bridge.device().serial_number,"record":record});
                if let Err(e)=super::app_storage::preserve_resident_recovery(&value){self.status_msg=format!("Resident100 raw recovery persistence failed: {e:#}");}
            }
        }
        let recovered_profiles: Vec<_>=self.qmk_hid_hosts.values().filter_map(|bridge| {
            let generation=bridge.application_layout_publication_generation()?;
            let key=bridge.application_layout_device_key()?.to_owned();
            Some((bridge.device().path.clone(),key,generation,self.sm62_rule_owner_key(bridge),bridge.resident_recoveries()))
        }).collect();
        for (path,key,generation,rule_key,records) in recovered_profiles {
            if self.qmk_hid_hosts.get(&path).and_then(|b|b.application_layout_publication_generation())!=Some(generation) {continue;}
            let archived_rules = rule_key.as_ref().and_then(|k| self.app_settings.sm62_host_rules.get(k)).cloned().unwrap_or_default();
            let explicit_ids = self.qmk_hid_hosts.get(&path).and_then(|b|
                self.application_layout_owner_intents.get(&(path.clone(), b.application_layout_owner_incarnation(), generation))).cloned().unwrap_or_default();
            // Stage readback in memory; the normalized whole library is persisted
            // once below, before any owner publication. Saving an intermediate
            // mapping-version-zero library would retrigger the migration backup.
            if let Some(settings)=self.app_settings.application_layouts.get_mut(&key) {
                for record in records {
                    // A cached readback is not permission to undo a pending
                    // user deletion, including records adopted under a host ID.
                    if settings.resident_deleted.iter().any(|(id, (_, deleted))| {
                        crate::qmk_hid_host::application_layout_identity_matches(id, &record.identity_crc[..20])
                            || settings.snapshot_for_layout(deleted).device_identity.as_ref()
                                .is_some_and(|identity| identity.as_slice() == &record.identity_crc[..20])
                    }) {
                        continue;
                    }
                    let Ok(mut layout)=crate::qmk_hid_host::decode_device_profile(&record) else {continue;};
                    let recovered_id=layout.id.clone();
                    // Resolve a known host ID by its forward digest, never invent
                    // matching metadata by reversing an unknown device identity.
                    let previous = settings.layouts.values().find(|p|
                        crate::qmk_hid_host::application_layout_identity_matches(&p.id,&record.identity_crc[..20]))
                        .or_else(|| settings.layouts.get(&recovered_id)).cloned();
                    let matches: Vec<_> = archived_rules.values().filter(|rule|
                        rule.identity.as_slice() == &record.identity_crc[..20] && rule.host_id != "default").collect();
                    // A live explicit edit wins over archived matching metadata.
                    // Otherwise restore only a unique rule for this half AND an
                    // actual currently validated device FULL. No absent profile
                    // or cached Default is copied into the new attachment.
                    if matches.len() == 1 && !previous.as_ref().is_some_and(|p| explicit_ids.contains(&p.id)) {
                        matches[0].restore(settings, &mut layout);
                    } else if let Some(previous)=previous.as_ref() {
                        layout.id=previous.id.clone();layout.executable=previous.executable.clone();
                        layout.application_identities=previous.application_identities.clone();
                        layout.title_contains=previous.title_contains.clone();layout.category=previous.category;
                        layout.custom_category_id=previous.custom_category_id.clone();
                        layout.automatic_switching=previous.automatic_switching;
                    }
                    if let Some(previous)=previous.as_ref().filter(|p| p.id == layout.id) {
                        // Readback changes device data, not the history of
                        // explicit host edits observed by the current owner.
                        layout.user_edit_revision=previous.user_edit_revision;
                    }
                    if recovered_id!=layout.id && settings.layouts.get(&recovered_id)
                        .is_some_and(|p|settings.snapshot_for_layout(p).device_raw.is_some()) {
                        // Retire only the generated duplicate host entry, never
                        // queue DELETE for the underlying device record.
                        settings.layouts.remove(&recovered_id);
                        settings.device_raw_profiles.remove(&recovered_id);
                        settings.resident_slots.remove(&recovered_id);
                        if settings.editor_layout_id==recovered_id {settings.editor_layout_id=layout.id.clone();}
                        if settings.active_layout_id==recovered_id {settings.active_layout_id=layout.id.clone();}
                    }
                    if settings.device_raw_profiles.get(&layout.id)==Some(&record.raw) {continue;}
                    settings.resident_slots.retain(|id,slot|*slot!=record.slot || id==&layout.id);
                    settings.resident_slots.insert(layout.id.clone(),record.slot);
                    settings.device_raw_profiles.insert(layout.id.clone(),record.raw);
                    settings.layouts.insert(layout.id.clone(),layout);
                }
            }
        }
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
            let confirmed_deletions:Vec<_>=self.app_settings.application_layouts.get(&device_key).into_iter()
                .flat_map(|s|s.resident_deleted.keys()).filter(|id|self.qmk_hid_hosts.values().any(|b|
                    b.application_layout_device_key()==Some(device_key.as_str()) && b.resident_deletion_confirmed(id)))
                .cloned().collect();
            if !confirmed_deletions.is_empty() {
                let before=self.app_settings.clone();
                let s=self.app_settings.application_layouts.get_mut(&device_key).unwrap();
                for id in confirmed_deletions {s.resident_deleted.remove(&id);}s.normalize();
                if let Err(e)=super::app_storage::try_save_app_settings(&self.app_settings) {self.app_settings=before;self.status_msg=format!("Resident100 delete persistence failed: {e:#}");continue;}
            }
            let selected = selected_key.as_ref() == Some(&device_key);
            let seed = selected
                .then(|| self.layout.as_ref().map(|layout| self.application_layer_seed(layout)))
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
            let is_sm62 = seed.as_ref().is_some_and(|layers| layers[0].len() == 62)
                || self.qmk_hid_hosts.values().any(|bridge| bridge.device().is_ergohaven_sm62()
                    && bridge.application_layout_device_key() == Some(device_key.as_str()))
                || self.app_settings.application_layouts.get(&device_key).is_some_and(|settings|
                    settings.layouts.get("default").is_some_and(|layout| layout.layers[0].len() == 62));
            let allowed_profiles = is_sm62.then(|| {
                let mut allowed = std::collections::BTreeSet::from([crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID.to_owned()]);
                if let Some(settings) = self.app_settings.application_layouts.get(&device_key) {
                    for layout in settings.layouts.values() {
                        let snapshot = settings.snapshot_for_layout(layout);
                        if self.qmk_hid_hosts.values().any(|bridge| {
                            if bridge.application_layout_device_key() != Some(device_key.as_str())
                                || !bridge.device().is_ergohaven_sm62()
                                || !self.device_manager.devices().iter().any(|device| device.permits_hid_target(bridge.device())) {
                                return false;
                            }
                            let Some(generation) = bridge.application_layout_publication_generation() else { return false; };
                            self.application_layout_owner_intents.get(&(bridge.device().path.clone(), bridge.application_layout_owner_incarnation(), generation))
                                .is_some_and(|ids| ids.contains(&layout.id))
                                || bridge.confirms_resident_snapshot_for_generation(&snapshot, generation)
                        }) {
                            allowed.insert(layout.id.clone());
                        }
                    }
                }
                allowed
            });
            let mut persist = false;
            let (active_layout_followed, active_layout_changed) = {
                let settings = self
                    .app_settings
                    .application_layouts
                    .entry(device_key.clone())
                    .or_insert_with(|| {
                        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
                        if let Some(seed) = seed.as_ref().filter(|s| s[0].len() == 62) {
                            settings.layouts.get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID).unwrap().layers = seed.to_vec();
                        }
                        settings
                    });
                persist |= settings.normalize();
                persist |= settings.provision_builtin_presets(installed_presets.as_ref().ok());
                persist |=
                    settings.enrich_application_identities(&self.application_discovery.available);
                if let Some(seed) = seed {
                    for layout in settings.layouts.values_mut() {
                        if !settings.device_raw_profiles.contains_key(&layout.id) {
                            persist |= layout.seed_unset_keycodes(seed.clone());
                        }
                    }
                }
                let resolved = if let Some(allowed) = allowed_profiles.as_ref() {
                    resolve_authorized_layout_for_runtime(settings, &foreground_status.state, manual_override_active, allowed)
                } else {
                    resolve_layout_for_runtime(settings, &foreground_status.state, manual_override_active)
                };
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
            // Reconcile persistent Default before constructing the first runtime snapshot.
            if selected {
                self.maybe_sync_default_application_layout_to_device();
            }
            let settings=&self.app_settings.application_layouts[&device_key];
            let mut library:Vec<_>=settings.resident_deleted.values().map(|(slot,p)|{
                let mut s=settings.snapshot_for_layout(p);s.resident_slot=Some(*slot);s.active=false;s
            }).collect();
            library.extend(settings.layouts.values()
                .filter(|p|p.id!=crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
                .filter(|p|p.layers.len()==16 && p.layers.iter().all(|l|l.len()==62))
                .map(|p|settings.snapshot_for_layout(p))
                .filter(|p|p.layers.iter().all(|l|l.len()==62 && (p.device_identity.is_some() || !l.contains(&crate::application_layouts::APPLICATION_LAYOUT_UNSET_KEYCODE))) && p.encoder_stacks.iter().all(|s|s.is_empty() || (2..=4).contains(&s.len()))));
            let snapshot = self.app_settings.application_layouts[&device_key]
                .active_layout()
                .map(|layout| {
                    let prepared=self.app_settings.application_layouts[&device_key].snapshot_for_layout(layout);
                    // Ownership and semantic identity are separate: background
                    // Default releases the overlay, but is still Default.
                    if (layout.layers[0].len()==62 || connected_selected_key.as_ref() != Some(&device_key))
                        && layout.id == crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID
                    {
                        crate::application_layouts::ApplicationLayoutSnapshot::firmware_default()
                    } else if layout.layers[0].len()==62 && prepared.device_identity.is_none() && layout.layers.iter().any(|l|l.contains(&crate::application_layouts::APPLICATION_LAYOUT_UNSET_KEYCODE)) {
                        crate::application_layouts::ApplicationLayoutSnapshot::firmware_default()
                    } else {
                        prepared
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
            // Archive only matching rules with explicit current-owner intent or
            // verified device provenance. This does not authorize publication.
            let rule_updates: Vec<_> = self.qmk_hid_hosts.values().filter_map(|bridge| {
                if bridge.application_layout_device_key() != Some(device_key.as_str()) { return None; }
                let stable_key = self.sm62_rule_owner_key(bridge)?;
                let generation = bridge.application_layout_publication_generation()?;
                let intents = self.application_layout_owner_intents.get(&(bridge.device().path.clone(), bridge.application_layout_owner_incarnation(), generation));
                let settings = &self.app_settings.application_layouts[&device_key];
                let rules: Vec<_> = settings.layouts.values().filter(|p| p.id != "default").filter_map(|p| {
                    let snapshot = settings.snapshot_for_layout(p);
                    if !intents.is_some_and(|ids| ids.contains(&p.id))
                        && !bridge.confirms_resident_snapshot_for_generation(&snapshot, generation) { return None; }
                    let identity = snapshot.device_identity.unwrap_or_else(|| crate::qmk_hid_host::application_layout_identity_digest(&p.id));
                    Some(crate::application_layouts::Sm62HostRule::capture(settings, p, identity))
                }).collect();
                let deleted: Vec<_> = settings.resident_deleted.iter().map(|(id, (_, p))| {
                    (id.clone(), settings.snapshot_for_layout(p).device_identity)
                }).collect();
                Some((stable_key, rules, deleted))
            }).collect();
            for (stable_key, rules, deleted) in rule_updates {
                let archive = self.app_settings.sm62_host_rules.entry(stable_key).or_default();
                let before = archive.clone();
                archive.retain(|id, rule| !deleted.iter().any(|(deleted_id, identity)|
                    id == deleted_id || identity.as_ref() == Some(&rule.identity)));
                for rule in rules {
                    archive.retain(|id, old| old.identity != rule.identity || id == &rule.host_id);
                    archive.insert(rule.host_id.clone(), rule);
                }
                persist |= *archive != before;
            }
            let durable_library=self.app_settings.application_layouts[&device_key].layouts.get("default")
                .is_some_and(|p|p.layers[0].len()==62);
            let persistence_current=durable_library && self.persisted_application_libraries.get(&device_key)
                .is_some_and(|proof|proof.matches(&self.app_settings.application_layouts[&device_key]));
            if persist || (durable_library && !persistence_current) {
                if let Err(e)=super::app_storage::try_save_app_settings(&self.app_settings) {
                    self.persisted_application_libraries.remove(&device_key);
                    self.status_msg=format!("Resident100 settings persistence failed: {e:#}");continue;
                }
                if durable_library {
                    if let Some(proof)=PersistedApplicationLibrary::capture(&device_key,&self.app_settings.application_layouts[&device_key]) {
                        self.persisted_application_libraries.insert(device_key.clone(),proof);
                    } else {
                        self.persisted_application_libraries.remove(&device_key);
                        self.status_msg="Resident100 settings persistence could not be revalidated".into();
                        continue;
                    }
                }
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
                    let authorized = |profile: &crate::application_layouts::ApplicationLayoutSnapshot| {
                        // R1 gates publication, not the existing tombstone lifecycle.
                        // Owner-scoped deletion authorization is the separate R2 seam.
                        !profile.active || profile.profile_id.as_ref().is_some_and(|id| {
                            self.application_layout_owner_intents.get(&(bridge.device().path.clone(), bridge.application_layout_owner_incarnation(), *generation))
                                .is_some_and(|ids| ids.contains(id))
                                || bridge.confirms_resident_snapshot_for_generation(profile, *generation)
                        })
                    };
                    let mut owner_snapshot = snapshot.clone();
                    if bridge.device().is_ergohaven_sm62() {
                        let owner_library = library.iter().filter(|p| authorized(p)).cloned().collect();
                        bridge.publish_resident_library_for_generation(owner_library,*generation);
                        if owner_snapshot.active && !authorized(&owner_snapshot) {
                            owner_snapshot = crate::application_layouts::ApplicationLayoutSnapshot::firmware_default();
                        }
                    }
                    bridge.publish_application_layout_snapshot_for_generation(
                        owner_snapshot,
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
            .and_then(|settings| settings.active_layout().map(|layout| settings.snapshot_for_layout(layout)))
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

fn resolve_authorized_layout_for_runtime(
    settings: &crate::application_layouts::DeviceApplicationLayouts,
    foreground: &crate::app_discovery::ForegroundState,
    manual_override_active: bool,
    allowed: &std::collections::BTreeSet<String>,
) -> String {
    if manual_override_active || matches!(foreground, crate::app_discovery::ForegroundState::BackendUnavailable(_)) {
        return if allowed.contains(&settings.active_layout_id) {
            settings.active_layout_id.clone()
        } else {
            crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID.to_owned()
        };
    }
    match foreground {
        crate::app_discovery::ForegroundState::Focused(application) => settings.resolve_authorized(Some(application), Some(allowed)),
        _ => settings.resolve_authorized(None, Some(allowed)),
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
    include!("runtime_read_first_fixture.inc");
    include!("host_lease_runtime_regression.rs");
    include!("foreground_wake_regression.rs");
    include!("latest_base_composition_tests.rs");
    include!("final_product_ui_tests.rs");
    include!("matrix_runtime_regression.rs");
    include!("multi_sm62_regression.rs");

    #[test]
    fn sm62_every_matrix_control_survives_reordered_ui_and_default_snapshot() {
        let mut layout = indicator_test_layout();
        layout.name = "SM62".into();
        layout.rows = 10;
        layout.cols = 7;
        let positions = [
            (0,5),(0,4),(0,3),(0,2),(0,1),(0,0),(5,0),(5,1),(5,2),(5,3),(5,4),(5,5),
            (0,6),(1,5),(1,4),(1,3),(1,2),(1,1),(1,0),(6,0),(6,1),(6,2),(6,3),(6,4),(6,5),
            (5,6),(1,6),(2,5),(2,4),(2,3),(2,2),(2,1),(2,0),(7,0),(7,1),(7,2),(7,3),(7,4),(7,5),
            (6,6),(2,6),(3,5),(3,4),(3,3),(3,2),(3,1),(3,0),(8,0),(8,1),(8,2),(8,3),(8,4),(8,5),
            (7,6),(4,3),(4,2),(4,1),(4,0),(9,0),(9,1),(9,2),(9,3),
        ];
        let key = layout.keys[0].clone();
        layout.keys = positions.iter().rev().map(|&(row,col)| PhysicalKey { row,col,..key.clone() }).collect();
        layout.layers = (0..16).map(|l| (0..62).rev().map(|k| crate::keyboard::KeyBinding::Vial(4+l*62+k)).collect()).collect();
        let base = EntropyApp::base_application_layers(&layout);
        assert_eq!(base[0].len(),62);
        for l in 0..16 {
            let snapshot = EntropyApp::default_application_layer_snapshot(&layout,l,&base[l]);
            assert_eq!(snapshot.keycodes,layout.layers[l]);
            for k in 0..62 {
                assert_eq!(EntropyApp::application_control_for_key(&layout,61-k),Some(k));
                assert_eq!(base[l][k],4+(l*62+k) as u16);
            }
        }
        layout.keys[0].row = 4; layout.keys[0].col = 6;
        assert_eq!(EntropyApp::application_control_for_key(&layout,0),None);
    }



    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn application_layout_runtime_stays_off_without_a_layout_device_accepted_composition() {
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
    fn application_layout_runtime_follows_a_connected_macropad_accepted_composition() {
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

    fn sm62_test_layout() -> KeyboardLayout {
        let mut layout = indicator_test_layout();
        layout.name = "SM62".into(); layout.rows = 10; layout.cols = 7;
        let key = layout.keys[0].clone();
        layout.keys = crate::application_layouts::SM62_APPLICATION_POSITIONS.iter()
            .map(|&(row,col)| PhysicalKey { row,col, ..key.clone() }).collect();
        layout.layers = vec![vec![crate::keyboard::KeyBinding::Vial(4);62];16];
        layout
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_usb_role_change_keeps_browser_profile_and_sends_to_new_selected_owner() {
        sm62_read_first_usb_role_change(false);
    }

    include!("manual_role_probe_tests.inc");
    include!("new_keyboard_authority_tests.inc");
    include!("device_inventory_display_tests.inc");
    include!("catalog_restore_regression.inc");
    include!("resident_catalog_identity_tests.inc");
    include!("review_resident_regressions.inc");
    include!("r1_publication_intent_tests.inc");
    include!("r1_readback_authorship_tests.inc");
    include!("r1_pending_owner_tests.inc");
    include!("r1_direct_action_tests.inc");
    include!("r1_direct_control_tests.inc");
    include!("r2_same_half_rules_tests.inc");
    include!("resident_catalog_production_c_tests.inc");
    include!("resident_catalog_cold_c_tests.inc");

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
    fn background_macropad_autoswitch_survives_k04_selection() { background_autoswitch_survives_k04_selection(false); }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_background_autoswitch_shared_owner_handoff_survives_k04_selection() { background_autoswitch_survives_k04_selection(true); }

    #[cfg(not(target_arch = "wasm32"))]
    fn background_autoswitch_survives_k04_selection(sm62: bool) {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for (select_k04, serial) in [(false, ""), (true, ""), (true, "indicator-test")] {
            let mut app = EntropyApp::new_inert_for_test();
            let mut macropad = m4cr0pad_v3_device();
            if sm62 { macropad.product_id=0x00b2; macropad.name="SM62".into(); }
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
            app.layout = Some(if sm62 { sm62_test_layout() } else { indicator_test_layout() });
            if !sm62 { mark_application_layout_protocol_supported(&mut app); }
            let (macro_hid, macro_packets) = if sm62 {
                let (hid,rec,_)=runtime_fixture_hid(Vec::new()); (hid,rec)
            } else { crate::hid::HidDevice::test_device() };
            if !sm62 { macro_packets.respond_by(move |request| {
                if request[0] == 0xeb { return Some(Ok(*request)); }
                if request[0] == 0xe6 {
                    let mut response = [0; 32];
                    response[..3].copy_from_slice(&[
                        0xe6,
                        if sm62 {8} else {7},
                        0x5a,
                    ]);
                    Some(Ok(response))
                } else {
                    None
                }
            }); }
            let (k04_hid, k04_packets) = crate::hid::HidDevice::test_device();
            if !sm62 {
                let bridge = crate::qmk_hid_host::test_start_profile_handoff_bridge(
                    macropad.clone(), macro_hid.shared_output());
                bridge.set_selected_application_layout_support(true);
                app.qmk_hid_hosts.insert(macropad.path.clone(), bridge);
            }
            if sm62 {
                // Complete the real native read/connect/owner boundary before
                // the first GUI authoring action, not after handoff.
                runtime_fixture_connect(&mut app, &macropad, macro_hid);
            } else {
                app.shared_hid_output = macro_hid.shared_output();
                app.hid_device = Some(macro_hid.into());
                app.remember_connected_application_layout_device();
            }
            // poll_connect reconciles background services. Install the synthetic
            // K04 selected owner AFTER that boundary, otherwise its pre-connect
            // Selected(true) service is replaced by a Discover service and the
            // later exact-retention assertion tests a stale synthetic owner.
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
            assert!(app.qmk_hid_hosts[&macropad.path].matches_shared_output(app.shared_hid_output.as_ref()));
            let connected_owner = app.qmk_hid_hosts[&macropad.path].application_layout_owner_incarnation();
            let key = app.application_layout_device_key().unwrap();
            let foreground = crate::application_layouts::DetectedApplication {
                executable: "routing-editor".into(),
                display_name: "Routing editor".into(),
                ..Default::default()
            };
            let settings = app.application_layout_settings_mut().unwrap();
            let profile = settings.create_for_application(&foreground);
            settings
                .layouts
                .get_mut(&profile)
                .unwrap()
                .set_keycode(0, 0, 0x0005);
            assert!(app.activate_application_layout(&profile));
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
                assert_eq!(app.qmk_hid_hosts[&macropad.path].application_layout_owner_incarnation(), connected_owner,
                    "connect-created owner was replaced during K04 selection");
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
            let mut expected = crate::application_layouts::ApplicationLayoutSnapshot::from_layout(
                app.app_settings.application_layouts[&key]
                    .active_layout()
                    .unwrap(),
            );
            if macropad.is_ergohaven_sm62() {
                // Application mappings are stable, not fixed Firefox/user roles.
                expected.resident_slot = app.app_settings.application_layouts[&key].resident_slots.get(&profile).copied();
            }
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
        cold_background_discovers_binding_with_k04_selected();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_cold_background_discovers_binding_with_k04_selected() {
        sm62_read_first_cold_background_with_k04_selected();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn sm62_read_first_cold_background_with_k04_selected() {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        let mut app=EntropyApp::new_inert_for_test();
        let mut device=m4cr0pad_v3_device();device.name="SM62".into();device.product_id=0x00b2;
        device.path=format!("{}/src/main.rs",env!("CARGO_MANIFEST_DIR"));
        device.serial_number="C281FE0DD04A01230000000000000000".into();
        device.instance_token="cold-background-first-attachment".into();
        let mut k04=device.clone();k04.name="K:04".into();k04.product_id=0x0071;
        k04.path=format!("{}/src/app.rs",env!("CARGO_MANIFEST_DIR"));
        k04.serial_number="k04".into();k04.instance_token="cold-k04-enumeration".into();
        app.device_manager.replace_devices(vec![device.clone(),k04.clone()]);
        app.selected_device=Some(1);app.current_keyboard_id=Some(4);
        app.layout=Some(indicator_test_layout());app.selected_layer=3;app.selected_key=Some((1,0));
        let original=app.layout.as_ref().unwrap().layers.clone();
        let (k04_hid,k04_packets)=crate::hid::HidDevice::test_device();
        app.qmk_hid_hosts.insert(k04.path.clone(),crate::qmk_hid_host::test_start_bridge(
            k04.clone(),Default::default(),k04_hid.shared_output(),None,
            crate::qmk_hid_host::HostProtocol::Selected(true),||None));
        let key=device.application_layout_storage_key("vial-0000000000000042".into());
        // Empty per-enumeration destination with the discovered device geometry;
        // no host application payload, ready flag, or publication grant is seeded.
        app.app_settings.application_layouts.entry(key.clone()).or_default()
            .layouts.get_mut("default").unwrap().layers=vec![vec![0xffff;62];16];
        let (id,raw)=runtime_fixture_builtin_record("firefox","Cold own browser",5);
        let (hid,packets,_)=runtime_fixture_hid(vec![(7,raw.clone())]);
        app.qmk_hid_hosts.insert(device.path.clone(),crate::qmk_hid_host::test_start_bridge(
            device.clone(),Default::default(),None,Some(hid),crate::qmk_hid_host::HostProtocol::Discover,||None));
        let id = runtime_fixture_adopt(&mut app,&device.path,&key,&id,&raw,&packets);
        assert_eq!(app.qmk_hid_hosts[&device.path].application_layout_device_key(),Some(key.as_str()));
        assert_eq!(app.app_settings.application_layouts[&key].layouts[&id].layers[15][61],5);
        let settings=app.app_settings.application_layouts.get_mut(&key).unwrap();
        settings.pending_builtin_auto_bind.remove(&id);
        settings.layouts.get_mut(&id).unwrap().automatic_switching=false;
        settings.layouts.get_mut(&id).unwrap().executable="firefox".into();
        assert!(!settings.layouts[&id].automatic_switching);
        assert!(!packets.requests().iter().any(|p|p[..3]==[0xef,9,2]));
        // Explicit per-profile matching checkbox after confirmed background READ.
        app.app_settings.application_layouts.get_mut(&key).unwrap().layouts.get_mut(&id).unwrap().automatic_switching=true;
        let foreground=crate::application_layouts::DetectedApplication {executable:"firefox".into(),display_name:"Cold browser".into(),..Default::default()};
        let discovery=crate::app_discovery::ApplicationDiscoverySnapshot {
            foreground:Some(foreground.clone()),
            foreground_status:crate::app_discovery::ForegroundStatus {backend:"cold background OS boundary".into(),state:crate::app_discovery::ForegroundState::Focused(foreground)},..Default::default()
        };
        let cursor=packets.requests().len();
        app.update_application_layout_runtime_with(discovery.clone());
        assert_eq!(app.app_settings.application_layouts[&key].active_layout_id,id);
        runtime_fixture_wait_select(&packets,cursor,&raw);
        app.app_settings=crate::app::app_storage::load_app_settings();
        assert_eq!(app.app_settings.application_layout_bindings[&key],(device.vendor_id,device.product_id,device.serial_number.clone()));
        let archived=serde_json::to_value(&app.app_settings.application_layouts[&key].layouts).unwrap();
        // A new enumeration has its own namespace and must read its own content.
        // Same half, different unit, wrong UID/product, alias and unknown unit
        // are separate controls; none may inherit the old payload or deletion set.
        for mode in 0..6 {
            app.qmk_hid_hosts.remove(&device.path);
            let mut fresh=device.clone();fresh.instance_token=format!("cold-successor-{mode}");
            let mut uid=0x42;
            if mode==1 {fresh.serial_number="C281FE0DD04A04560000000000000000".into();}
            if mode==2 {uid=0x99;}
            if mode==3 {fresh.product_id=0x0041;}
            if mode==5 {fresh.serial_number.clear();}
            let mut devices=vec![fresh.clone(),k04.clone()];
            if mode==4 || mode==5 {
                let mut alias=fresh.clone();alias.path=format!("{}/src/device.rs",env!("CARGO_MANIFEST_DIR"));
                devices.push(alias);
            }
            app.device_manager.replace_devices(devices);
            let new_key=fresh.application_layout_storage_key(format!("vial-{uid:016x}"));
            assert_ne!(new_key,key);
            if mode<=1 {
                app.app_settings.application_layouts.entry(new_key.clone()).or_default()
                    .layouts.get_mut("default").unwrap().layers=vec![vec![0xffff;62];16];
            }
            let (_,own_raw)=runtime_fixture_builtin_record("firefox",&format!("Successor {mode}"),20+mode);
            let (hid,rec,_)=runtime_fixture_hid_identity(vec![(7,own_raw.clone())],uid,8);
            app.qmk_hid_hosts.insert(fresh.path.clone(),crate::qmk_hid_host::test_start_bridge(
                fresh.clone(),Default::default(),None,Some(hid),crate::qmk_hid_host::HostProtocol::Discover,||None));
            if mode<=1 {
                let id = runtime_fixture_adopt(&mut app,&fresh.path,&new_key,&id,&own_raw,&rec);
                assert_eq!(app.qmk_hid_hosts[&fresh.path].application_layout_device_key(),Some(new_key.as_str()));
                assert_ne!(app.app_settings.application_layouts[&new_key].device_raw_profiles[&id],raw);
                assert_eq!(app.app_settings.application_layouts[&new_key].layouts[&id].layers[15][61],20+mode);
                if mode==1 {
                    let settings=app.app_settings.application_layouts.get_mut(&new_key).unwrap();
                    settings.pending_builtin_auto_bind.remove(&id);
                    settings.layouts.get_mut(&id).unwrap().automatic_switching=false;
                    assert!(!settings.layouts[&id].automatic_switching);
                    assert_eq!(settings.layouts[&id].name,format!("Successor {mode}"),
                        "different physical unit inherited the previous profile binding");
                }
                let cursor=rec.requests().len();
                let profile=app.app_settings.application_layouts.get_mut(&new_key).unwrap().layouts.get_mut(&id).unwrap();
                profile.executable="firefox".into();
                profile.automatic_switching=true;
                app.update_application_layout_runtime_with(discovery.clone());
                runtime_fixture_wait_select(&rec,cursor,&own_raw);
                let mut alias=fresh.clone();alias.path=format!("{}/src/device.rs",env!("CARGO_MANIFEST_DIR"));
                let mut devices=app.device_manager.devices().to_vec();devices.push(alias);
                let cursor=rec.requests().len();
                app.device_manager.replace_devices(devices);app.restore_background_application_layout_bindings();
                assert!(app.qmk_hid_hosts[&fresh.path].application_layout_device_key().is_none(),"late alias did not revoke the owner");
                app.update_application_layout_runtime_with(discovery.clone());
                std::thread::sleep(std::time::Duration::from_millis(30));
                assert!(!rec.requests()[cursor..].iter().any(|p|p[..2]==[0xef,9]&&matches!(p[2],1|2|4)||matches!(p[0],0xe1..=0xe5|0xe7|0xe9)));
            } else {
                let deadline=std::time::Instant::now()+std::time::Duration::from_millis(150);
                while std::time::Instant::now()<deadline {
                    app.update_application_layout_runtime_with(discovery.clone());
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                assert!(app.qmk_hid_hosts[&fresh.path].application_layout_device_key().is_none(),"unsafe cold successor mode {mode} borrowed a library");
                assert_eq!(app.qmk_hid_hosts[&fresh.path].test_application_layout_snapshot(),crate::application_layouts::ApplicationLayoutSnapshot::inactive());
                assert!(!rec.requests().iter().any(|p|p[..2]==[0xef,9]&&matches!(p[2],1|2|4)||matches!(p[0],0xe1..=0xe5|0xe7|0xe9)));
            }
            assert_eq!(serde_json::to_value(&app.app_settings.application_layouts[&key].layouts).unwrap(),archived,"successor changed the old recoverable library");
            assert!(rec.requests().iter().all(|p|p[0]!=5&&p[..2]!=[0xfe,0x0b]),"cold successor overwrote native Default");
            assert_eq!(app.selected_device,Some(1));assert_eq!(app.selected_layer,3);assert_eq!(app.selected_key,Some((1,0)));
            assert_eq!(app.layout.as_ref().unwrap().layers,original);
            assert!(k04_packets.requests().is_empty(),"cold runtime wrote to K04");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn cold_background_discovers_binding_with_k04_selected() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let mut app = EntropyApp::new_inert_for_test();
        let mut macropad = m4cr0pad_v3_device();
        macropad.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
        let mut k04 = macropad.clone();
        k04.product_id = 0x0071;
        k04.path = format!("{}/src/app.rs", env!("CARGO_MANIFEST_DIR"));
        k04.serial_number = "k04".into();
        k04.instance_token = "k04-distinct-enumeration".into();
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
        packets.respond_by(move |request| {
            if request[0] == 0xeb { return Some(Ok(*request)); }
            if request[0..2] == [0xfe, 0] {
                let mut response = [0; 32];
                response[0..4].copy_from_slice(&6u32.to_le_bytes());
                response[4..12].copy_from_slice(&0x42u64.to_le_bytes());
                Some(Ok(response))
            } else if request[0] == 0xe6 {
                let mut response = [0; 32];
                response[..3].copy_from_slice(&[
                    0xe6,
                    7,
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
        let original_library = serde_json::to_value(&app.app_settings.application_layouts[key].layouts).unwrap();
        for (serial, uid, product_id, duplicate, should_bind) in [
            ("indicator-test", 0x42u64, 0x42, false, true),
            // Macropad keeps its existing serial-scoped policy.
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
                if request[0] == 0xeb { return Some(Ok(*request)); }
                if request[0..2] == [0xfe, 0] {
                    let mut response = [0; 32];
                    response[..4].copy_from_slice(&6u32.to_le_bytes());
                    response[4..12].copy_from_slice(&uid.to_le_bytes());
                    Some(Ok(response))
                } else if request[0] == 0xe6 {
                    let mut response = [0; 32];
                    response[..3].copy_from_slice(&[
                        0xe6,
                        7,
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
            assert_eq!(serde_json::to_value(&app.app_settings.application_layouts[key].layouts).unwrap(), original_library,
                "USB-owner replacement changed or duplicated the common profile library");
            assert!(packets.requests().iter().all(|r| r[0] != 0x05 && r[..2] != [0xfe, 0x0b]),
                "background USB-role replacement must not overwrite native Default assignments/names");
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
        assert_eq!(Category::ALL.len(), 8);
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

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_bridge_rejects_foreign_profile_geometry_without_v7_publication() {
        let mut device=m4cr0pad_v3_device();device.product_id=0x00b2;
        let mut bridge=crate::qmk_hid_host::QmkHidHostBridge::test_inert(device,Default::default(),None,
            crate::qmk_hid_host::HostProtocol::Discover);
        bridge.bind_selected_application_layout_device("sm62-geometry-fence".into());
        let generation=bridge.application_layout_publication_generation().unwrap();
        let settings=crate::application_layouts::DeviceApplicationLayouts::default();
        let profile=&settings.layouts["default"];
        assert!(!bridge.publish_application_layout_snapshot_for_generation(crate::application_layouts::ApplicationLayoutSnapshot::from_layout(profile),generation));
        assert!(!bridge.test_application_layout_snapshot().active);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_bridge_default_fallback_keepalive_and_shutdown_never_send_v7() {
        let mut device = m4cr0pad_v3_device(); device.product_id = 0x00b2; device.name = "SM62".into();
        device.path = format!("{}/src/main.rs",env!("CARGO_MANIFEST_DIR"));
        let (hid,recorder) = crate::hid::HidDevice::test_device();
        recorder.respond_by(|request| {
            if request[0] == 0xeb { return Some(Ok(*request)); }
            if request[0] == 0xe6 { let mut r=[0;32]; r[..3].copy_from_slice(&[0xe6,8,0x5a]); Some(Ok(r)) }
            else { None }
        });
        let mut bridge = crate::qmk_hid_host::test_start_bridge(device,Default::default(),hid.shared_output(),None,
            crate::qmk_hid_host::HostProtocol::Selected(true),||None);
        bridge.set_selected_application_layout_support(true);
        bridge.bind_selected_application_layout_device("vial-test-sm62".into());
        bridge.set_application_layout_snapshot(crate::application_layouts::ApplicationLayoutSnapshot::firmware_default());
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(4);
        while !recorder.requests().iter().any(|r|r[0]==0xe5) {
            assert!(std::time::Instant::now()<deadline); std::thread::sleep(std::time::Duration::from_millis(10));
        }
        drop(bridge);
        while !recorder.requests().iter().any(|r|r[0]==0xea) {
            assert!(std::time::Instant::now()<deadline,"shutdown did not deactivate");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let app_packets: Vec<_> = recorder.requests().into_iter().filter(|r|matches!(r[0],0xe1..=0xea)).collect();
        assert!(app_packets.iter().any(|r|r[0]==0xea));
        assert!(app_packets.iter().all(|r|r[1]==8),"SM62 fallback or shutdown used Macropad version");
    }

    #[test]
    fn resident_stack_ui_runtime_publishes_complete_profiles_and_retains_delete_intent() {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        let mut app=name_review_device_app(true);
        let key=app.application_layout_device_key().unwrap();
        let device=app.device_manager.devices()[0].clone();
        let mut bridge=crate::qmk_hid_host::QmkHidHostBridge::test_inert(device.clone(),Default::default(),None,crate::qmk_hid_host::HostProtocol::Selected(false));
        bridge.bind_selected_application_layout_device(key.clone());
        bridge.set_selected_application_layout_support(true);
        app.qmk_hid_hosts.insert(device.path.clone(),bridge);
        let s=app.application_layout_settings_mut().unwrap();
        let id=s.create_for_application(&crate::application_layouts::DetectedApplication{executable:"stack-runtime".into(),..Default::default()});
        let p=s.layouts.get_mut(&id).unwrap();
        p.automatic_switching=false;
        for l in 0..16 {
            p.layers[l]=vec![4;62];
            p.encoder_stacks[l]=(0..2+l%3).map(|i|crate::application_layouts::EncoderStackAction{
                name:format!("Action {i}"),counter_clockwise:4+i as u16,clockwise:12+i as u16,
            }).collect();
        }
        let stacks=p.encoder_stacks.clone();
        app.update_application_layout_runtime_with(Default::default());
        assert!(app.qmk_hid_hosts[&device.path].resident_sync_status(&id).is_some(),"complete stack profile silently omitted from ordinary library");
        app.app_settings.application_layouts.get_mut(&key).unwrap().remove(&id);
        app.update_application_layout_runtime_with(Default::default());
        assert!(app.qmk_hid_hosts[&device.path].resident_sync_status(&id).is_some(),"stack-bearing persisted deletion rejected");
        let saved=super::super::app_storage::load_app_settings();
        assert_eq!(saved.application_layouts[&key].resident_deleted[&id].1.encoder_stacks,stacks);
    }

    #[test]
    fn resident100_ui_runtime_publishes_disabled_inactive_profiles_but_not_unready_inheritance() {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();let mut app=name_review_device_app(true);
        let key=app.application_layout_device_key().unwrap();
        let device=app.device_manager.devices()[0].clone();
        let mut bridge=crate::qmk_hid_host::QmkHidHostBridge::test_inert(device.clone(),Default::default(),None,crate::qmk_hid_host::HostProtocol::Selected(false));
        bridge.bind_selected_application_layout_device(key.clone());bridge.set_selected_application_layout_support(true);app.qmk_hid_hosts.insert(device.path.clone(),bridge);
        let s=app.application_layout_settings_mut().unwrap();
        for i in 0..100 {let id=s.create_for_application(&crate::application_layouts::DetectedApplication{executable:format!("ui-bg-{i}"),..Default::default()});s.layouts.get_mut(&id).unwrap().automatic_switching=false;}
        let id=s.layouts.keys().find(|id|id.starts_with("ui_bg")||id.starts_with("ui-bg")).cloned().unwrap_or_else(||s.layouts.keys().find(|id|id.as_str()!="default").unwrap().clone());
        app.update_application_layout_runtime_with(Default::default());
        assert!(app.qmk_hid_hosts[&device.path].resident_sync_status(&id).is_some(),"ordinary runtime must publish inactive disabled apps");
        app.app_settings.application_layouts.get_mut(&key).unwrap().layouts.get_mut(&id).unwrap().layers[15][61]=0xffff;
        app.deferred_device_load.set_layer_status(15,DeferredLoadStatus::NotLoaded);
        app.update_application_layout_runtime_with(Default::default());
        assert_eq!(app.qmk_hid_hosts[&device.path].resident_sync_status(&id),None,"unconfirmed inherited Default must not freeze FULL");
        app.deferred_device_load.set_layer_status(15,DeferredLoadStatus::Loaded);
        app.update_application_layout_runtime_with(Default::default());
        assert!(app.qmk_hid_hosts[&device.path].resident_sync_status(&id).is_some());
        app.app_settings.application_layouts.get_mut(&key).unwrap().remove(&id);
        app.update_application_layout_runtime_with(Default::default());
        assert!(app.qmk_hid_hosts[&device.path].resident_sync_status(&id).is_some(),"UI must publish persisted tombstone intent until device confirmation");
        app.qmk_hid_hosts[&device.path].test_add_resident_recovery();
        app.update_application_layout_runtime_with(Default::default());
        let recovery=super::super::app_storage::app_settings_path().with_file_name("resident100-recovery");
        assert!(recovery.exists(),"raw recoveries must be persisted atomically by ordinary UI runtime");
        let paths:Vec<_>=std::fs::read_dir(recovery).unwrap().map(|p|p.unwrap().path()).collect();assert_eq!(paths.len(),1);
        let saved:serde_json::Value=serde_json::from_slice(&std::fs::read(&paths[0]).unwrap()).unwrap();assert_eq!(saved["record"]["raw"],serde_json::json!([1,2,3]));
    }

    #[test]
    fn resident100_failed_mapping_backup_cannot_publish_on_later_unchanged_tick() {
        let _scope=crate::app::app_storage::ScopedTempConfig::new();let mut app=name_review_device_app(true);let key=app.application_layout_device_key().unwrap();let device=app.device_manager.devices()[0].clone();
        let s=app.app_settings.application_layouts.get_mut(&key).unwrap();let id=s.create_for_application(&crate::application_layouts::DetectedApplication{executable:"backup-repaint".into(),..Default::default()});s.resident_mapping_version=0;s.resident_slots.clear();
        let path=super::super::app_storage::app_settings_path();let original=serde_json::to_vec(&app.app_settings).unwrap();std::fs::write(&path,&original).unwrap();std::fs::create_dir(path.with_extension("resident100-backup.json")).unwrap();
        let mut bridge=crate::qmk_hid_host::QmkHidHostBridge::test_inert(device.clone(),Default::default(),None,crate::qmk_hid_host::HostProtocol::Selected(false));bridge.bind_selected_application_layout_device(key.clone());bridge.set_selected_application_layout_support(true);app.qmk_hid_hosts.insert(device.path.clone(),bridge);
        app.update_application_layout_runtime_with(Default::default());app.update_application_layout_runtime_with(Default::default());
        assert_eq!(app.qmk_hid_hosts[&device.path].resident_sync_status(&id),None,"unchanged repaint must not bypass failed durable mapping migration");assert_eq!(std::fs::read(path).unwrap(),original);
    }

    #[test]
    fn sm62_cold_profile_initialization_waits_for_each_confirmed_layer() {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        let mut app=name_review_device_app(true);
        let key=app.application_layout_device_key().unwrap();
        app.app_settings.application_layouts.remove(&key);
        app.layout.as_mut().unwrap().layers=vec![vec![crate::keyboard::KeyBinding::Vial(0);62];16];
        for layer in 0..16 { app.deferred_device_load.set_layer_status(layer,DeferredLoadStatus::NotLoaded); }
        app.update_application_layout_runtime_with(Default::default());
        assert!(app.application_layout_settings().unwrap().layouts.values().all(|p|p.layers.iter().flatten().all(|&c|c==0xffff)),"cold placeholder zeros leaked into host profiles");
        let before=app.application_layout_settings().unwrap().clone();
        app.update_application_layout_runtime_with(Default::default());
        assert_eq!(&before,app.application_layout_settings().unwrap(),"unchanged cold repaint bumped profile revisions");
        app.layout.as_mut().unwrap().layers[15].fill(crate::keyboard::KeyBinding::Vial(9));
        app.deferred_device_load.set_layer_status(15,DeferredLoadStatus::Loaded);
        app.update_application_layout_runtime_with(Default::default());
        for p in app.application_layout_settings().unwrap().layouts.values() {
            assert_eq!(p.layers[15],vec![9;62]);
            assert_eq!(p.layers[0],vec![0xffff;62]);
        }
        let (hid,recorder)=crate::hid::HidDevice::test_device();app.hid_device=Some(hid.into());
        for layer in [0,7] { app.deferred_device_load.set_layer_status(layer,DeferredLoadStatus::Failed("injected failed cold read".into())); }
        let before=app.application_layout_settings().unwrap().clone();
        for _ in 0..4 { app.update_application_layout_runtime_with(Default::default()); }
        assert_eq!(app.application_layout_settings().unwrap(),&before);
        assert!(recorder.requests().is_empty(),"cold/failed repaint issued HID mutations");
        // A failed layer's zero-filled buffer must remain UNSET; a subsequently
        // confirmed external Vial edit seeds only that layer, once.
        app.layout.as_mut().unwrap().layers[7].fill(crate::keyboard::KeyBinding::Vial(12));
        app.deferred_device_load.set_layer_status(7,DeferredLoadStatus::Loaded);
        app.update_application_layout_runtime_with(Default::default());
        for profile in app.application_layout_settings().unwrap().layouts.values() {
            assert_eq!(profile.layers[7],vec![12;62]);
            assert_eq!(profile.layers[0],vec![0xffff;62]);
            assert_eq!(profile.layers[15],vec![9;62]);
        }
        let before=app.application_layout_settings().unwrap().clone();
        for _ in 0..4 { app.update_application_layout_runtime_with(Default::default()); }
        assert_eq!(app.application_layout_settings().unwrap(),&before);
        assert!(recorder.requests().is_empty());
    }

    #[test]
    fn sm62_background_initialization_provisions_full_default_derived_profiles() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let mut app = EntropyApp::new_inert_for_test();
        let mut device = m4cr0pad_v3_device(); device.product_id = 0x00b2; device.name = "SM62".into();
        app.device_manager.replace_devices(vec![device]); app.selected_device = Some(0);
        app.current_keyboard_id = Some(0x42); app.layout = Some(sm62_test_layout());
        app.deferred_device_load = DeferredDeviceLoadState::complete(16);
        app.layer_names = crate::application_layouts::default_layer_names();
        mark_application_layout_protocol_supported(&mut app);
        let key = app.application_layout_device_key().unwrap();
        app.app_settings.application_layouts.remove(&key);
        app.update_application_layout_runtime_with(Default::default());
        let settings = app.application_layout_settings().unwrap();
        assert!(settings.layouts.len()>1);
        for p in settings.layouts.values() {
            assert_eq!(p.layers,vec![vec![4;62];16],"{}",p.name);
            assert!(p.encoder_stacks.iter().all(Vec::is_empty));
        }
        assert!(crate::app::app_storage::load_app_settings().application_layouts[&key].layouts.values().all(|p|p.layers[15].len()==62));
    }

    #[test]
    fn sm62_exact_identity_has_isolated_application_profile_key() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&cc);
        let mut device = m4cr0pad_v3_device();
        device.product_id = 0x00B2;
        device.name = "SM62".into();
        assert!(device_supports_application_layouts(&device));
        app.current_keyboard_id = Some(0xBB17F05B02801D1D);
        app.device_manager.replace_devices(vec![device.clone()]);
        app.selected_device = Some(0);
        assert!(app.application_layout_device_key().is_none());
        app.device_about_info = Some(DeviceAboutInfo {
            supports_application_layouts: true,
            vendor_id: device.vendor_id, product_id: device.product_id,
            path: device.path.clone(), ..Default::default()
        });
        let owner_key = crate::qmk_hid_host::sm62_owner_library_key(
            "vial-bb17f05b02801d1d", &device.instance_token);
        assert_eq!(app.application_layout_device_key().as_deref(), Some(owner_key.as_str()));
        assert_ne!(owner_key, "vial-bb17f05b02801d1d-sm62",
            "the old model-wide archive is not this owner's live library");
        let mut replacement = device.clone();
        replacement.instance_token = "different-sm62-enumeration".into();
        app.device_manager.replace_devices(vec![replacement]);
        assert_ne!(app.application_layout_device_key().as_deref(), Some(owner_key.as_str()),
            "a new enumeration must not inherit the prior owner's cache namespace");
        device.product_id = 0x9999;
        assert!(!device_supports_application_layouts(&device));
        device.product_id = 0x00B2;
        device.vendor_id = 0x9999;
        assert!(!device_supports_application_layouts(&device));
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
        ui_layer_rename_persists_to_firmware(false);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_ui_default_layer_rename_persists_to_firmware() {
        ui_layer_rename_persists_to_firmware(true);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn ui_layer_rename_persists_to_firmware(sm62: bool) {
        use crate::application_layouts::{DetectedApplication, DEFAULT_APPLICATION_LAYOUT_ID};
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for (mode, scenario) in [
            (1, "ok"),
            (2, "ok"),
            (0, "ok"),
            (0, "borrowed"),
            (0, "switch_profile"),
            (0, "failed_layer"),
            (0, "set_error"),
            (0, "get_error"),
            (0, "mismatch"),
            (0, "stale_device"),
            (0, "draft_switch_device"),
            (0, "unicode"),
        ] {
            let ctx = egui::Context::default();
            let mut app = EntropyApp::new_inert_for_test();
            let mut device = m4cr0pad_v3_device();
            if sm62 { device.product_id = 0x00b2; device.name = "SM62".into(); }
            device.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
            app.device_manager.replace_devices(vec![device]);
            app.selected_device = Some(0);
            app.current_keyboard_id = Some(0x42);
            app.current_device_name = if sm62 { "SM62" } else { "Macropad" }.into();
            app.firmware = FirmwareProtocol::Vial;
            mark_application_layout_protocol_supported(&mut app);
            app.layout = Some(if sm62 { sm62_test_layout() } else { indicator_test_layout() });
            app.layer_count = 16;
            app.layer_names = crate::application_layouts::default_layer_names();
            app.deferred_device_load = DeferredDeviceLoadState::complete(16);
            let key = app.application_layout_device_key().unwrap();
            let seed = EntropyApp::base_application_layers(app.layout.as_ref().unwrap());
            let settings = app.application_layout_settings_mut().unwrap();
            if sm62 { assert!(settings.layouts[DEFAULT_APPLICATION_LAYOUT_ID].layers.iter().all(|l| l.len() == 62)); }
            let other = settings.create_for_application(&DetectedApplication {
                executable: "rename-control.exe".into(),
                ..Default::default()
            });
            let profile = if mode == 1 {
                other.clone()
            } else {
                DEFAULT_APPLICATION_LAYOUT_ID.into()
            };
            settings.active_layout_id = profile.clone();
            settings.editor_layout_id = profile.clone();
            for layout in settings.layouts.values_mut() { layout.seed_unset_keycodes(seed.clone()); }
            let mut untouched = settings.layouts[if mode == 1 { DEFAULT_APPLICATION_LAYOUT_ID } else { &other }].clone();
            app.application_layout_editor_active = mode != 2;
            let (hid, packets) = crate::hid::HidDevice::test_device();
            let store = std::sync::Arc::new(std::sync::Mutex::new("Factory".to_owned()));
            let firmware = store.clone();
            let (release, gate) = std::sync::mpsc::channel();
            let gate = std::sync::Mutex::new(gate);
            packets.respond_by(move |request| {
                if request[..2] == [0xfe, 0x08] {
                    gate.lock()
                        .unwrap()
                        .recv_timeout(std::time::Duration::from_secs(3))
                        .unwrap();
                    Some(Ok([0; 32]))
                } else if request[..2] == [0xfe, 0x0b] && request[2..4] == 200u16.to_le_bytes() {
                    if scenario == "set_error" {
                        return Some(Ok([0xff; 32]));
                    }
                    let end = request[4..].iter().position(|b| *b == 0).unwrap_or(28);
                    *firmware.lock().unwrap() = String::from_utf8(request[4..4 + end].to_vec())
                        .unwrap()
                        .replace("%%", "%");
                    Some(Ok([0; 32]))
                } else if request[..2] == [0xfe, 0x0a] && request[2..4] == 200u16.to_le_bytes() {
                    if scenario == "get_error" {
                        return Some(Ok([0xff; 32]));
                    }
                    if scenario == "mismatch" {
                        return Some(Ok([0; 32]));
                    }
                    let name = firmware.lock().unwrap();
                    let mut response = [0; 32];
                    response[1..1 + name.len()].copy_from_slice(name.as_bytes());
                    Some(Ok(response))
                } else {
                    None
                }
            });
            app.hid_device = Some(hid.into());
            let frame = |app: &mut EntropyApp, events| {
                crate::app::discard_texture_deltas(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1100.0, 800.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| app.draw_layout_layer_switcher_and_hints(ui, 0.0, 0.0, 52.0),
                ))
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
            frame(&mut app, vec![egui::Event::Text("New".into())]);
            if scenario == "unicode" {
                frame(
                    &mut app,
                    vec![egui::Event::Key {
                        key: egui::Key::A,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers {
                            command: true,
                            ctrl: true,
                            ..Default::default()
                        },
                    }],
                );
                frame(&mut app, vec![egui::Event::Text("Слой%Макро".into())]);
            }
            let proposed = app.editing_layer_text.clone();
            let expected = if scenario == "unicode" {
                "Слой%Мак".to_owned()
            } else {
                proposed.clone()
            };
            let (unrelated_hid, unrelated_packets) = crate::hid::HidDevice::test_device();
            let unrelated_key = "vial-0000000000000043";
            let unrelated_settings =
                crate::application_layouts::DeviceApplicationLayouts::default();
            if scenario == "draft_switch_device" {
                app.app_settings
                    .application_layouts
                    .insert(unrelated_key.into(), unrelated_settings.clone());
                app.current_keyboard_id = Some(0x43);
                app.connection_generation += 1;
                app.hid_device = Some(unrelated_hid.into());
            }
            if scenario == "borrowed" {
                assert_eq!(
                    app.start_vial_hid_operation(
                        &ctx,
                        super::super::vial_hid_task::VialHidOperation::Lock
                    ),
                    super::super::vial_hid_task::VialHidTaskStart::Started
                );
            }
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
            if mode == 0 {
                assert!(
                    !packets.requests().iter().any(|p| p[..2] == [0xfe, 0x0b]),
                    "UI must not synchronously write HID"
                );
            }
            if scenario == "failed_layer" {
                app.deferred_device_load.set_layer_status(
                    0,
                    DeferredLoadStatus::Failed("synthetic cold-read failure".into()),
                );
            }
            if scenario == "switch_profile" {
                assert!(app.activate_application_layout(&other));
                // This is an intentional manual selection of the control,
                // not rename/readback authoring. Only its provenance changes;
                // retain byte-for-byte equality of every other profile field.
                untouched.user_edit_revision = untouched.user_edit_revision.wrapping_add(1).max(1);
            }
            if scenario == "stale_device" {
                app.connection_generation += 1;
                app.device_manager
                    .replace_devices(vec![crate::device::Device {
                        path: "unrelated-owner".into(),
                        ..m4cr0pad_v3_device()
                    }]);
            }
            if mode == 0 && scenario != "draft_switch_device" {
                assert!(app.defer_exit_until_hid_write_returns(&ctx));
            }
            // Drain production workers only; no direct setting writer in the probe.
            app.poll_vial_hid_task(&ctx);
            if scenario == "borrowed" {
                assert_eq!(
                    app.pending_layer_name_writes.len(),
                    1,
                    "borrowed owner must retain rename"
                );
                assert!(!packets.requests().iter().any(|p| p[..2] == [0xfe, 0x0b]));
                app.application_layout_manual_override = Some((key.clone(), None));
                app.last_device_scan_at = 1.0;
                app.last_full_device_scan_at = 1.0;
                app.tick_layout_import_lifecycle_for_test(&ctx, 1.0);
                assert_eq!(app.pending_layer_name_writes.len(), 1);
                release.send(()).unwrap();
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while app.vial_hid_task_active() {
                app.poll_vial_hid_task(&ctx);
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert!(
                unrelated_packets.requests().is_empty(),
                "rename wrote an unrelated HID owner"
            );
            if mode == 0 {
                assert!(app.pending_layer_name_writes.is_empty());
                app.finish_deferred_exit_after_hid_write(&ctx);
                assert!(!app.exit_after_hid_write, "rename must drain before exit");
            }
            if mode != 2 {
                let saved = crate::app::app_storage::load_app_settings();
                assert_eq!(
                    saved.application_layouts[&key].layouts[&profile].layer_names[0],
                    expected
                );
                assert_eq!(
                    saved.application_layouts[&key].layouts[if mode == 1 {
                        DEFAULT_APPLICATION_LAYOUT_ID
                    } else {
                        &other
                    }],
                    untouched
                );
                if scenario == "draft_switch_device" {
                    assert_eq!(saved.application_layouts[unrelated_key], unrelated_settings);
                }
            }
            if mode == 1 {
                assert!(
                    packets.requests().is_empty(),
                    "non-Default must remain host-only"
                );
                assert_eq!(*store.lock().unwrap(), "Factory");
            } else if matches!(
                scenario,
                "failed_layer" | "set_error" | "stale_device" | "draft_switch_device"
            ) {
                assert_eq!(*store.lock().unwrap(), "Factory");
                if scenario != "set_error" {
                    assert!(
                        !packets.requests().iter().any(|p| p[..2] == [0xfe, 0x0b]),
                        "unsafe rename was admitted"
                    );
                }
            } else {
                assert_eq!(
                    *store.lock().unwrap(),
                    expected,
                    "UI rename left persistent firmware name unchanged (mode={mode})"
                );
                if mode == 0 {
                    assert!(
                        packets.requests().iter().any(|p| p[..2] == [0xfe, 0x0a]),
                        "Default requires confirmed firmware readback"
                    );
                }
            }
            if matches!(
                scenario,
                "get_error" | "mismatch" | "set_error" | "failed_layer"
            ) {
                assert!(
                    app.status_msg.contains("Layer name not saved"),
                    "{}",
                    app.status_msg
                );
                assert_ne!(
                    app.layer_names[0], proposed,
                    "unconfirmed name was published to firmware cache"
                );
                let before = packets.requests().len();
                for _ in 0..3 {
                    app.poll_vial_hid_task(&ctx);
                    frame(&mut app, vec![]);
                }
                assert_eq!(
                    packets.requests().len(),
                    before,
                    "failure retried on repaint"
                );
            }
            drop(app); // synthetic nonvolatile store outlives all host state
            if mode != 1
                && !matches!(
                    scenario,
                    "failed_layer" | "set_error" | "stale_device" | "draft_switch_device"
                )
            {
                assert_eq!(*store.lock().unwrap(), expected);
            }
        }
    }

    #[test]
    fn test14_default_name_no_enter_close() { test14_no_enter_close_probe(true); test14_no_enter_close_probe(false); }
    fn test14_no_enter_close_probe(sm62: bool) {
        use crate::application_layouts::{DetectedApplication, DEFAULT_APPLICATION_LAYOUT_ID};
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for (mode, scenario) in [(0,"ok"),(0,"borrowed"),(1,"ok"),
            (0,"switch_profile"),(0,"failed_layer"),(0,"set_error"),
            (0,"get_error"),(0,"mismatch"),(0,"stale_device"),
            (0,"draft_switch_device"),(0,"unicode")] {
            let ctx = egui::Context::default();
            let mut app = EntropyApp::new_inert_for_test();
            let mut device = m4cr0pad_v3_device();
            if sm62 { device.product_id = 0x00b2; device.name = "SM62".into(); }
            device.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
            app.device_manager.replace_devices(vec![device]);
            app.selected_device = Some(0);
            app.current_keyboard_id = Some(0x42);
            app.current_device_name = if sm62 { "SM62" } else { "Macropad" }.into();
            app.firmware = FirmwareProtocol::Vial;
            mark_application_layout_protocol_supported(&mut app);
            app.layout = Some(if sm62 { sm62_test_layout() } else { indicator_test_layout() });
            app.layer_count = 16;
            app.layer_names = crate::application_layouts::default_layer_names();
            app.deferred_device_load = DeferredDeviceLoadState::complete(16);
            let key = app.application_layout_device_key().unwrap();
            let seed = EntropyApp::base_application_layers(app.layout.as_ref().unwrap());
            let settings = app.application_layout_settings_mut().unwrap();
            if sm62 { assert!(settings.layouts[DEFAULT_APPLICATION_LAYOUT_ID].layers.iter().all(|l| l.len() == 62)); }
            let other = settings.create_for_application(&DetectedApplication {
                executable: "rename-control.exe".into(), ..Default::default()
            });
            let profile = if mode == 1 { other.clone() } else { DEFAULT_APPLICATION_LAYOUT_ID.into() };
            settings.active_layout_id = profile.clone();
            settings.editor_layout_id = profile.clone();
            for layout in settings.layouts.values_mut() { layout.seed_unset_keycodes(seed.clone()); }
            let mut untouched = settings.layouts[if mode == 1 { DEFAULT_APPLICATION_LAYOUT_ID } else { &other }].clone();
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
            app.hid_device = Some(hid.into());
            let frame = |app: &mut EntropyApp, events| {
                let _ = crate::app::discard_texture_deltas(ctx.run_ui(egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0,800.0))),
                    events, ..Default::default()
                }, |ui| app.draw_layout_layer_switcher_and_hints(ui,0.0,0.0,52.0)));
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
                app.hid_device = Some(unrelated_hid.into());
            }
            if scenario == "borrowed" {
                assert_eq!(app.start_vial_hid_operation(&ctx,super::super::vial_hid_task::VialHidOperation::Lock),
                    super::super::vial_hid_task::VialHidTaskStart::Started);
            }
            app.app_settings.close_to_tray_behavior = CloseToTrayBehavior::Close;
            app.app_settings.minimize_to_tray_on_close = false;
            let mut close_input = egui::RawInput::default();
            close_input.viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
            let output = crate::app::discard_texture_deltas(ctx.run_ui(close_input, |ui| {
                app.handle_close_to_tray(&ctx);
                app.draw_layout_layer_switcher_and_hints(ui,0.0,0.0,52.0);
            }));
            assert!(app.editing_layer.is_none(), "ordinary close lost open draft without Enter: sm62={sm62} scenario={scenario}");
            if mode == 0 && scenario != "draft_switch_device" {
                assert!(output.viewport_output[&egui::ViewportId::ROOT].commands.contains(&egui::ViewportCommand::CancelClose), "close must defer for native SET/GET");
            }
            assert!(app.editing_layer.is_none());
            if mode == 0 {
                assert!(!packets.requests().iter().any(|p| p[..2]==[0xfe,0x0b]),"UI must not synchronously write HID");
            }
            if scenario == "failed_layer" {
                app.deferred_device_load.set_layer_status(0, DeferredLoadStatus::Failed("synthetic cold-read failure".into()));
            }
            if scenario == "switch_profile" {
                assert!(app.activate_application_layout(&other));
                // This is an intentional manual selection of the control,
                // not rename/readback authoring. Only its provenance changes;
                // retain byte-for-byte equality of every other profile field.
                untouched.user_edit_revision = untouched.user_edit_revision.wrapping_add(1).max(1);
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
            if let Ok(root) = std::env::var("TEST14_OUTPUT") {
                std::fs::write(std::path::Path::new(&root).join(format!("close-packets-{sm62}-{mode}-{scenario}.json")),serde_json::to_string_pretty(&packets.requests()).unwrap()).unwrap();
            }
            println!("CLOSE no Enter: sm62={sm62} mode={mode} scenario={scenario} native_layers={} label={expected:?} requests={}",app.layer_count,packets.requests().len());
            drop(app); // synthetic nonvolatile store outlives all host state
            if mode != 1 && !matches!(scenario,"failed_layer"|"set_error"|"stale_device"|"draft_switch_device") {
                assert_eq!(*store.lock().unwrap(),expected);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ui_name_utf8_whitespace_boundary_confirms_production_worker() { name_utf8_whitespace_boundary(false); }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_ui_name_utf8_whitespace_boundary_confirms_production_worker() { name_utf8_whitespace_boundary(true); }

    #[cfg(not(target_arch = "wasm32"))]
    fn name_utf8_whitespace_boundary(sm62: bool) {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for (proposal, stored, confirmed) in [
            ("абвгдеж x", "абвгдеж ", "абвгдеж"),
            ("Слой%Макро", "Слой%Мак", "Слой%Мак"),
            ("100%", "100%", "100%"),
            ("abcdefghijkl", "abcdefghijkl", "abcdefghijkl"),
        ] {
            let ctx = egui::Context::default();
            let mut app = name_review_device_app(sm62);
            let (hid, packets) = crate::hid::HidDevice::test_device();
            let store = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
            let firmware = store.clone();
            packets.respond_by(move |r| {
                if r[..4] == [0xfe, 0x0b, 200, 0] {
                    let end = r[4..].iter().position(|b| *b == 0).unwrap_or(28);
                    *firmware.lock().unwrap() = String::from_utf8(r[4..4 + end].to_vec())
                        .unwrap()
                        .replace("%%", "%");
                    Some(Ok([0; 32]))
                } else if r[..4] == [0xfe, 0x0a, 200, 0] {
                    let name = firmware.lock().unwrap();
                    let mut response = [0; 32];
                    response[1..1 + name.len()].copy_from_slice(name.as_bytes());
                    Some(Ok(response))
                } else {
                    None
                }
            });
            app.hid_device = Some(hid.into());
            name_review_rename(&mut app, &ctx, proposal);
            name_review_drain(&mut app, &ctx);
            assert_eq!(*store.lock().unwrap(), stored, "setter wire budget changed");
            assert_eq!(
                app.layer_names[0], confirmed,
                "successful UI SET/GET was not confirmed: {}",
                app.status_msg
            );
            assert_eq!(app.layout.as_ref().unwrap().layer_names[0], confirmed);
            assert!(!app.selected_layer_names_uncertain(), "false uncertainty for {proposal}");
            assert!(!app.status_msg.contains("Layer name not saved"), "{}", app.status_msg);
            assert_eq!(crate::app::app_storage::load_layer_names(&app.current_device_name)[0], confirmed);
            let before = packets.requests().len();
            name_review_frame(&mut app, &ctx, vec![]);
            app.poll_vial_hid_task(&ctx);
            assert_eq!(
                packets.requests().len(),
                before,
                "repaint issued a new write"
            );
            assert_eq!(
                packets
                    .requests()
                    .iter()
                    .filter(|r| r[..2] == [0xfe, 0x0b])
                    .count(),
                1
            );
            assert_eq!(
                packets
                    .requests()
                    .iter()
                    .filter(|r| r[..2] == [0xfe, 0x0a])
                    .count(),
                1
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_default_names_follow_confirmed_keyboard_before_and_after_profile_fallback() {
        use crate::application_layouts::{DetectedApplication, DEFAULT_APPLICATION_LAYOUT_ID};
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for cached in [false, true] {
            let mut app = name_review_device_app(true);
            let native: Vec<String> = (0..16).map(|l| format!("Saved {l}")).collect();
            app.layer_names = native.clone();
            app.confirmed_firmware_layer_names = 0xffff;
            app.layout.as_mut().unwrap().layer_names = native.clone();
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            app.hid_device = Some(hid.into());
            app.deferred_device_load = DeferredDeviceLoadState::complete(16);
            app.default_layout_device_reconciled = false;
            let key = app.application_layout_device_key().unwrap();
            app.app_settings.application_layouts.remove(&key);
            if cached {
                let settings = app.application_layout_settings_mut().unwrap();
                for (l, name) in settings.layouts.get_mut(DEFAULT_APPLICATION_LAYOUT_ID).unwrap().layer_names.iter_mut().enumerate() {
                    *name = format!("Host-only {l}");
                }
            }
            app.maybe_sync_default_application_layout_to_device();
            let settings = app.application_layout_settings().unwrap();
            assert_eq!(settings.layouts[DEFAULT_APPLICATION_LAYOUT_ID].layer_names, native,
                "Default's GUI/runtime name must equal the already persisted keyboard name (cached={cached})");
            if cached {
                let directory = app_settings_path().parent().unwrap().join("backups");
                let files: Vec<_> = std::fs::read_dir(directory).into_iter().flatten().flatten().collect();
                assert!(!files.is_empty(), "host-only Default names were overwritten without a recovery copy");
                assert!(files.iter().any(|entry| std::fs::read_to_string(entry.path()).is_ok_and(|s| s.contains("Host-only 0"))));
            }
            let settings = app.application_layout_settings_mut().unwrap();
            let browser = settings.create_for_application(&DetectedApplication { executable: "offline-name-browser".into(), ..Default::default() });
            settings.layouts.get_mut(&browser).unwrap().set_layer_name(0, "Browser only".into());
            assert!(app.activate_application_layout(&browser));
            assert!(app.activate_application_layout(DEFAULT_APPLICATION_LAYOUT_ID));
            assert_eq!(app.application_layout_active_layer_names(), native);
            assert_eq!(app.layout.as_ref().unwrap().layer_names, native);
            assert!(recorder.requests().is_empty(), "reconciliation must not rewrite firmware or application names into Default");
            let saved = crate::app::app_storage::load_app_settings();
            assert_eq!(saved.application_layouts[&key].layouts[DEFAULT_APPLICATION_LAYOUT_ID].layer_names, native);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_default_name_reconciliation_preserves_unknown_pending_and_application_names() {
        use crate::application_layouts::{DetectedApplication, DEFAULT_APPLICATION_LAYOUT_ID};
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for scenario in ["unknown", "queued", "uncertain", "editing", "confirmed", "backup_error"] {
            let mut app = name_review_device_app(true);
            app.layer_names = (0..16).map(|l| format!("Native {l}")).collect();
            app.layout.as_mut().unwrap().layer_names = app.layer_names.clone();
            app.confirmed_firmware_layer_names = if scenario == "unknown" { 0 } else { 0xffff };
            app.default_layout_device_reconciled = false;
            let key = app.application_layout_device_key().unwrap();
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            app.hid_device = Some(hid.into());
            let settings = app.application_layout_settings_mut().unwrap();
            settings.layouts.get_mut(DEFAULT_APPLICATION_LAYOUT_ID).unwrap().set_layer_name(0, "User draft".into());
            let other = settings.create_for_application(&DetectedApplication { executable: "untouched-name-control".into(), ..Default::default() });
            settings.layouts.get_mut(&other).unwrap().set_layer_name(0, "Application only".into());
            if scenario == "queued" || scenario == "uncertain" {
                let request = super::super::vial_hid_task::LayerNameWrite {
                    device: app.device_manager.devices()[0].clone(), device_key: key.clone(),
                    layout_id: DEFAULT_APPLICATION_LAYOUT_ID.into(), generation: app.connection_generation,
                    layer: 0, name: "User draft".into(),
                };
                if scenario == "queued" { app.pending_layer_name_writes.push_back(request); }
                else { app.uncertain_layer_names.push(request); }
            }
            if scenario == "editing" {
                app.editing_layer = Some(0); app.editing_layer_layout_id = Some(DEFAULT_APPLICATION_LAYOUT_ID.into());
                app.editing_layer_device_key = Some(key.clone());
            }
            if scenario == "backup_error" {
                let directory = app_settings_path().parent().unwrap().join("backups");
                if directory.exists() { std::fs::remove_dir_all(&directory).unwrap(); }
                std::fs::write(directory, b"blocked backup destination").unwrap();
            }
            app.maybe_sync_default_application_layout_to_device();
            if scenario == "backup_error" { assert!(app.status_msg.contains("backup failed") || app.status_msg.contains("резервную копию")); }
            let settings = app.application_layout_settings().unwrap();
            assert_eq!(settings.layouts[DEFAULT_APPLICATION_LAYOUT_ID].layer_names[0],
                if scenario == "confirmed" { "Native 0" } else { "User draft" }, "{scenario}");
            if scenario != "unknown" && scenario != "backup_error" { assert_eq!(settings.layouts[DEFAULT_APPLICATION_LAYOUT_ID].layer_names[15], "Native 15"); }
            assert_eq!(settings.layouts[&other].layer_names[0], "Application only");
            assert!(recorder.requests().is_empty());
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_and_macropad_native_background_auto_manual_and_default_fallback_parity() {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        for sm62 in [false,true] {
            let ctx=egui::Context::default(); let mut app=name_review_device_app(sm62);
            app.default_layout_device_reconciled=true;
            let foreground=crate::application_layouts::DetectedApplication { executable:"native-lifecycle-editor".into(), ..Default::default() };
            let device=app.device_manager.devices()[0].clone();
            let (hid,recorder)=if sm62 {let(h,r,_)=runtime_fixture_hid(Vec::new());(h,r)}else{crate::hid::HidDevice::test_device()};
            let mut bridge=crate::qmk_hid_host::test_start_bridge(device.clone(),Default::default(),hid.shared_output(),None,
                crate::qmk_hid_host::HostProtocol::Selected(true),||None);
            bridge.set_selected_application_layout_support(true);
            bridge.bind_selected_application_layout_device(app.application_layout_device_key().unwrap());
            app.qmk_hid_hosts.insert(device.path.clone(),bridge);
            if sm62 {runtime_fixture_connect(&mut app,&device,hid);}else{app.shared_hid_output=hid.shared_output();app.hid_device=Some(hid.into());}
            let settings=app.application_layout_settings_mut().unwrap();
            let automatic=settings.create_for_application(&foreground);
            let manual=settings.create_for_application(&crate::application_layouts::DetectedApplication { executable:"manual-control".into(), ..Default::default() });
            settings.layouts.get_mut(&automatic).unwrap().set_keycode(15,if sm62 {61} else {0},5);
            assert!(app.activate_application_layout(&automatic));
            let tick=|app:&mut EntropyApp, focused:crate::application_layouts::DetectedApplication| {
                crate::app_discovery::with_test_application_discovery(crate::app_discovery::ApplicationDiscoverySnapshot {
                    foreground:Some(focused.clone()), foreground_status:crate::app_discovery::ForegroundStatus {
                        backend:"synthetic OS boundary".into(),state:crate::app_discovery::ForegroundState::Focused(focused)
                    }, ..Default::default()
                }, ||app.tick_layout_import_lifecycle_for_test(&ctx,1.0));
            };
            tick(&mut app,foreground.clone());
            assert_eq!(app.application_layout_settings().unwrap().active_layout_id,automatic);
            assert!(app.activate_application_layout(&manual));
            tick(&mut app,foreground.clone());
            assert_eq!(app.application_layout_settings().unwrap().active_layout_id,manual);
            tick(&mut app,crate::application_layouts::DetectedApplication { executable:"unmatched-application".into(), ..Default::default() });
            assert_eq!(app.application_layout_settings().unwrap().active_layout_id,"default");
            assert_eq!(app.qmk_hid_hosts[&device.path].test_application_layout_snapshot().profile_id.as_deref(),Some("default"));
            assert!(recorder.requests().iter().all(|r| r[0] != 0x05 && r[..2] != [0xfe,0x0b]));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_bridge_does_not_poll_while_default_write_worker_borrows_hid() {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        let ctx=egui::Context::default();let mut app=name_review_device_app(true);
        app.vial_unlocked=Some(true);app.default_layout_device_reconciled=true;
        let (hid,recorder)=crate::hid::HidDevice::test_device();
        let (entered_tx,entered_rx)=std::sync::mpsc::channel();
        let (release_tx,release_rx)=std::sync::mpsc::channel();
        recorder.respond_by(move |r| {
            if r[0]==0xeb {return Some(Ok(*r));}
            if r[0]==0xe6 {let mut reply=[0;32];reply[..3].copy_from_slice(&[0xe6,8,0x5a]);return Some(Ok(reply));}
            if r[0]==0x05 {entered_tx.send(()).unwrap();release_rx.recv_timeout(std::time::Duration::from_secs(4)).unwrap();return Some(Ok([0;32]));}
            if r[0]==0x04 {let mut reply=*r;reply[4..6].copy_from_slice(&9u16.to_be_bytes());return Some(Ok(reply));}
            None
        });
        let device=app.device_manager.devices()[0].clone();
        let mut bridge=crate::qmk_hid_host::test_start_bridge(device,Default::default(),hid.shared_output(),None,
            crate::qmk_hid_host::HostProtocol::Selected(true),||None);
        bridge.set_selected_application_layout_support(true);
        bridge.bind_selected_application_layout_device(app.application_layout_device_key().unwrap());
        bridge.set_application_layout_snapshot(crate::application_layouts::ApplicationLayoutSnapshot::from_layout(&app.application_layout_settings().unwrap().layouts["default"]));
        app.hid_device=Some(hid.into());
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(4);
        while !recorder.requests().iter().any(|r|r[0]==0xe6) {
            assert!(std::time::Instant::now()<deadline);std::thread::sleep(std::time::Duration::from_millis(5));
        }
        app.selected_layer=15;assert_eq!(app.assign_application_layout_key(61,9),Some(true));
        entered_rx.recv_timeout(std::time::Duration::from_secs(3)).unwrap();
        let before=recorder.requests().iter().filter(|r|r[0]==0xe6).count();
        std::thread::sleep(std::time::Duration::from_millis(160));
        let after=recorder.requests().iter().filter(|r|r[0]==0xe6).count();
        release_tx.send(()).unwrap();
        while app.hid_write_task_active() {
            app.poll_layer_write(&ctx);assert!(std::time::Instant::now()<deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        while recorder.requests().iter().filter(|r|r[0]==0xe6).count()==after {
            assert!(std::time::Instant::now()<deadline,"event poll did not resume after worker returned");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        bridge.suppress_shutdown();bridge.stop();
        assert_eq!(after,before,"event polling interleaved with a borrowed HID write transaction");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_default_last_control_layer15_uses_serialized_firmware_set_get() {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        let ctx=egui::Context::default(); let mut app=name_review_device_app(true);
        app.vial_unlocked=Some(true); app.default_layout_device_reconciled=true;
        let (hid,recorder)=crate::hid::HidDevice::test_device();
        let memory=std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::new()));
        let wire=memory.clone();
        recorder.respond_by(move |r| {
            if r[0]==0x05 { wire.lock().unwrap().insert((r[1],r[2],r[3]),u16::from_be_bytes([r[4],r[5]])); Some(Ok([0;32])) }
            else if r[0]==0x04 { let mut reply=[0;32]; reply[..4].copy_from_slice(&r[..4]);
                reply[4..6].copy_from_slice(&wire.lock().unwrap().get(&(r[1],r[2],r[3])).copied().unwrap_or(4u16).to_be_bytes()); Some(Ok(reply)) }
            else {None}
        });
        app.hid_device=Some(hid.into()); app.selected_layer=15;
        assert_eq!(app.assign_application_layout_key(61,9),Some(true));
        assert!(app.hid_write_task_active(),"Default edit bypassed serialized worker");
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(3);
        while app.hid_write_task_active() {
            app.poll_layer_write(&ctx); assert!(std::time::Instant::now()<deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(memory.lock().unwrap().get(&(15,9,3)),Some(&9));
        assert!(recorder.requests().iter().any(|r|r[..4]==[0x04,15,9,3]));
        assert_eq!(app.layout.as_ref().unwrap().layers[15][61].vial_keycode(),9);
        let key=app.application_layout_device_key().unwrap();
        assert_eq!(crate::app::app_storage::load_app_settings().application_layouts[&key].layouts["default"].layers[15][61],9);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_all_application_key_edits_persist_without_default_eeprom_writes() {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        let mut app = name_review_device_app(true);
        let key = app.application_layout_device_key().unwrap();
        let profile = app.application_layout_settings_mut().unwrap().create_for_application(&crate::application_layouts::DetectedApplication {
            executable:"full-keyboard-editor".into(), ..Default::default()
        });
        app.application_layout_settings_mut().unwrap().active_layout_id=profile.clone();
        let (hid,recorder)=crate::hid::HidDevice::test_device(); app.hid_device=Some(hid.into());
        for layer in 0..16 { app.selected_layer=layer; for control in 0..62 {
            assert_eq!(app.assign_application_layout_key(control,5+(control%20) as u16),Some(true));
        }}
        let reloaded=crate::app::app_storage::load_app_settings();
        for layer in 0..16 { for control in 0..62 {
            assert_eq!(reloaded.application_layouts[&key].layouts[&profile].layers[layer][control],5+(control%20) as u16);
        }}
        assert!(recorder.requests().is_empty());
        assert_eq!(reloaded.application_layouts[&key].layouts["default"].layers,vec![vec![4;62];16]);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn name_review_app() -> EntropyApp { name_review_device_app(false) }

    #[cfg(not(target_arch = "wasm32"))]
    fn name_review_device_app(sm62: bool) -> EntropyApp {
        let mut app = EntropyApp::new_inert_for_test();
        let mut device = m4cr0pad_v3_device();
        if sm62 { device.product_id=0x00b2; device.name="SM62".into(); }
        device.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
        app.device_manager.replace_devices(vec![device]);
        app.selected_device = Some(0);
        app.current_keyboard_id = Some(0x42);
        app.current_device_name = if sm62 { "SM62" } else { "Macropad" }.into();
        app.firmware = FirmwareProtocol::Vial;
        mark_application_layout_protocol_supported(&mut app);
        app.layout = Some(if sm62 { sm62_test_layout() } else { indicator_test_layout() });
        app.layer_count = 16;
        app.layer_names = crate::application_layouts::default_layer_names();
        app.deferred_device_load = DeferredDeviceLoadState::complete(16);
        app.application_layout_settings_mut().unwrap();
        app.application_layout_editor_active = true;
        app
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn name_review_frame(app: &mut EntropyApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        crate::app::discard_texture_deltas(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.draw_layout_layer_switcher_and_hints(ui, 0.0, 0.0, 52.0),
        ))
        .drop_without_applying_deltas();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn name_review_rename(app: &mut EntropyApp, ctx: &egui::Context, name: &str) {
        let pos = egui::pos2(550.0, 50.0);
        name_review_frame(app, ctx, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            name_review_frame(
                app,
                ctx,
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
        name_review_frame(app, ctx, vec![]);
        assert_eq!(app.editing_layer, Some(app.selected_layer));
        name_review_frame(
            app,
            ctx,
            vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    command: true,
                    ctrl: true,
                    ..Default::default()
                },
            }],
        );
        let replacement = if name.is_empty() {
            egui::Event::Key {
                key: egui::Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }
        } else {
            egui::Event::Text(name.into())
        };
        name_review_frame(app, ctx, vec![replacement]);
        name_review_frame(
            app,
            ctx,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cleared_layer_name_commits_from_ui_and_remains_editable() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for mode in 0..3 {
            let ctx = egui::Context::default();
            let mut app = name_review_app();
            let key = app.application_layout_device_key().unwrap();
            let settings = app.app_settings.application_layouts.get_mut(&key).unwrap();
            let profile = if mode == 2 {
                settings.create_for_application(&crate::application_layouts::DetectedApplication {
                    executable: "clear-name.exe".into(),
                    ..Default::default()
                })
            } else {
                crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID.to_owned()
            };
            settings.active_layout_id = profile.clone();
            settings.editor_layout_id = profile.clone();
            settings
                .layouts
                .get_mut(&profile)
                .unwrap()
                .set_layer_name(0, "Old".into());
            app.application_layout_editor_active = mode != 0;
            app.layer_names[0] = "Old".into();
            app.layout.as_mut().unwrap().layer_names[0] = "Old".into();
            let (hid, packets) = crate::hid::HidDevice::test_device();
            let store = std::sync::Arc::new(std::sync::Mutex::new("Old".to_owned()));
            let firmware = store.clone();
            packets.respond_by(move |r| {
                if r[..4] == [0xfe, 0x0b, 200, 0] {
                    let end = r[4..].iter().position(|b| *b == 0).unwrap_or(28);
                    *firmware.lock().unwrap() = String::from_utf8(r[4..4 + end].to_vec()).unwrap();
                    Some(Ok([0; 32]))
                } else if r[..4] == [0xfe, 0x0a, 200, 0] {
                    let name = firmware.lock().unwrap();
                    let mut response = [0; 32];
                    response[1..1 + name.len()].copy_from_slice(name.as_bytes());
                    Some(Ok(response))
                } else {
                    None
                }
            });
            app.hid_device = Some(hid.into());
            name_review_rename(&mut app, &ctx, "");
            name_review_drain(&mut app, &ctx);
            assert_eq!(app.editing_layer, None);
            if mode == 2 {
                assert_eq!(*store.lock().unwrap(), "Old");
                assert!(!packets.requests().iter().any(|r| r[..2] == [0xfe, 0x0b]));
            } else {
                assert_eq!(
                    *store.lock().unwrap(),
                    "",
                    "mode {mode}: clear did not reach firmware"
                );
                assert_eq!(app.layer_names[0], "");
                assert_eq!(crate::app::app_storage::load_layer_names("Macropad")[0], "");
            }
            if mode != 0 {
                assert_eq!(
                    app.app_settings.application_layouts[&key].layouts[&profile].layer_names[0],
                    ""
                );
                assert_eq!(
                    crate::app::app_storage::load_app_settings().application_layouts[&key].layouts
                        [&profile]
                        .layer_names[0],
                    ""
                );
            }
            let output = crate::app::discard_texture_deltas(ctx.run_ui(egui::RawInput::default(), |ui| {
                app.draw_layout_layer_switcher_and_hints(ui, 0.0, 0.0, 52.0);
            }));
            assert!(output.shapes.iter().any(|shape| matches!(
                &shape.shape, egui::Shape::Text(text) if text.galley.text() == "0"
            )), "an unnamed layer must display its number");
            // Clicking the number of an unnamed layer must still start editing.
            name_review_rename(&mut app, &ctx, "Restored");
            name_review_drain(&mut app, &ctx);
            let names = if mode == 0 {
                app.layer_names.clone()
            } else {
                app.application_layout_editor_layer_names()
            };
            assert_eq!(names[0], "Restored");

            let writes = packets.requests().len();
            app.editing_layer = Some(0);
            app.editing_layer_layout_id = (mode != 0).then(|| profile.clone());
            app.editing_layer_text.clear();
            name_review_frame(
                &mut app,
                &ctx,
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            let names = if mode == 0 {
                app.layer_names.clone()
            } else {
                app.application_layout_editor_layer_names()
            };
            assert_eq!(names[0], "Restored", "Escape must cancel clearing");
            assert_eq!(packets.requests().len(), writes);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cleared_pending_layer_name_commits_when_leaving_editor() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let mut app = name_review_app();
        let key = app.application_layout_device_key().unwrap();
        app.editing_layer = Some(0);
        app.editing_layer_layout_id =
            Some(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID.into());
        app.editing_layer_text = "  ".into();
        app.commit_pending_application_layout_edits();
        assert_eq!(
            app.app_settings.application_layouts[&key].layouts
                [crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID]
                .layer_names[0],
            ""
        );
        assert_eq!(app.editing_layer, None);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ui_cold_default_name_waits_for_deferred_load_and_exit() { cold_default_name_waits_for_deferred_load(false, false); }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test14_sm62_no_enter_cold_close() { cold_default_name_waits_for_deferred_load(true, true); }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_ui_cold_default_name_waits_for_deferred_load_and_exit() { cold_default_name_waits_for_deferred_load(true, false); }

    #[cfg(not(target_arch = "wasm32"))]
    fn cold_default_name_waits_for_deferred_load(sm62: bool, close_without_enter: bool) {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let ctx = egui::Context::default();
        let mut app = name_review_device_app(sm62);
        // Staged connects already load native L0; L1 exercises the cold queue.
        app.selected_layer = 1;
        let layer = app.selected_layer;
        app.deferred_device_load = DeferredDeviceLoadState::staged(DeferredDeviceLoadContext {
            json: std::sync::Arc::new(serde_json::json!({})),
            supported_qmk_settings: std::sync::Arc::new(vec![200 + layer as u16]),
            definition_fingerprint: 7, layer_count:16, rows:if sm62 {10} else {1}, cols:if sm62 {7} else {1}, encoder_count:0,
            macro_count:0, macro_memory_bytes:Some(0), tap_dance_count:0, combo_count:0,
            key_override_count:0, alt_repeat_count:0, modules_supported:false, touchpad_supported:false,
            bluetooth_supported:false, layer_leds_supported:false, rgb_supported:false, lighting_mode:None,
            supports_rmk_native_key_actions:false, supports_universal_numero: false, supports_universal_symbols:false,
            supports_universal_russian_letters:false, supports_rmk_native_combo_output:false,
            supports_rmk_native_tap_dance_actions:false, supports_rmk_combo_layers:false,
        });
        let (hid, packets) = crate::hid::HidDevice::test_device();
        let (release, gate) = std::sync::mpsc::channel();
        let gate = std::sync::Mutex::new(gate);
        let store = std::sync::Arc::new(std::sync::Mutex::new("Factory".to_owned()));
        let firmware = store.clone();
        let mut blocked = false;
        packets.respond_by(move |request| {
            if request[0] == 0x12 {
                if !blocked { gate.lock().unwrap().recv_timeout(std::time::Duration::from_secs(3)).unwrap(); blocked=true; }
                let mut response=[0;32]; response[..4].copy_from_slice(&request[..4]);
                Some(Ok(response))
            } else if request[..2] == [0xfe,0x0b] {
                let end = request[4..].iter().position(|b| *b==0).unwrap_or(28);
                *firmware.lock().unwrap() = String::from_utf8(request[4..4+end].to_vec()).unwrap();
                Some(Ok([0;32]))
            } else if request[..2] == [0xfe,0x0a] {
                let name = firmware.lock().unwrap(); let mut response=[0;32];
                response[1..1+name.len()].copy_from_slice(name.as_bytes()); Some(Ok(response))
            } else { None }
        });
        app.hid_device = Some(hid.into());
        if close_without_enter {
            let pos = egui::pos2(550.0, 50.0);
            name_review_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                name_review_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {
                    pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE,
                }]);
            }
            name_review_frame(&mut app, &ctx, vec![]);
            assert_eq!(app.editing_layer, Some(layer));
            name_review_frame(&mut app, &ctx, vec![egui::Event::Key {
                key: egui::Key::A, physical_key: None, pressed: true, repeat: false,
                modifiers: egui::Modifiers { command: true, ctrl: true, ..Default::default() },
            }]);
            name_review_frame(&mut app, &ctx, vec![egui::Event::Text("Cold".into())]);
            app.app_settings.close_to_tray_behavior = CloseToTrayBehavior::Close;
            app.app_settings.minimize_to_tray_on_close = false;
            let mut input = egui::RawInput::default();
            input.viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
            let output = crate::app::discard_texture_deltas(ctx.run_ui(input, |_ui| app.handle_close_to_tray(&ctx)));
            assert!(app.editing_layer.is_none());
            assert!(output.viewport_output[&egui::ViewportId::ROOT].commands.contains(&egui::ViewportCommand::CancelClose));
        } else {
            name_review_rename(&mut app, &ctx, "Cold");
        }
        assert!(app.defer_exit_until_hid_write_returns(&ctx));
        app.poll_vial_hid_task(&ctx);
        assert_eq!(
            app.pending_layer_name_writes.len(),
            1,
            "cold rename was discarded"
        );
        assert!(
            app.vial_hid_task_active(),
            "pending name must schedule its required cold read"
        );
        app.poll_vial_hid_task(&ctx);
        assert_eq!(
            app.pending_layer_name_writes.len(),
            1,
            "Loading must retain rename"
        );
        app.finish_deferred_exit_after_hid_write(&ctx);
        assert!(app.exit_after_hid_write);
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while app.hid_write_lifecycle_busy() {
            app.tick_layout_import_lifecycle_for_test(&ctx, 1.0);
            assert!(
                std::time::Instant::now() < deadline,
                "cold name drain deadlocked: {}",
                app.status_msg
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        app.finish_deferred_exit_after_hid_write(&ctx);
        assert!(!app.exit_after_hid_write);
        assert_eq!(*store.lock().unwrap(),"Cold");
        assert_eq!(app.layer_names[layer],"Cold");
        assert!(packets.requests().iter().any(|r| r[..4]==[0xfe,0x0b,200 + layer as u8,0]));
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn name_review_drain(app: &mut EntropyApp, ctx: &egui::Context) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while app.hid_write_lifecycle_busy() {
            app.poll_vial_hid_task(ctx);
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ui_unconfirmed_name_is_read_before_other_name_publication() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for mismatch in [false, true] {
            let ctx = egui::Context::default();
            let mut app = name_review_app();
            let (hid, packets) = crate::hid::HidDevice::test_device();
            let store = std::sync::Arc::new(std::sync::Mutex::new(vec!["Old".to_owned(); 16]));
            let invalid = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let firmware = store.clone();
            let fail = invalid.clone();
            packets.respond_by(move |r| {
                let layer = u16::from_le_bytes([r[2], r[3]]).wrapping_sub(200) as usize;
                if layer >= 16 {
                    return None;
                }
                if r[..2] == [0xfe, 0x0b] {
                    let end = r[4..].iter().position(|b| *b == 0).unwrap_or(28);
                    firmware.lock().unwrap()[layer] =
                        String::from_utf8(r[4..4 + end].to_vec()).unwrap();
                    Some(Ok([0; 32]))
                } else if r[..2] == [0xfe, 0x0a] {
                    if fail.load(std::sync::atomic::Ordering::SeqCst) {
                        return Some(Ok(if mismatch { [0; 32] } else { [0xff; 32] }));
                    }
                    let store = firmware.lock().unwrap();
                    let mut response = [0; 32];
                    response[1..1 + store[layer].len()].copy_from_slice(store[layer].as_bytes());
                    Some(Ok(response))
                } else {
                    None
                }
            });
            app.hid_device = Some(hid.into());
            app.layer_names[0] = "Old".into();
            app.layout.as_mut().unwrap().layer_names[0] = "Old".into();
            name_review_rename(&mut app, &ctx, "Changed");
            name_review_drain(&mut app, &ctx);
            assert_eq!(store.lock().unwrap()[0], "Changed");
            let before = packets.requests().len();
            for _ in 0..3 {
                app.poll_vial_hid_task(&ctx);
                name_review_frame(&mut app, &ctx, vec![]);
            }
            assert_eq!(
                packets.requests().len(),
                before,
                "repaint retried an uncertain write"
            );
            invalid.store(false, std::sync::atomic::Ordering::SeqCst);
            app.application_layout_editor_active = false;
            app.selected_layer = 1;
            name_review_rename(&mut app, &ctx, "Other");
            assert_eq!(
                crate::app::app_storage::load_layer_names("Macropad")[0],
                "Changed",
                "ordinary rename republished a stale firmware name"
            );
            assert_eq!(app.layout.as_ref().unwrap().layer_names[0], "Changed");
            assert_eq!(
                packets
                    .requests()
                    .iter()
                    .filter(|r| r[..4] == [0xfe, 0x0b, 200, 0])
                    .count(),
                1,
                "read-only recovery must not retry SET"
            );
            let key = app.application_layout_device_key().unwrap();
            assert_eq!(
                app.app_settings.application_layouts[&key].layouts
                    [crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID]
                    .layer_names[0],
                "Changed"
            );
            // Explicitly restoring the previously confirmed value still requires SET+GET.
            app.application_layout_editor_active = true;
            app.selected_layer = 0;
            name_review_rename(&mut app, &ctx, "Old");
            name_review_drain(&mut app, &ctx);
            assert_eq!(store.lock().unwrap()[0], "Old");
            assert_eq!(
                packets
                    .requests()
                    .iter()
                    .filter(|r| r[..4] == [0xfe, 0x0b, 200, 0])
                    .count(),
                2
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ui_same_layer_rename_preserves_explicit_edit_after_uncertainty_recovery() { same_layer_rename_after_uncertainty(false); }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_ui_same_layer_rename_preserves_explicit_edit_after_uncertainty_recovery() { same_layer_rename_after_uncertainty(true); }

    #[cfg(not(target_arch = "wasm32"))]
    fn same_layer_rename_after_uncertainty(sm62: bool) {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let ctx = egui::Context::default();
        let mut app = name_review_device_app(sm62);
        let (hid, packets) = crate::hid::HidDevice::test_device();
        let store = std::sync::Arc::new(std::sync::Mutex::new(vec!["Old".to_owned(); 16]));
        let invalid = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let firmware = store.clone();
        let fail = invalid.clone();
        packets.respond_by(move |r| {
            let layer = u16::from_le_bytes([r[2], r[3]]).wrapping_sub(200) as usize;
            if layer >= 16 {
                return None;
            }
            if r[..2] == [0xfe, 0x0b] {
                let end = r[4..].iter().position(|b| *b == 0).unwrap_or(28);
                firmware.lock().unwrap()[layer] =
                    String::from_utf8(r[4..4 + end].to_vec()).unwrap();
                Some(Ok([0; 32]))
            } else if r[..2] == [0xfe, 0x0a] {
                if fail.load(std::sync::atomic::Ordering::SeqCst) {
                    return Some(Ok([0xff; 32]));
                }
                let store = firmware.lock().unwrap();
                let mut response = [0; 32];
                response[1..1 + store[layer].len()].copy_from_slice(store[layer].as_bytes());
                Some(Ok(response))
            } else {
                None
            }
        });
        app.hid_device = Some(hid.into());
        app.layer_names[0] = "Old".into();
        app.layout.as_mut().unwrap().layer_names[0] = "Old".into();
        name_review_rename(&mut app, &ctx, "Changed");
        name_review_drain(&mut app, &ctx);
        assert_eq!(store.lock().unwrap()[0], "Changed");
        assert!(app.selected_layer_names_uncertain());
        let key = app.application_layout_device_key().unwrap();
        let draft = app.app_settings.application_layouts[&key].layouts
            [crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID]
            .clone();
        invalid.store(false, std::sync::atomic::Ordering::SeqCst);
        app.application_layout_editor_active = false;
        name_review_rename(&mut app, &ctx, "Other");
        assert_eq!(store.lock().unwrap()[0], "Other");
        assert_eq!(app.layer_names[0], "Other", "recovery overwrote the explicit same-layer edit");
        assert_eq!(crate::app::app_storage::load_layer_names(&app.current_device_name)[0], "Other");
        assert!(!app.selected_layer_names_uncertain());
        assert_eq!(
            app.app_settings.application_layouts[&key].layouts
                [crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID],
            draft,
            "firmware recovery must preserve the host profile draft"
        );
        app.selected_layer = 1;
        name_review_rename(&mut app, &ctx, "Next");
        assert_eq!(store.lock().unwrap()[0], "Other");
        assert_eq!(app.layer_names[0], "Other");
        assert_eq!(crate::app::app_storage::load_layer_names(&app.current_device_name)[0], "Other",
            "later ordinary publication restored the recovered old name");
        assert_eq!(packets.requests().iter().filter(|r| r[..4] == [0xfe,0x0b,200,0]).count(), 2,
            "recovery must not add an automatic SET retry");
        assert_eq!(crate::app::app_storage::load_app_settings().application_layouts[&key].layouts[crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID], draft);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ui_name_write_errors_use_selected_language() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for (language, prefix, reason) in [
            (
                crate::i18n::Language::Russian,
                "Имя слоя не сохранено",
                "данные слоя не готовы",
            ),
            (
                crate::i18n::Language::English,
                "Layer name not saved",
                "layer data is not ready",
            ),
        ] {
            let ctx = egui::Context::default();
            let mut app = name_review_app();
            app.app_settings.language = language;
            let (hid, _) = crate::hid::HidDevice::test_device();
            app.hid_device = Some(hid.into());
            name_review_rename(&mut app, &ctx, "Test");
            app.deferred_device_load
                .set_layer_status(0, DeferredLoadStatus::Failed("read error".into()));
            app.poll_vial_hid_task(&ctx);
            assert_eq!(app.status_msg, format!("{prefix}: {reason}"));
            for (en, ru) in [
                ("device changed", "устройство изменилось"),
                ("device is busy", "устройство занято"),
                ("device is disconnected", "устройство отключено"),
                ("layer data is not ready", "данные слоя не готовы"),
                (
                    "layer name readback mismatch",
                    "прочитанное имя слоя не совпадает",
                ),
            ] {
                let expected = if language == crate::i18n::Language::Russian {
                    ru
                } else {
                    en
                };
                assert_eq!(
                    super::super::vial_hid_task::layer_name_write_error(language, en),
                    format!("{prefix}: {expected}")
                );
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ui_uncertain_name_gui_export_rechecks_before_writing() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let ctx = egui::Context::default();
        let mut app = name_review_app();
        let (hid, packets) = crate::hid::HidDevice::test_device();
        packets.respond_by(|r| {
            if r[..2] == [0xfe, 0x0b] {
                Some(Ok([0; 32]))
            } else if r[..2] == [0xfe, 0x0a] {
                Some(Ok([0xff; 32]))
            } else {
                None
            }
        });
        app.hid_device = Some(hid.into());
        name_review_rename(&mut app, &ctx, "Draft");
        name_review_drain(&mut app, &ctx);
        let before = packets
            .requests()
            .iter()
            .filter(|r| r[..2] == [0xfe, 0x0a])
            .count();
        let path =
            std::env::temp_dir().join(format!("name-export-{}.entlayout", std::process::id()));
        std::fs::write(&path, b"preserve existing export").unwrap();
        app.write_entlayout_export(&path);
        assert!(
            packets
                .requests()
                .iter()
                .filter(|r| r[..2] == [0xfe, 0x0a])
                .count()
                > before,
            "GUI export consumed uncertainty without a fresh read"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"preserve existing export");
        assert!(app.entlayout_export_json().unwrap().is_err());
        assert_eq!(
            packets
                .requests()
                .iter()
                .filter(|r| r[..2] == [0xfe, 0x0b])
                .count(),
            1
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ui_successive_name_edits_keep_newer_draft_across_delayed_completion() { successive_name_edits(false); }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sm62_ui_successive_name_edits_keep_newer_draft_across_delayed_completion() { successive_name_edits(true); }

    #[cfg(not(target_arch = "wasm32"))]
    fn successive_name_edits(sm62: bool) {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        for cancel_generation in [false,true] {
            let ctx=egui::Context::default(); let mut app=name_review_device_app(sm62);
            let (hid,packets)=crate::hid::HidDevice::test_device();
            let store=std::sync::Arc::new(std::sync::Mutex::new(String::new()));
            let firmware=store.clone();
            let (release,gate)=std::sync::mpsc::channel();let gate=std::sync::Mutex::new(Some(gate));
            let (entered,waiting)=std::sync::mpsc::channel();
            packets.respond_by(move |r| {
                if r[..2] == [0xfe, 0x0b] {
                    let end = r[4..].iter().position(|b| *b == 0).unwrap_or(28);
                    *firmware.lock().unwrap() = String::from_utf8(r[4..4 + end].to_vec()).unwrap();
                    Some(Ok([0; 32]))
                } else if r[..2] == [0xfe, 0x0a] {
                    if let Some(gate) = gate.lock().unwrap().take() {
                        entered.send(()).unwrap();
                        gate.recv_timeout(std::time::Duration::from_secs(3))
                            .unwrap();
                    }
                    let name = firmware.lock().unwrap();
                    let mut response = [0; 32];
                    response[1..1 + name.len()].copy_from_slice(name.as_bytes());
                    Some(Ok(response))
                } else {
                    None
                }
            });
            app.hid_device = Some(hid.into());
            name_review_rename(&mut app, &ctx, "First");
            app.poll_vial_hid_task(&ctx);
            waiting
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap();
            name_review_rename(&mut app, &ctx, "Second");
            let key = app.application_layout_device_key().unwrap();
            let (unrelated, unrelated_packets) = crate::hid::HidDevice::test_device();
            if cancel_generation {
                app.connection_generation += 1;
                app.current_keyboard_id = Some(0x99);
                app.hid_device = Some(unrelated.into());
            }
            release.send(()).unwrap();
            name_review_drain(&mut app, &ctx);
            assert_eq!(
                app.app_settings.application_layouts[&key].layouts
                    [crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID]
                    .layer_names[0],
                "Second"
            );
            assert!(unrelated_packets.requests().is_empty());
            if cancel_generation {
                assert_eq!(*store.lock().unwrap(), "First");
                assert_ne!(
                    app.layer_names[0], "First",
                    "stale completion published to new generation"
                );
            } else {
                assert_eq!(*store.lock().unwrap(), "Second");
                assert_eq!(app.layer_names[0], "Second");
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn pending_cold_name_without_owner_terminates_visibly() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let ctx = egui::Context::default();
        let mut app = name_review_app();
        app.deferred_device_load
            .set_layer_status(0, DeferredLoadStatus::Loading);
        app.rename_application_layout_layer(
            crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID,
            0,
            "Draft".into(),
        );
        assert!(app.defer_exit_until_hid_write_returns(&ctx));
        app.poll_vial_hid_task(&ctx);
        assert!(
            app.pending_layer_name_writes.is_empty(),
            "orphaned loading state blocked exit forever"
        );
        assert!(!app.status_msg.is_empty());
        app.finish_deferred_exit_after_hid_write(&ctx);
        assert!(!app.exit_after_hid_write);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn uncertainty_is_scoped_to_device_not_shared_model_uid() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let ctx = egui::Context::default();
        let mut app = name_review_app();
        for fail in [true, false] {
            let (hid, packets) = crate::hid::HidDevice::test_device();
            packets.respond_by(move |r| {
                if r[..2] == [0xfe, 0x0b] {
                    Some(Ok([0; 32]))
                } else if r[..2] == [0xfe, 0x0a] {
                    let mut response = [0; 32];
                    response[1..6].copy_from_slice(b"Draft");
                    Some(Ok(if fail { [0xff; 32] } else { response }))
                } else {
                    None
                }
            });
            if !fail {
                let mut other = app.device_manager.devices()[0].clone();
                other.path = "another-unit".into();
                app.device_manager.replace_devices(vec![other]);
                app.connection_generation += 1;
                mark_application_layout_protocol_supported(&mut app);
            }
            app.hid_device = Some(hid.into());
            app.rename_application_layout_layer(
                crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID,
                0,
                "Draft".into(),
            );
            name_review_drain(&mut app, &ctx);
        }
        assert_eq!(
            app.uncertain_layer_names.len(),
            1,
            "another unit's success discarded first unit's uncertainty"
        );
        assert_ne!(app.uncertain_layer_names[0].device.path, "another-unit");
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
                crate::app::discard_texture_deltas(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1100.0, 800.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| app.draw_layout_layer_switcher_and_hints(ui, 0.0, 0.0, 52.0),
                ))
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
                    crate::app::discard_texture_deltas(ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1100.0, 800.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| app.draw_layout_layer_switcher_and_hints(ui, 0.0, 0.0, 52.0),
                    ))
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
            crate::app::discard_texture_deltas(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.draw_layout_layer_switcher_and_hints(ui, 0.0, 0.0, 52.0),
            ))
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
        app.hid_device = Some(hid.into());
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
