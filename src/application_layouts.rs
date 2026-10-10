use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const APPLICATION_LAYOUT_CONTROL_COUNT: usize = 15;
pub(crate) const APPLICATION_LAYOUT_KEY_COUNT: usize = 13;
pub(crate) const APPLICATION_LAYOUT_LAYER_COUNT: usize = 16;
pub(crate) const APPLICATION_LAYOUT_UNSET_KEYCODE: u16 = u16::MAX;
pub(crate) const DEFAULT_APPLICATION_LAYOUT_ID: &str = "default";
pub(crate) const APPLICATION_LAYOUT_PROTOCOL_VERSION: u8 = 7;
pub(crate) const APPLICATION_LAYOUT_STACK_SLOTS: usize = 4;
pub(crate) const APPLICATION_LAYOUT_STACK_NAME_BYTES: usize = 12;
pub(crate) const APPLICATION_LAYOUT_NAME_BYTES: usize = 22;

/// UI grouping for application layouts; never sent to firmware.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApplicationLayoutCategory {
    Browsers,
    Development,
    Graphics,
    Video,
    Audio,
    Communication,
    Other,
}

impl ApplicationLayoutCategory {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Browsers => "browsers",
            Self::Development => "development",
            Self::Graphics => "graphics",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Communication => "communication",
            Self::Other => "other",
        }
    }

    pub(crate) const ALL: [Self; 7] = [
        Self::Browsers,
        Self::Development,
        Self::Graphics,
        Self::Video,
        Self::Audio,
        Self::Communication,
        Self::Other,
    ];

    pub(crate) fn for_preset_id(id: &str) -> Self {
        match id {
            "google_chrome" | "firefox" => Self::Browsers,
            "visual_studio_code" | "visual_studio" | "intellij_idea" | "pycharm" => {
                Self::Development
            }
            "blender" | "figma" | "krita" | "adobe_photoshop" | "adobe_illustrator" => {
                Self::Graphics
            }
            "obs_studio" | "streamlabs_desktop" | "davinci_resolve" | "adobe_premiere_pro"
            | "capcut" => Self::Video,
            "audacity" | "vlc" => Self::Audio,
            "discord" => Self::Communication,
            _ => Self::Other,
        }
    }
}

pub(crate) fn application_layout_category_for_executable(
    executable: &str,
) -> ApplicationLayoutCategory {
    static PRESET_CATEGORIES: std::sync::OnceLock<Vec<(&'static str, ApplicationLayoutCategory)>> =
        std::sync::OnceLock::new();
    PRESET_CATEGORIES
        .get_or_init(|| {
            builtin_application_layout_presets()
                .iter()
                .map(|preset| {
                    (
                        preset.executable,
                        ApplicationLayoutCategory::for_preset_id(preset.id),
                    )
                })
                .collect()
        })
        .iter()
        .find(|(known, _)| executables_match(executable, known))
        .map(|(_, category)| *category)
        .unwrap_or(ApplicationLayoutCategory::Other)
}

const MOD_CTRL: u16 = 0x0100;
const MOD_SHIFT: u16 = 0x0200;
const MOD_ALT: u16 = 0x0400;
const MOD_GUI: u16 = 0x0800;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ApplicationLayoutPreset {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) executable: &'static str,
    pub(crate) identities: &'static [&'static str],
    pub(crate) summary: &'static str,
    pub(crate) summary_ru: &'static str,
    layers: Vec<ApplicationLayoutPresetLayer>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ApplicationLayoutPresetLayer {
    name: &'static str,
    keycodes: [u16; APPLICATION_LAYOUT_CONTROL_COUNT],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShortcutPlatform {
    MacOs,
    WindowsLinux,
}

// Display transfers already occupy 0xC0..=0xD6 in the upstream firmware.
// Keep application-layout traffic in its own non-overlapping command range.
pub(crate) const HID_APPLICATION_LAYOUT_BEGIN: u8 = 0xE1;
pub(crate) const HID_APPLICATION_LAYOUT_KEYCODES: u8 = 0xE2;
pub(crate) const HID_APPLICATION_LAYOUT_COMMIT: u8 = 0xE3;
pub(crate) const HID_APPLICATION_LAYOUT_LAYER_NAME: u8 = 0xE4;
pub(crate) const HID_APPLICATION_LAYOUT_KEEPALIVE: u8 = 0xE5;
pub(crate) const HID_APPLICATION_LAYOUT_VISUALS: u8 = 0xE7;
pub(crate) const HID_APPLICATION_LAYOUT_ENCODER_STACK: u8 = 0xE9;
pub(crate) const HID_APPLICATION_LAYOUT_DEACTIVATE: u8 = 0xEA;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DetectedApplication {
    pub(crate) executable: String,
    /// Stable identities reported by launchers and window systems for the
    /// same application (desktop id, Exec, StartupWMClass, process, app_id).
    #[serde(default)]
    pub(crate) identities: Vec<String>,
    #[serde(default)]
    pub(crate) display_name: String,
    #[serde(default)]
    pub(crate) window_title: String,
}

impl DetectedApplication {
    pub(crate) fn label(&self) -> String {
        let name = if self.display_name.trim().is_empty() {
            self.executable.trim()
        } else {
            self.display_name.trim()
        };
        if self.window_title.trim().is_empty() || self.window_title.trim() == name {
            name.to_owned()
        } else {
            format!("{name} — {}", self.window_title.trim())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EncoderStackAction {
    pub(crate) name: String,
    pub(crate) counter_clockwise: u16,
    pub(crate) clockwise: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ApplicationLayout {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) executable: String,
    /// All known launcher/window identities captured when the application is
    /// selected. This keeps matching data-driven instead of app-specific.
    #[serde(default)]
    pub(crate) application_identities: Vec<String>,
    #[serde(default)]
    pub(crate) title_contains: String,
    /// Explicit UI category; absent for legacy layouts and auto-categorized presets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) category: Option<ApplicationLayoutCategory>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) custom_category_id: Option<String>,
    #[serde(default = "default_true")]
    pub(crate) automatic_switching: bool,
    /// Compatibility seed for application-layouts v1. New builds persist
    /// `layers`; the old single-layer keycodes are read once during normalize.
    #[serde(rename = "keycodes", default = "default_keycodes")]
    #[serde(skip_serializing)]
    legacy_keycodes: [u16; APPLICATION_LAYOUT_CONTROL_COUNT],
    #[serde(default)]
    pub(crate) layers: Vec<[u16; APPLICATION_LAYOUT_CONTROL_COUNT]>,
    #[serde(default = "default_layer_names")]
    pub(crate) layer_names: Vec<String>,
    /// Empty = legacy encoder behavior; otherwise exactly 2–4 actions per layer.
    #[serde(default)]
    pub(crate) encoder_stacks: Vec<Vec<EncoderStackAction>>,
    #[serde(default = "default_revision")]
    pub(crate) revision: u32,
}

impl ApplicationLayout {
    fn default_layout() -> Self {
        Self {
            id: DEFAULT_APPLICATION_LAYOUT_ID.to_owned(),
            name: "Default".to_owned(),
            executable: String::new(),
            application_identities: Vec::new(),
            title_contains: String::new(),
            category: None,
            custom_category_id: None,
            automatic_switching: false,
            legacy_keycodes: default_keycodes(),
            layers: default_layer_keycodes(),
            layer_names: default_layer_names(),
            encoder_stacks: vec![Vec::new(); APPLICATION_LAYOUT_LAYER_COUNT],
            revision: 1,
        }
    }

    fn normalize(&mut self) -> bool {
        let mut changed = false;
        let identities = normalized_identity_values(
            std::iter::once(self.executable.as_str())
                .chain(self.application_identities.iter().map(String::as_str)),
        );
        if self.application_identities != identities {
            self.application_identities = identities;
            changed = true;
        }
        if self.layers.is_empty() {
            self.layers = default_layer_keycodes();
            self.layers[0] = self.legacy_keycodes;
            changed = true;
        }
        if self.layers.len() < APPLICATION_LAYOUT_LAYER_COUNT {
            self.layers.extend(
                (self.layers.len()..APPLICATION_LAYOUT_LAYER_COUNT).map(|_| default_keycodes()),
            );
            changed = true;
        } else if self.layers.len() > APPLICATION_LAYOUT_LAYER_COUNT {
            self.layers.truncate(APPLICATION_LAYOUT_LAYER_COUNT);
            changed = true;
        }
        if self.layer_names.len() < APPLICATION_LAYOUT_LAYER_COUNT {
            let defaults = default_layer_names();
            self.layer_names.extend(
                (self.layer_names.len()..APPLICATION_LAYOUT_LAYER_COUNT)
                    .map(|index| defaults[index].clone()),
            );
            changed = true;
        } else if self.layer_names.len() > APPLICATION_LAYOUT_LAYER_COUNT {
            self.layer_names.truncate(APPLICATION_LAYOUT_LAYER_COUNT);
            changed = true;
        }
        if self.encoder_stacks.len() < APPLICATION_LAYOUT_LAYER_COUNT {
            self.encoder_stacks
                .resize_with(APPLICATION_LAYOUT_LAYER_COUNT, Vec::new);
            changed = true;
        } else if self.encoder_stacks.len() > APPLICATION_LAYOUT_LAYER_COUNT {
            self.encoder_stacks.truncate(APPLICATION_LAYOUT_LAYER_COUNT);
            changed = true;
        }
        for stack in &mut self.encoder_stacks {
            if stack.len() == 1 || stack.len() > APPLICATION_LAYOUT_STACK_SLOTS {
                stack.clear();
                changed = true;
            }
            for (index, action) in stack.iter_mut().enumerate() {
                let name = if action.name.trim().is_empty() {
                    format!("Action {}", index + 1)
                } else {
                    action.name.trim().to_owned()
                };
                let canonical = std::str::from_utf8(application_layout_stack_name_bytes(&name))
                    .unwrap_or("")
                    .to_owned();
                if action.name != canonical {
                    action.name = canonical;
                    changed = true;
                }
                for keycode in [&mut action.counter_clockwise, &mut action.clockwise] {
                    if *keycode == APPLICATION_LAYOUT_UNSET_KEYCODE || *keycode == 1 {
                        *keycode = 0;
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    pub(crate) fn matches(&self, application: &DetectedApplication) -> bool {
        if self.id == DEFAULT_APPLICATION_LAYOUT_ID || self.executable.trim().is_empty() {
            return false;
        }
        application_identities_match(
            std::iter::once(self.executable.as_str())
                .chain(self.application_identities.iter().map(String::as_str)),
            std::iter::once(application.executable.as_str())
                .chain(application.identities.iter().map(String::as_str)),
        ) && (self.title_contains.trim().is_empty()
            || application
                .window_title
                .to_lowercase()
                .contains(&self.title_contains.trim().to_lowercase()))
    }

    pub(crate) fn set_keycode(&mut self, layer: usize, control: usize, keycode: u16) -> bool {
        let Some(slot) = self
            .layers
            .get_mut(layer)
            .and_then(|keycodes| keycodes.get_mut(control))
        else {
            return false;
        };
        if *slot == keycode {
            return false;
        }
        *slot = keycode;
        self.bump_revision();
        true
    }

    pub(crate) fn set_encoder_stack(
        &mut self,
        layer: usize,
        actions: Vec<EncoderStackAction>,
    ) -> bool {
        if layer >= APPLICATION_LAYOUT_LAYER_COUNT
            || !(actions.is_empty()
                || (2..=APPLICATION_LAYOUT_STACK_SLOTS).contains(&actions.len()))
        {
            return false;
        }
        if self.encoder_stacks.len() != APPLICATION_LAYOUT_LAYER_COUNT {
            self.normalize();
        }
        let actions = actions
            .into_iter()
            .enumerate()
            .map(|(index, mut action)| {
                if action.name.trim().is_empty() {
                    action.name = format!("Action {}", index + 1);
                }
                action.name =
                    std::str::from_utf8(application_layout_stack_name_bytes(action.name.trim()))
                        .unwrap_or("")
                        .to_owned();
                if action.counter_clockwise == APPLICATION_LAYOUT_UNSET_KEYCODE
                    || action.counter_clockwise == 1
                {
                    action.counter_clockwise = 0;
                }
                if action.clockwise == APPLICATION_LAYOUT_UNSET_KEYCODE || action.clockwise == 1 {
                    action.clockwise = 0;
                }
                action
            })
            .collect::<Vec<_>>();
        if self.encoder_stacks[layer] == actions {
            return false;
        }
        self.encoder_stacks[layer] = actions;
        self.bump_revision();
        true
    }

    pub(crate) fn set_layer_name(&mut self, layer: usize, name: String) -> bool {
        let Some(slot) = self.layer_names.get_mut(layer) else {
            return false;
        };
        let name = if name.trim().is_empty() {
            default_layer_names()
                .get(layer)
                .cloned()
                .unwrap_or_else(|| format!("Layer {layer}"))
        } else {
            name.trim().to_owned()
        };
        if *slot == name {
            return false;
        }
        *slot = name;
        self.bump_revision();
        true
    }

    pub(crate) fn seed_unset_keycodes(
        &mut self,
        layers: [[u16; APPLICATION_LAYOUT_CONTROL_COUNT]; APPLICATION_LAYOUT_LAYER_COUNT],
    ) -> bool {
        let mut changed = false;
        self.normalize();
        for (target_layer, source_layer) in self.layers.iter_mut().zip(layers) {
            for (slot, source) in target_layer.iter_mut().zip(source_layer) {
                if *slot == APPLICATION_LAYOUT_UNSET_KEYCODE {
                    *slot = source;
                    changed = true;
                }
            }
        }
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub(crate) fn apply_preset(&mut self, preset: &ApplicationLayoutPreset) -> bool {
        self.normalize();
        let mut changed = false;
        for (index, preset_layer) in preset.layers.iter().enumerate() {
            let Some(layer) = self.layers.get_mut(index) else {
                break;
            };
            if *layer != preset_layer.keycodes {
                *layer = preset_layer.keycodes;
                changed = true;
            }
            if self.layer_names[index] != preset_layer.name {
                self.layer_names[index] = preset_layer.name.to_owned();
                changed = true;
            }
        }
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub(crate) fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DeviceApplicationLayouts {
    #[serde(default = "default_layouts")]
    pub(crate) layouts: BTreeMap<String, ApplicationLayout>,
    /// Compatibility bridge for application-layouts v1/v2, where one global
    /// switch controlled every application. It is consumed once by normalize.
    #[serde(default, rename = "automatic_switching", skip_serializing)]
    legacy_automatic_switching: Option<bool>,
    #[serde(default = "default_layout_id")]
    pub(crate) editor_layout_id: String,
    #[serde(default = "default_layout_id")]
    pub(crate) active_layout_id: String,
    /// Automatically return to Default when the focused window does not match
    /// a configured application. A known application with automatic switching
    /// disabled preserves the current manually selected layout.
    #[serde(
        default = "default_true",
        rename = "automatically_return_to_default",
        alias = "unknown_window_uses_default"
    )]
    pub(crate) automatically_return_to_default: bool,
    /// Master gate: manual layout selection and editing remain available when off.
    #[serde(default = "default_true")]
    pub(crate) automatic_switching_enabled: bool,
    /// One-time import marker; user deletion and edits are never undone.
    #[serde(default)]
    pub(crate) builtin_presets_provisioned: bool,
    /// One-time catalog migration; never re-create a profile a user deleted.
    #[serde(default)]
    builtin_presets_catalog_revision: u8,
    /// Auto-created but not yet linked: detector unavailable or app not installed.
    #[serde(default)]
    pub(crate) pending_builtin_auto_bind: BTreeMap<String, String>,
    /// Stable category IDs and user-visible names, per device. Built-in names
    /// here override translations; custom IDs are allocated by create_category.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) category_names: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub(crate) removed_categories: BTreeSet<String>,
    #[serde(default = "default_next_id")]
    next_category_id: u32,
    #[serde(default = "default_next_id")]
    next_id: u32,
}

impl Default for DeviceApplicationLayouts {
    fn default() -> Self {
        Self {
            layouts: default_layouts(),
            legacy_automatic_switching: None,
            editor_layout_id: default_layout_id(),
            active_layout_id: default_layout_id(),
            automatically_return_to_default: true,
            automatic_switching_enabled: true,
            builtin_presets_provisioned: false,
            builtin_presets_catalog_revision: 0,
            pending_builtin_auto_bind: BTreeMap::new(),
            category_names: BTreeMap::new(),
            removed_categories: BTreeSet::new(),
            next_category_id: default_next_id(),
            next_id: default_next_id(),
        }
    }
}

impl DeviceApplicationLayouts {
    pub(crate) fn category_exists(&self, id: &str) -> bool {
        !self.removed_categories.contains(id)
            && (ApplicationLayoutCategory::ALL
                .iter()
                .any(|category| category.id() == id)
                || self.category_names.contains_key(id))
    }

    pub(crate) fn category_id_for_layout(&self, layout: &ApplicationLayout) -> String {
        let id = layout.custom_category_id.as_deref().unwrap_or_else(|| {
            layout
                .category
                .unwrap_or_else(|| application_layout_category_for_executable(&layout.executable))
                .id()
        });
        if self.category_exists(id) {
            id.to_owned()
        } else {
            "other".to_owned()
        }
    }

    pub(crate) fn create_category(&mut self, name: &str) -> Option<String> {
        let name = name.trim();
        if name.is_empty()
            || name.chars().count() > 48
            || self
                .category_names
                .values()
                .any(|other| other.eq_ignore_ascii_case(name))
        {
            return None;
        }
        let id = loop {
            let id = format!("custom:{}", self.next_category_id);
            self.next_category_id = self.next_category_id.checked_add(1)?;
            if !self.category_names.contains_key(&id) && !self.removed_categories.contains(&id) {
                break id;
            }
        };
        self.category_names.insert(id.clone(), name.to_owned());
        Some(id)
    }

    pub(crate) fn rename_category(&mut self, id: &str, name: &str) -> bool {
        let name = name.trim();
        if !self.category_exists(id)
            || name.is_empty()
            || name.chars().count() > 48
            || self
                .category_names
                .iter()
                .any(|(other_id, other)| other_id != id && other.eq_ignore_ascii_case(name))
        {
            return false;
        }
        if self.category_names.get(id).is_some_and(|old| old == name) {
            return false;
        }
        self.category_names.insert(id.to_owned(), name.to_owned());
        true
    }

    pub(crate) fn remove_category(&mut self, id: &str) -> bool {
        if id == "other" || !self.category_exists(id) {
            return false;
        }
        let affected = self
            .layouts
            .iter()
            .filter(|(key, _)| key.as_str() != DEFAULT_APPLICATION_LAYOUT_ID)
            .filter(|(_, layout)| self.category_id_for_layout(layout) == id)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in affected {
            self.set_layout_category_id(&key, "other");
        }
        self.category_names.remove(id);
        if ApplicationLayoutCategory::ALL
            .iter()
            .any(|category| category.id() == id)
        {
            self.removed_categories.insert(id.to_owned());
        }
        true
    }

    pub(crate) fn set_layout_category_id(&mut self, layout_id: &str, category_id: &str) -> bool {
        if layout_id == DEFAULT_APPLICATION_LAYOUT_ID || !self.category_exists(category_id) {
            return false;
        }
        let Some(layout) = self.layouts.get_mut(layout_id) else {
            return false;
        };
        if layout.custom_category_id.as_deref() == Some(category_id) {
            return false;
        }
        layout.custom_category_id = Some(category_id.to_owned());
        layout.bump_revision();
        true
    }

    pub(crate) fn normalize(&mut self) -> bool {
        let mut changed = false;
        if let Some(automatic_switching) = self.legacy_automatic_switching.take() {
            for layout in self.layouts.values_mut() {
                if layout.id != DEFAULT_APPLICATION_LAYOUT_ID {
                    layout.automatic_switching = automatic_switching;
                }
            }
            changed = true;
        }
        if !self.layouts.contains_key(DEFAULT_APPLICATION_LAYOUT_ID) {
            self.layouts.insert(
                DEFAULT_APPLICATION_LAYOUT_ID.to_owned(),
                ApplicationLayout::default_layout(),
            );
            changed = true;
        }
        if let Some(default) = self.layouts.get_mut(DEFAULT_APPLICATION_LAYOUT_ID) {
            if default.category.take().is_some() {
                default.bump_revision();
                changed = true;
            }
            if default.custom_category_id.take().is_some() {
                default.bump_revision();
                changed = true;
            }
            let stock_names = default_layer_names();
            let legacy_names = legacy_default_layer_names();
            let mut migrated_names = false;
            for (index, name) in default.layer_names.iter_mut().enumerate() {
                if legacy_names.get(index).is_some_and(|legacy| name == legacy) {
                    *name = stock_names[index].clone();
                    migrated_names = true;
                }
            }
            if migrated_names {
                default.bump_revision();
                changed = true;
            }
            if default.name != "Default"
                || !default.executable.is_empty()
                || !default.application_identities.is_empty()
                || !default.title_contains.is_empty()
                || default.automatic_switching
            {
                default.name = "Default".to_owned();
                default.executable.clear();
                default.application_identities.clear();
                default.title_contains.clear();
                default.automatic_switching = false;
                default.bump_revision();
                changed = true;
            }
        }
        // The map key is the stable profile identity. Older or partially
        // written settings could contain a stale `layout.id`; UI actions used
        // that inner value and could therefore edit/delete a neighbouring
        // profile. Repair the invariant before any profile is exposed.
        for (stable_id, layout) in &mut self.layouts {
            if layout.id != *stable_id {
                layout.id = stable_id.clone();
                changed = true;
            }
            changed |= layout.normalize();
        }
        // Older X11 builds could expose their launch name as WM_CLASS, so a
        // profile for Entropy may have been saved with the misleading name Vial.
        // Keep the real Vial application and any other user-named profile intact.
        if !self.layout_name_exists("Entropy", None) {
            let legacy_entropy = self
                .layouts
                .values()
                .find(|layout| {
                    layout.name == "Vial" && normalize_executable(&layout.executable) == "entropy"
                })
                .map(|layout| layout.id.clone());
            if let Some(id) = legacy_entropy {
                changed |= self.rename_layout(&id, "Entropy");
            }
        }
        let next_unused_id = self
            .layouts
            .keys()
            .filter_map(|id| id.rsplit_once('_')?.1.parse::<u32>().ok())
            .max()
            .map(|value| value.saturating_add(1))
            .unwrap_or(1);
        if self.next_id < next_unused_id {
            self.next_id = next_unused_id;
            changed = true;
        }
        if !self.layouts.contains_key(&self.editor_layout_id) {
            self.editor_layout_id = default_layout_id();
            changed = true;
        }
        if !self.layouts.contains_key(&self.active_layout_id) {
            self.active_layout_id = default_layout_id();
            changed = true;
        }
        changed
    }

    pub(crate) fn editor_layout(&self) -> Option<&ApplicationLayout> {
        self.layouts.get(&self.editor_layout_id)
    }

    pub(crate) fn active_layout(&self) -> Option<&ApplicationLayout> {
        self.layouts
            .get(&self.active_layout_id)
            .or_else(|| self.layouts.get(DEFAULT_APPLICATION_LAYOUT_ID))
    }

    pub(crate) fn create_for_application(&mut self, application: &DetectedApplication) -> String {
        self.create_for_application_named(application, None, "")
    }

    pub(crate) fn create_for_application_named(
        &mut self,
        application: &DetectedApplication,
        requested_name: Option<&str>,
        title_contains: &str,
    ) -> String {
        let id = self.unique_id(&application.executable);
        let default_name = if application.display_name.trim().is_empty() {
            application.executable.trim()
        } else {
            application.display_name.trim()
        };
        let name = requested_name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(default_name);
        let seed_layers = self
            .layouts
            .get(DEFAULT_APPLICATION_LAYOUT_ID)
            .map(|layout| layout.layers.clone())
            .unwrap_or_else(default_layer_keycodes);
        let seed_names = self
            .layouts
            .get(DEFAULT_APPLICATION_LAYOUT_ID)
            .map(|layout| layout.layer_names.clone())
            .unwrap_or_else(default_layer_names);
        self.layouts.insert(
            id.clone(),
            ApplicationLayout {
                id: id.clone(),
                name: if name.is_empty() {
                    "Application".to_owned()
                } else {
                    name.to_owned()
                },
                executable: application.executable.trim().to_owned(),
                application_identities: normalized_identity_values(
                    std::iter::once(application.executable.as_str())
                        .chain(application.identities.iter().map(String::as_str)),
                ),
                title_contains: title_contains.trim().to_owned(),
                category: None,
                custom_category_id: None,
                automatic_switching: true,
                legacy_keycodes: default_keycodes(),
                layers: seed_layers,
                layer_names: seed_names,
                encoder_stacks: vec![Vec::new(); APPLICATION_LAYOUT_LAYER_COUNT],
                revision: 1,
            },
        );
        self.editor_layout_id = id.clone();
        id
    }

    pub(crate) fn provision_builtin_presets(
        &mut self,
        installed: Option<&std::collections::BTreeSet<String>>,
    ) -> bool {
        let mut changed = false;
        const CATALOG_REVISION: u8 = 6;
        if !self.builtin_presets_provisioned
            || self.builtin_presets_catalog_revision < CATALOG_REVISION
        {
            let initial_provision = !self.builtin_presets_provisioned;
            let default_layers = self
                .layouts
                .get(DEFAULT_APPLICATION_LAYOUT_ID)
                .map(|layout| layout.layers.clone())
                .unwrap_or_else(default_layer_keycodes);
            let default_names = self
                .layouts
                .get(DEFAULT_APPLICATION_LAYOUT_ID)
                .map(|layout| layout.layer_names.clone())
                .unwrap_or_else(default_layer_names);
            // Retire only the untouched, automatically provisioned Writer
            // profile. A customized Writer layout belongs to the user.
            if let Some(writer_id) = self
                .layouts
                .values()
                .find(|layout| {
                    layout.name == "LibreOffice Writer"
                        && executables_match(&layout.executable, "libreoffice-writer")
                        && legacy_writer_is_stock(layout, &default_layers, &default_names)
                })
                .map(|layout| layout.id.clone())
            {
                self.remove(&writer_id);
                self.pending_builtin_auto_bind.remove(&writer_id);
            }
            let platform = if cfg!(target_os = "macos") {
                ShortcutPlatform::MacOs
            } else {
                ShortcutPlatform::WindowsLinux
            };
            for preset in all_application_layout_presets_for(platform)
                .into_iter()
                .filter(|preset| retired_builtin_preset(preset.id))
            {
                let ids = self
                    .layouts
                    .values()
                    .filter(|layout| {
                        retired_preset_is_stock(layout, &preset, &default_layers, &default_names)
                    })
                    .map(|layout| layout.id.clone())
                    .collect::<Vec<_>>();
                for id in ids {
                    self.remove(&id);
                }
            }
            let editor = self.editor_layout_id.clone();
            let active = self.active_layout_id.clone();
            for preset in builtin_application_layout_presets() {
                // Existing built-ins may gain untouched layers, but deleted
                // profiles are not silently recreated. Only genuinely new
                // presets are provisioned by this catalog revision.
                if !initial_provision {
                    if let Some(layout) = self.layouts.values_mut().find(|layout| {
                        layout.name == preset.name
                            && executables_match(&layout.executable, preset.executable)
                    }) {
                        // Firmware seeding fills the three reserved layer
                        // controls and untouched layers with the current QMK
                        // keymap. Compare those slots against Default, not
                        // against the all-UNSET serialization seed.
                        let first_is_stock =
                            preset_layer_is_stock(layout, 0, &preset.layers[0], &default_layers);
                        let configured_layers_are_stock = preset
                            .layers
                            .iter()
                            .enumerate()
                            .skip(1)
                            .all(|(index, layer)| {
                                preset_layer_is_stock(layout, index, layer, &default_layers)
                                    || (layout.layers[index] == default_layers[index]
                                        && layout.layer_names[index] == default_names[index]
                                        && layout.encoder_stacks[index].is_empty())
                            });
                        if first_is_stock && configured_layers_are_stock {
                            let mut enriched = false;
                            for (index, layer) in preset.layers.iter().enumerate().skip(1) {
                                if layout.layers[index] == default_layers[index]
                                    && layout.layer_names[index] == default_names[index]
                                {
                                    layout.layers[index] = layer.keycodes;
                                    layout.layer_names[index] = layer.name.to_owned();
                                    enriched = true;
                                }
                            }
                            if enriched {
                                layout.bump_revision();
                            }
                        }
                        continue;
                    }
                    let is_new = matches!(
                        preset.id,
                        "google_chrome"
                            | "adobe_premiere_pro"
                            | "adobe_illustrator"
                            | "visual_studio"
                            | "intellij_idea"
                            | "pycharm"
                            | "discord"
                    );
                    if !is_new
                        && !(preset.id == "adobe_photoshop"
                            && self.builtin_presets_catalog_revision < 2)
                    {
                        continue;
                    }
                }
                let application = DetectedApplication {
                    executable: preset.executable.to_owned(),
                    identities: preset
                        .identities
                        .iter()
                        .map(|value| (*value).to_owned())
                        .collect(),
                    display_name: preset.name.to_owned(),
                    window_title: String::new(),
                };
                let already_linked = self.layouts.values().any(|layout| {
                    layout.id != DEFAULT_APPLICATION_LAYOUT_ID
                        && (executables_match(&layout.executable, &application.executable)
                            || application_identities_match(
                                std::iter::once(layout.executable.as_str()).chain(
                                    layout.application_identities.iter().map(String::as_str),
                                ),
                                std::iter::once(application.executable.as_str())
                                    .chain(application.identities.iter().map(String::as_str)),
                            ))
                });
                if already_linked {
                    continue;
                }
                let id = self.create_from_preset(&preset);
                if let Some(layout) = self.layouts.get_mut(&id) {
                    layout.automatic_switching =
                        installed.is_some_and(|found| found.contains(preset.id));
                }
                if !installed.is_some_and(|found| found.contains(preset.id)) {
                    self.pending_builtin_auto_bind
                        .insert(id, preset.id.to_owned());
                }
            }
            self.editor_layout_id = if self.layouts.contains_key(&editor) {
                editor
            } else {
                default_layout_id()
            };
            self.active_layout_id = if self.layouts.contains_key(&active) {
                active
            } else {
                default_layout_id()
            };
            self.builtin_presets_provisioned = true;
            self.builtin_presets_catalog_revision = CATALOG_REVISION;
            changed = true;
        }
        if let Some(installed) = installed {
            let pending = std::mem::take(&mut self.pending_builtin_auto_bind);
            for (id, preset_id) in pending {
                if !installed.contains(&preset_id) {
                    self.pending_builtin_auto_bind.insert(id, preset_id);
                    continue;
                }
                if let Some(layout) = self.layouts.get_mut(&id) {
                    if !layout.automatic_switching {
                        layout.automatic_switching = true;
                        layout.bump_revision();
                    }
                }
                changed = true;
            }
        }
        changed
    }

    pub(crate) fn create_from_preset(&mut self, preset: &ApplicationLayoutPreset) -> String {
        let name = (1..)
            .map(|suffix| {
                if suffix == 1 {
                    preset.name.to_owned()
                } else {
                    format!("{} ({suffix})", preset.name)
                }
            })
            .find(|name| !self.layout_name_exists(name, None))
            .unwrap_or_else(|| preset.name.to_owned());
        let application = DetectedApplication {
            executable: preset.executable.to_owned(),
            identities: preset
                .identities
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            display_name: preset.name.to_owned(),
            window_title: String::new(),
        };
        let id = self.create_for_application_named(&application, Some(&name), "");
        if let Some(layout) = self.layouts.get_mut(&id) {
            layout.apply_preset(preset);
        }
        id
    }

    pub(crate) fn layout_name_exists(&self, name: &str, excluding_id: Option<&str>) -> bool {
        let name = name.trim();
        !name.is_empty()
            && self.layouts.values().any(|layout| {
                Some(layout.id.as_str()) != excluding_id
                    && layout.name.trim().eq_ignore_ascii_case(name)
            })
    }

    pub(crate) fn rename_layout(&mut self, id: &str, name: &str) -> bool {
        let name = name.trim();
        if id == DEFAULT_APPLICATION_LAYOUT_ID
            || name.is_empty()
            || self.layout_name_exists(name, Some(id))
        {
            return false;
        }
        let Some(layout) = self.layouts.get_mut(id) else {
            return false;
        };
        if layout.name == name {
            return false;
        }
        layout.name = name.to_owned();
        layout.bump_revision();
        true
    }

    pub(crate) fn set_layout_category(
        &mut self,
        id: &str,
        category: ApplicationLayoutCategory,
    ) -> bool {
        if id == DEFAULT_APPLICATION_LAYOUT_ID || !self.category_exists(category.id()) {
            return false;
        }
        let Some(layout) = self.layouts.get_mut(id) else {
            return false;
        };
        if layout.category == Some(category) && layout.custom_category_id.is_none() {
            return false;
        }
        layout.category = Some(category);
        layout.custom_category_id = None;
        layout.bump_revision();
        true
    }

    pub(crate) fn application_rule_exists(
        &self,
        application: &DetectedApplication,
        title_contains: &str,
        excluding_id: Option<&str>,
    ) -> bool {
        let title_contains = title_contains.trim();
        self.layouts.values().any(|layout| {
            layout.id != DEFAULT_APPLICATION_LAYOUT_ID
                && Some(layout.id.as_str()) != excluding_id
                && layout
                    .title_contains
                    .trim()
                    .eq_ignore_ascii_case(title_contains)
                && application_identities_match(
                    std::iter::once(layout.executable.as_str())
                        .chain(layout.application_identities.iter().map(String::as_str)),
                    std::iter::once(application.executable.as_str())
                        .chain(application.identities.iter().map(String::as_str)),
                )
        })
    }

    pub(crate) fn update_application_rule(
        &mut self,
        id: &str,
        application: &DetectedApplication,
        name: &str,
        title_contains: &str,
    ) -> bool {
        if id == DEFAULT_APPLICATION_LAYOUT_ID {
            return false;
        }
        let Some(layout) = self.layouts.get_mut(id) else {
            return false;
        };
        let identities = normalized_identity_values(
            std::iter::once(application.executable.as_str())
                .chain(application.identities.iter().map(String::as_str)),
        );
        let name = name.trim();
        let name = if name.is_empty() {
            application.display_name.trim()
        } else {
            name
        };
        let name = if name.is_empty() { "Application" } else { name };
        let executable = application.executable.trim();
        let title_contains = title_contains.trim();
        let rule_changed = layout.executable != executable
            || layout.application_identities != identities
            || layout.title_contains != title_contains;
        if layout.name == name && !rule_changed {
            return false;
        }
        layout.name = name.to_owned();
        layout.executable = executable.to_owned();
        layout.application_identities = identities;
        layout.title_contains = title_contains.to_owned();
        layout.bump_revision();
        if rule_changed {
            self.pending_builtin_auto_bind.remove(id);
        }
        true
    }

    pub(crate) fn remove(&mut self, id: &str) -> bool {
        if id == DEFAULT_APPLICATION_LAYOUT_ID || self.layouts.remove(id).is_none() {
            return false;
        }
        if self.editor_layout_id == id {
            self.editor_layout_id = default_layout_id();
        }
        if self.active_layout_id == id {
            self.active_layout_id = default_layout_id();
        }
        self.pending_builtin_auto_bind.remove(id);
        true
    }

    pub(crate) fn resolve(&self, application: Option<&DetectedApplication>) -> String {
        if !self.automatic_switching_enabled {
            return if self.layouts.contains_key(&self.active_layout_id) {
                self.active_layout_id.clone()
            } else {
                default_layout_id()
            };
        }
        let active_or_default = || {
            if self.layouts.contains_key(&self.active_layout_id) {
                self.active_layout_id.clone()
            } else {
                default_layout_id()
            }
        };
        let matched = application.and_then(|application| {
            self.layouts
                .values()
                .filter(|layout| layout.matches(application))
                .max_by(|left, right| {
                    let rank = |layout: &&ApplicationLayout| {
                        (
                            !layout.title_contains.trim().is_empty(),
                            layout.title_contains.len(),
                            application_identity_match_score(
                                std::iter::once(layout.executable.as_str()).chain(
                                    layout.application_identities.iter().map(String::as_str),
                                ),
                                std::iter::once(application.executable.as_str())
                                    .chain(application.identities.iter().map(String::as_str)),
                            ),
                        )
                    };
                    rank(&left)
                        .cmp(&rank(&right))
                        // Stable fallback for legacy configurations that already contain
                        // ambiguous duplicate rules. Lower ids win deterministically.
                        .then_with(|| right.id.cmp(&left.id))
                })
                .map(|layout| {
                    if layout.automatic_switching {
                        layout.id.clone()
                    } else {
                        active_or_default()
                    }
                })
        });
        matched.unwrap_or_else(|| {
            if self.automatically_return_to_default {
                default_layout_id()
            } else {
                active_or_default()
            }
        })
    }

    /// Enrich layouts created by older builds with identities from the
    /// current launcher/window catalog. Upgrades then work without forcing
    /// users to delete and re-add every application.
    pub(crate) fn enrich_application_identities(
        &mut self,
        applications: &[DetectedApplication],
    ) -> bool {
        let mut changed = false;
        for layout in self.layouts.values_mut().filter(|layout| {
            layout.id != DEFAULT_APPLICATION_LAYOUT_ID && !layout.executable.trim().is_empty()
        }) {
            let Some(application) = applications.iter().max_by_key(|application| {
                application_identity_match_score(
                    std::iter::once(layout.executable.as_str())
                        .chain(layout.application_identities.iter().map(String::as_str)),
                    std::iter::once(application.executable.as_str())
                        .chain(application.identities.iter().map(String::as_str)),
                )
            }) else {
                continue;
            };
            if !application_identities_match(
                std::iter::once(layout.executable.as_str())
                    .chain(layout.application_identities.iter().map(String::as_str)),
                std::iter::once(application.executable.as_str())
                    .chain(application.identities.iter().map(String::as_str)),
            ) {
                continue;
            }
            let identities = normalized_identity_values(
                std::iter::once(layout.executable.as_str())
                    .chain(layout.application_identities.iter().map(String::as_str))
                    .chain(std::iter::once(application.executable.as_str()))
                    .chain(application.identities.iter().map(String::as_str)),
            );
            if layout.application_identities != identities {
                layout.application_identities = identities;
                layout.bump_revision();
                changed = true;
            }
        }
        changed
    }

    fn unique_id(&mut self, executable: &str) -> String {
        let base = executable
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .trim_matches('_')
            .to_owned();
        let base = if base.is_empty() {
            "application"
        } else {
            &base
        };
        loop {
            let suffix = self.next_id;
            self.next_id = self.next_id.wrapping_add(1).max(1);
            let candidate = format!("{base}_{suffix}");
            if !self.layouts.contains_key(&candidate) {
                return candidate;
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ApplicationLayoutSnapshot {
    pub(crate) active: bool,
    pub(crate) revision: u32,
    pub(crate) name: String,
    pub(crate) layer_names: [String; APPLICATION_LAYOUT_LAYER_COUNT],
    pub(crate) layers: [[u16; APPLICATION_LAYOUT_CONTROL_COUNT]; APPLICATION_LAYOUT_LAYER_COUNT],
    pub(crate) visuals: [[u8; APPLICATION_LAYOUT_CONTROL_COUNT]; APPLICATION_LAYOUT_LAYER_COUNT],
    pub(crate) encoder_stacks: [Vec<EncoderStackAction>; APPLICATION_LAYOUT_LAYER_COUNT],
}

impl ApplicationLayoutSnapshot {
    pub(crate) fn from_layout(layout: &ApplicationLayout) -> Self {
        let preset = builtin_application_layout_presets()
            .into_iter()
            .find(|preset| {
                !layout.executable.trim().is_empty()
                    && executables_match(&layout.executable, preset.executable)
            });
        let visuals = std::array::from_fn(|layer| {
            std::array::from_fn(|control| {
                let Some(preset) = &preset else { return 0 };
                let Some(preset_layer) = preset.layers.get(layer) else {
                    return 0;
                };
                let Some(configured) = layout.layers.get(layer) else {
                    return 0;
                };
                let keycode = preset_layer.keycodes[control];
                if keycode == 0 || configured[control] != keycode {
                    return 0;
                }
                crate::action_icons::preset_visual(preset.id, layer, control)
            })
        });
        Self {
            active: true,
            revision: layout.revision,
            visuals,
            name: layout.name.trim().to_owned(),
            layer_names: std::array::from_fn(|layer| {
                layout
                    .layer_names
                    .get(layer)
                    .cloned()
                    .unwrap_or_else(|| format!("Layer {layer}"))
            }),
            encoder_stacks: std::array::from_fn(|layer| {
                layout
                    .encoder_stacks
                    .get(layer)
                    .cloned()
                    .unwrap_or_default()
            }),
            layers: std::array::from_fn(|layer| {
                layout
                    .layers
                    .get(layer)
                    .copied()
                    .unwrap_or_else(default_keycodes)
            }),
        }
    }

    pub(crate) fn inactive() -> Self {
        Self {
            active: false,
            revision: 0,
            name: String::new(),
            layer_names: std::array::from_fn(|_| String::new()),
            layers: [default_keycodes(); APPLICATION_LAYOUT_LAYER_COUNT],
            visuals: [[0; APPLICATION_LAYOUT_CONTROL_COUNT]; APPLICATION_LAYOUT_LAYER_COUNT],
            encoder_stacks: std::array::from_fn(|_| Vec::new()),
        }
    }

    pub(crate) fn packets(&self) -> Vec<[u8; 32]> {
        let mut begin = [0u8; 32];
        begin[0] = HID_APPLICATION_LAYOUT_BEGIN;
        begin[1] = APPLICATION_LAYOUT_PROTOCOL_VERSION;
        begin[2] = u8::from(self.active);
        begin[3..7].copy_from_slice(&self.revision.to_le_bytes());
        begin[7] = APPLICATION_LAYOUT_CONTROL_COUNT as u8;
        begin[8] = APPLICATION_LAYOUT_LAYER_COUNT as u8;
        let name = application_layout_name_bytes(&self.name);
        begin[9] = name.len() as u8;
        begin[10..10 + name.len()].copy_from_slice(name);

        let mut packets = vec![begin];
        for (layer, keycodes) in self.layers.iter().enumerate() {
            for (start, count) in [
                (0, APPLICATION_LAYOUT_KEY_COUNT),
                (APPLICATION_LAYOUT_KEY_COUNT, 2),
            ] {
                let mut packet = [0u8; 32];
                packet[0] = HID_APPLICATION_LAYOUT_KEYCODES;
                packet[1] = APPLICATION_LAYOUT_PROTOCOL_VERSION;
                packet[2] = layer as u8;
                packet[3] = start as u8;
                packet[4] = count as u8;
                for (index, keycode) in keycodes[start..start + count].iter().enumerate() {
                    let offset = 5 + index * 2;
                    packet[offset..offset + 2].copy_from_slice(&keycode.to_le_bytes());
                }
                packets.push(packet);
            }
        }

        for layer in 0..APPLICATION_LAYOUT_LAYER_COUNT {
            let mut packet = [0u8; 32];
            packet[0] = HID_APPLICATION_LAYOUT_VISUALS;
            packet[1] = APPLICATION_LAYOUT_PROTOCOL_VERSION;
            packet[2] = layer as u8;
            packet[3] = APPLICATION_LAYOUT_CONTROL_COUNT as u8;
            packet[4..4 + APPLICATION_LAYOUT_CONTROL_COUNT].copy_from_slice(&self.visuals[layer]);
            packets.push(packet);
        }

        for (layer, actions) in self.encoder_stacks.iter().enumerate() {
            let count = if (2..=APPLICATION_LAYOUT_STACK_SLOTS).contains(&actions.len()) {
                actions.len()
            } else {
                0
            };
            for slot in 0..APPLICATION_LAYOUT_STACK_SLOTS {
                let mut packet = [0u8; 32];
                packet[0] = HID_APPLICATION_LAYOUT_ENCODER_STACK;
                packet[1] = APPLICATION_LAYOUT_PROTOCOL_VERSION;
                packet[2] = layer as u8;
                packet[3] = slot as u8;
                packet[4] = count as u8;
                if slot < count {
                    let action = &actions[slot];
                    let name = application_layout_stack_name_bytes(&action.name);
                    packet[5] = name.len() as u8;
                    packet[6..8].copy_from_slice(&action.counter_clockwise.to_le_bytes());
                    packet[8..10].copy_from_slice(&action.clockwise.to_le_bytes());
                    packet[10..10 + name.len()].copy_from_slice(name);
                }
                packets.push(packet);
            }
        }

        for (layer, layer_name) in self.layer_names.iter().enumerate() {
            let mut packet = [0u8; 32];
            let encoded = application_layout_name_bytes(layer_name);
            packet[0] = HID_APPLICATION_LAYOUT_LAYER_NAME;
            packet[1] = APPLICATION_LAYOUT_PROTOCOL_VERSION;
            packet[2] = layer as u8;
            packet[3] = encoded.len() as u8;
            packet[4..4 + encoded.len()].copy_from_slice(encoded);
            packets.push(packet);
        }

        let mut commit = [0u8; 32];
        commit[0] = HID_APPLICATION_LAYOUT_COMMIT;
        commit[1] = APPLICATION_LAYOUT_PROTOCOL_VERSION;
        commit[2] = u8::from(self.active);
        commit[3..7].copy_from_slice(&self.revision.to_le_bytes());
        commit[7..9].copy_from_slice(
            &crc16_snapshot(
                &self.layers,
                &self.visuals,
                &self.encoder_stacks,
                name,
                &self.layer_names,
            )
            .to_le_bytes(),
        );
        packets.push(commit);
        packets
    }

    pub(crate) fn deactivate_packet() -> [u8; 32] {
        let mut packet = [0u8; 32];
        packet[0] = HID_APPLICATION_LAYOUT_DEACTIVATE;
        packet[1] = APPLICATION_LAYOUT_PROTOCOL_VERSION;
        packet
    }

    pub(crate) fn keepalive_packet(&self) -> [u8; 32] {
        let mut packet = [0u8; 32];
        packet[0] = HID_APPLICATION_LAYOUT_KEEPALIVE;
        packet[1] = APPLICATION_LAYOUT_PROTOCOL_VERSION;
        packet[2] = u8::from(self.active);
        packet[3..7].copy_from_slice(&self.revision.to_le_bytes());
        packet
    }
}

fn application_layout_stack_name_bytes(name: &str) -> &[u8] {
    let mut end = name.len().min(APPLICATION_LAYOUT_STACK_NAME_BYTES);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name.as_bytes()[..end]
}

fn application_layout_name_bytes(name: &str) -> &[u8] {
    let mut end = name.len().min(APPLICATION_LAYOUT_NAME_BYTES);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name.as_bytes()[..end]
}

pub(crate) fn normalize_executable(value: &str) -> String {
    let value = value.trim().replace('\\', "/");
    let file = value
        .rsplit('/')
        .next()
        .unwrap_or(&value)
        .to_ascii_lowercase();
    let normalized = file
        .strip_suffix(".exe")
        .unwrap_or(&file)
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>();
    normalized
}

pub(crate) fn executables_match(left: &str, right: &str) -> bool {
    application_identities_match([left], [right])
}

pub(crate) fn application_identities_match<'a>(
    left: impl IntoIterator<Item = &'a str>,
    right: impl IntoIterator<Item = &'a str>,
) -> bool {
    application_identity_match_score(left, right) > 0
}

pub(crate) fn application_identity_match_score<'a>(
    left: impl IntoIterator<Item = &'a str>,
    right: impl IntoIterator<Item = &'a str>,
) -> usize {
    let left = left.into_iter().collect::<Vec<_>>();
    let right = right.into_iter().collect::<Vec<_>>();
    let left_bundle_ids = authoritative_bundle_ids(left.iter().copied());
    let right_bundle_ids = authoritative_bundle_ids(right.iter().copied());

    // NSWorkspace gives us a stable bundle identifier. When both records have
    // one, paths, localized names and helper-process aliases must never make
    // two different applications equal (for example Finder and another
    // process containing the word `finder`). Fall back to aliases only when a
    // stable identifier is absent on at least one side.
    if !left_bundle_ids.is_empty() && !right_bundle_ids.is_empty() {
        return left_bundle_ids
            .iter()
            .filter(|bundle_id| right_bundle_ids.contains(bundle_id))
            .map(String::len)
            .max()
            .unwrap_or(0);
    }

    let left = normalized_identity_values(left);
    let right = normalized_identity_values(right);
    left.iter()
        .filter(|identity| right.contains(identity))
        .map(String::len)
        .max()
        .unwrap_or(0)
}

fn authoritative_bundle_ids<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut bundle_ids = values
        .into_iter()
        .filter_map(|value| {
            let value = value.trim();
            if value.is_empty()
                || value
                    .chars()
                    .any(|character| matches!(character, '/' | '\\' | ' '))
                || value.ends_with(".exe")
            {
                return None;
            }
            let parts = value.split('.').collect::<Vec<_>>();
            if parts.len() < 3
                || !matches!(
                    parts[0].to_ascii_lowercase().as_str(),
                    "com" | "org" | "net" | "io" | "app" | "ru"
                )
                || parts.iter().any(|part| {
                    part.is_empty()
                        || !part
                            .chars()
                            .all(|character| character.is_ascii_alphanumeric() || character == '-')
                })
            {
                return None;
            }
            Some(value.to_ascii_lowercase())
        })
        .collect::<Vec<_>>();
    bundle_ids.sort();
    bundle_ids.dedup();
    bundle_ids
}

pub(crate) fn normalized_identity_values<'a>(
    values: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let mut identities = values
        .into_iter()
        .flat_map(application_identity_aliases)
        .collect::<Vec<_>>();
    identities.sort();
    identities.dedup();
    identities
}

fn application_identity_aliases(value: &str) -> Vec<String> {
    let value = value.trim().replace('\\', "/");
    let file = value.rsplit('/').next().unwrap_or(&value);
    let file = file
        .strip_suffix(".desktop")
        .or_else(|| file.strip_suffix(".exe"))
        .unwrap_or(file);
    let parts = file
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return Vec::new();
    }

    let mut aliases = vec![parts.concat()];
    let mut semantic = parts.as_slice();
    if semantic
        .first()
        .is_some_and(|part| matches!(part.as_str(), "com" | "org" | "net" | "io" | "app"))
    {
        semantic = &semantic[1..];
        if !semantic.is_empty() {
            aliases.push(semantic.concat());
        }
    }
    while semantic.last().is_some_and(|part| {
        matches!(
            part.as_str(),
            "desktop"
                | "application"
                | "app"
                | "launcher"
                | "server"
                | "client"
                | "bin"
                | "binary"
                | "wrapper"
                | "process"
        )
    }) {
        semantic = &semantic[..semantic.len() - 1];
        if !semantic.is_empty() {
            aliases.push(semantic.concat());
        }
    }
    if semantic.len() >= 2
        && semantic[semantic.len() - 2] == "url"
        && semantic[semantic.len() - 1] == "handler"
    {
        let base = &semantic[..semantic.len() - 2];
        if !base.is_empty() {
            aliases.push(base.concat());
            semantic = base;
        }
    }
    if semantic.len() > 1
        && semantic.last().is_some_and(|part| {
            !matches!(
                part.as_str(),
                "handler"
                    | "helper"
                    | "desktop"
                    | "application"
                    | "app"
                    | "launcher"
                    | "mac"
                    | "macos"
                    | "windows"
                    | "linux"
            )
        })
    {
        aliases.push(semantic.last().cloned().unwrap_or_default());
    }
    // Older builds could persist launcher/runtime implementation names as
    // identities. Values such as `desktop`, `electron`, or `snap` are shared
    // by unrelated applications and turn one rule (notably Telegram) into a
    // catch-all. Filter them both for new records and while normalizing old
    // settings so a poisoned profile repairs itself on the next load.
    aliases.retain(|alias| alias.len() >= 2 && !generic_application_identity(alias));
    aliases.sort();
    aliases.dedup();
    aliases
}

fn generic_application_identity(value: &str) -> bool {
    matches!(
        value,
        "app"
            | "application"
            | "bin"
            | "binary"
            | "chromium"
            | "client"
            | "desktop"
            | "electron"
            | "flatpak"
            | "handler"
            | "helper"
            | "launcher"
            | "linux"
            | "mac"
            | "macos"
            | "process"
            | "server"
            | "snap"
            | "wayland"
            | "windows"
            | "wrapper"
            | "x11"
    )
}

pub(crate) fn builtin_application_layout_presets() -> Vec<ApplicationLayoutPreset> {
    let platform = if cfg!(target_os = "macos") {
        ShortcutPlatform::MacOs
    } else {
        ShortcutPlatform::WindowsLinux
    };
    builtin_application_layout_presets_for(platform)
}

fn builtin_application_layout_presets_for(
    platform: ShortcutPlatform,
) -> Vec<ApplicationLayoutPreset> {
    all_application_layout_presets_for(platform)
        .into_iter()
        .filter(|preset| !retired_builtin_preset(preset.id))
        .collect()
}

fn retired_builtin_preset(id: &str) -> bool {
    matches!(
        id,
        "vlc" | "capcut" | "krita" | "davinci_resolve" | "streamlabs_desktop"
    )
}

fn all_application_layout_presets_for(platform: ShortcutPlatform) -> Vec<ApplicationLayoutPreset> {
    let primary = match platform {
        ShortcutPlatform::MacOs => MOD_GUI,
        ShortcutPlatform::WindowsLinux => MOD_CTRL,
    };
    let browser_back = match platform {
        ShortcutPlatform::MacOs => chord(primary, 0x002F),
        ShortcutPlatform::WindowsLinux => chord(MOD_ALT, 0x0050),
    };
    let browser_forward = match platform {
        ShortcutPlatform::MacOs => chord(primary, 0x0030),
        ShortcutPlatform::WindowsLinux => chord(MOD_ALT, 0x004F),
    };

    let mut presets = vec![
        preset(
            "obs_studio",
            "OBS Studio",
            "obs",
            &["obs64.exe", "obs32.exe", "com.obsproject.Studio"],
            "Safe source transform, layout and undo controls; streaming and recording are intentionally omitted.",
            "Безопасное управление трансформацией источников и отменой; запуск трансляции и записи намеренно исключён.",
            "Sources",
            [
                chord(primary, 0x001D),
                chord(primary | MOD_SHIFT, 0x001D),
                chord(primary, 0x0009),
                chord(primary, 0x0007),
                chord(primary, 0x0015),
                chord(primary, 0x0008),
                0x0050,
                0x0052,
                0x004F,
            ],
            chord(primary, 0x0008),
            0x0051,
            0x0052,
        ),
        preset(
            "visual_studio_code",
            "Visual Studio Code",
            "code",
            &["Code.exe", "Visual Studio Code", "com.microsoft.VSCode"],
            "Navigation, command palette, panels, rename, comments and debugging.",
            "Навигация, палитра команд, панели, переименование, комментарии и отладка.",
            "Coding",
            [
                chord(primary, 0x0013),
                chord(primary | MOD_SHIFT, 0x0013),
                chord(MOD_CTRL, 0x0035),
                chord(primary, 0x0005),
                chord(primary, 0x000D),
                chord(primary, 0x0038),
                0x003E,
                chord(MOD_SHIFT, 0x003E),
                0x003B,
            ],
            chord(primary | MOD_SHIFT, 0x0013),
            chord(MOD_CTRL | MOD_SHIFT, 0x002B),
            chord(MOD_CTRL, 0x002B),
        ),
        preset(
            "blender",
            "Blender",
            "blender",
            &["blender.exe", "org.blender.Blender"],
            "Core transform, mode, add, search, shading and undo controls.",
            "Основные команды трансформации, режима, добавления, поиска, отображения и отмены.",
            "Modeling",
            [
                0x000A,
                0x0015,
                0x0016,
                0x002B,
                chord(MOD_SHIFT, 0x0004),
                0x003C,
                0x001D,
                chord(primary, 0x001D),
                chord(primary | MOD_SHIFT, 0x001D),
            ],
            0x004A,
            0x0056,
            0x0057,
        ),
        preset(
            "davinci_resolve",
            "DaVinci Resolve",
            "resolve",
            &["Resolve.exe", "DaVinci Resolve", "com.blackmagic-design.DaVinciResolve"],
            "Stable edit-page transport, mark, selection, blade and undo controls.",
            "Стабильные команды монтажа: транспорт, метки, выбор, лезвие и отмена.",
            "Editing",
            [
                0x000D,
                0x000E,
                0x000F,
                0x000C,
                0x0012,
                0x0004,
                0x0005,
                0x002C,
                chord(primary, 0x001D),
            ],
            0x002C,
            0x0050,
            0x004F,
        ),
        preset(
            "figma",
            "Figma",
            "figma",
            &["Figma.exe", "com.figma.Desktop", "figma-linux"],
            "Tool selection, zoom-to-fit, zoom-to-selection, quick actions and undo.",
            "Инструменты, масштаб по экрану и выделению, быстрые действия и отмена.",
            "Design",
            [
                0x0019,
                0x0009,
                0x0015,
                0x0013,
                0x0017,
                chord(MOD_SHIFT, 0x001E),
                chord(MOD_SHIFT, 0x001F),
                chord(primary, 0x0038),
                chord(primary, 0x001D),
            ],
            chord(primary, 0x0038),
            chord(primary, 0x002D),
            chord(primary | MOD_SHIFT, 0x002E),
        ),
        preset(
            "krita",
            "Krita",
            "krita",
            &["krita.exe", "org.kde.krita"],
            "Painting, canvas view, save, undo/redo and encoder zoom.",
            "Рисование, управление холстом, сохранение, отмена/повтор и масштаб на энкодере.",
            "Painting",
            [
                0x0005,
                0x0008,
                0x0010,
                0x002B,
                0x0022,
                0x0019,
                chord(primary, 0x001D),
                chord(primary | MOD_SHIFT, 0x001D),
                chord(primary, 0x0016),
            ],
            0x0005,
            0x002D,
            chord(MOD_SHIFT, 0x002E),
        ),
        preset(
            "audacity",
            "Audacity",
            "audacity",
            &["Audacity.exe", "org.audacityteam.Audacity"],
            "Safe playback, navigation, looping, undo/redo and zoom; record is intentionally omitted.",
            "Безопасное воспроизведение, навигация, цикл, отмена и масштаб; запись намеренно исключена.",
            "Audio",
            [
                0x002C,
                0x0013,
                0x000D,
                0x000E,
                0x000F,
                0x004A,
                0x004D,
                chord(primary, 0x001D),
                chord(primary, 0x001C),
            ],
            chord(primary, 0x0009),
            chord(primary, 0x0020),
            chord(primary, 0x001E),
        ),
        preset(
            "vlc",
            "VLC media player",
            "vlc",
            &["vlc.exe", "org.videolan.VLC"],
            "Playback, fullscreen, mute, navigation, timing and speed controls.",
            "Воспроизведение, полный экран, звук, навигация, время и скорость.",
            "Playback",
            [
                0x002C,
                0x0009,
                0x0010,
                0x0016,
                0x0011,
                0x0013,
                0x0017,
                0x002F,
                0x0030,
            ],
            0x00A8,
            0x00AA,
            0x00A9,
        ),
        preset(
            "firefox",
            "Mozilla Firefox",
            "firefox",
            &["firefox.exe", "org.mozilla.firefox", "Firefox"],
            "Address/search, tabs, history, restore-tab and fullscreen without close/reload actions.",
            "Адрес и поиск, вкладки, история, восстановление вкладки и полный экран без закрытия и перезагрузки.",
            "Browsing",
            [
                chord(primary, 0x000F),
                chord(primary, 0x0017),
                chord(primary, 0x0009),
                browser_back,
                browser_forward,
                chord(primary | MOD_SHIFT, 0x0017),
                0x0044,
                chord(MOD_CTRL | MOD_SHIFT, 0x002B),
                chord(MOD_CTRL, 0x002B),
            ],
            chord(primary, 0x000F),
            0x004E,
            0x004B,
        ),
        preset(
            "adobe_photoshop",
            "Adobe Photoshop",
            "Photoshop",
            &["Photoshop.exe", "com.adobe.Photoshop", "Adobe Photoshop"],
            "Retouching tools, transform, brush size and common editing shortcuts.",
            "Ретушь: инструменты, трансформация, размер кисти и основные команды редактирования.",
            "Tools",
            [
                0x0019, // V: Move
                0x0005, // B: Brush
                0x0008, // E: Eraser
                0x0006, // C: Crop
                0x000F, // L: Lasso
                0x0010, // M: Marquee
                0x000C, // I: Eyedropper
                0x000D, // J: Healing
                0x0017, // T: Type
            ],
            chord(primary, 0x0017), // Free Transform
            0x002F, // Smaller brush
            0x0030, // Larger brush
        ),
    ];

    // Keep the established first two layers. The lower hardware row remains
    // available for layer switching on every preset layer.
    for preset in &mut presets {
        let layer = match preset.id {
            "obs_studio" => preset_layer(
                "Source order",
                [
                    chord(primary, 0x0006), // Copy source
                    chord(primary, 0x0019), // Paste source
                    chord(primary, 0x0052), // Raise source
                    chord(primary, 0x0051), // Lower source
                    chord(primary, 0x004A), // To top
                    chord(primary, 0x004D), // To bottom
                    chord(primary, 0x0016), // Stretch to screen
                    0x0051,                 // Nudge down
                    chord(primary, 0x0007), // Center to screen
                ],
                chord(primary, 0x0016),
                chord(primary, 0x0051),
                chord(primary, 0x0052),
            ),
            "visual_studio_code" => preset_layer(
                "Search",
                [
                    chord(primary, 0x0009),             // Find
                    chord(primary, 0x000B),             // Replace
                    chord(primary | MOD_SHIFT, 0x0009), // Find in files
                    chord(primary | MOD_SHIFT, 0x000B), // Replace in files
                    chord(primary, 0x000A),             // Go to line
                    chord(primary | MOD_SHIFT, 0x000A), // Source control
                    chord(primary, 0x0007),             // Add next occurrence
                    chord(MOD_ALT, 0x0052),             // Move line up
                    chord(MOD_ALT, 0x0051),             // Move line down
                ],
                chord(primary, 0x0009),
                chord(MOD_SHIFT, 0x003C),
                0x003C,
            ),
            "blender" => preset_layer(
                "Selection",
                [
                    0x0004,                   // Select all
                    chord(MOD_ALT, 0x0004),   // Select none
                    chord(MOD_CTRL, 0x000C),  // Invert selection
                    0x000B,                   // Hide selected
                    chord(MOD_SHIFT, 0x000B), // Hide unselected
                    chord(MOD_ALT, 0x000B),   // Reveal hidden
                    0x0017,                   // Toolbar
                    0x0011,                   // Sidebar
                    0x003B,                   // Rename (F2)
                ],
                0x0063,
                chord(MOD_CTRL, 0x004E),
                chord(MOD_CTRL, 0x004B),
            ),
            "davinci_resolve" => preset_layer(
                "Project",
                [
                    chord(primary, 0x0016),             // Save
                    chord(primary, 0x0006),             // Copy
                    chord(primary, 0x0019),             // Paste
                    chord(primary, 0x001D),             // Undo
                    chord(primary | MOD_SHIFT, 0x001D), // Redo
                    chord(primary, 0x0004),             // Select all
                    0x004A,                             // Start
                    0x004D,                             // End
                    0x0000,                             // Intentionally unassigned
                ],
                0x002C,
                0x0050,
                0x004F,
            ),
            "figma" => preset_layer(
                "Arrange",
                [
                    0x0012,                 // Ellipse
                    0x000F,                 // Line
                    0x000B,                 // Hand
                    0x000E,                 // Scale
                    chord(primary, 0x0004), // Select all
                    chord(primary, 0x0006), // Copy
                    chord(primary, 0x0019), // Paste
                    chord(primary, 0x0007), // Duplicate
                    chord(primary, 0x000A), // Group
                ],
                chord(primary | MOD_SHIFT, 0x000A),
                chord(primary, 0x002D),
                chord(primary | MOD_SHIFT, 0x002E),
            ),
            "krita" => preset_layer(
                "Files & colors",
                [
                    chord(primary, 0x0011),             // New
                    chord(primary, 0x0012),             // Open
                    chord(primary | MOD_SHIFT, 0x0016), // Save as
                    chord(primary, 0x0006),             // Copy
                    chord(primary, 0x0019),             // Paste
                    chord(primary, 0x0004),             // Select all
                    0x0007,                             // Reset colors
                    0x001B,                             // Swap colors
                    chord(primary, 0x001D),             // Undo
                ],
                chord(primary, 0x0016),
                0x002D,
                chord(MOD_SHIFT, 0x002E),
            ),
            "audacity" => preset_layer(
                "Clip editing",
                [
                    chord(primary, 0x001B),             // Cut
                    chord(primary, 0x0006),             // Copy
                    chord(primary, 0x0019),             // Paste
                    chord(primary, 0x0007),             // Duplicate selection
                    chord(primary, 0x0004),             // Select all
                    chord(primary, 0x0016),             // Save project
                    chord(primary, 0x0011),             // New project
                    chord(primary, 0x0012),             // Open
                    chord(primary | MOD_SHIFT, 0x0008), // Export audio
                ],
                chord(primary | MOD_SHIFT, 0x000C),
                0x004E,
                0x004B,
            ),
            "vlc" => preset_layer(
                "Seeking",
                [
                    chord(MOD_SHIFT, 0x0050), // Short jump back
                    chord(MOD_SHIFT, 0x004F), // Short jump forward
                    chord(MOD_ALT, 0x0050),   // Medium jump back
                    chord(MOD_ALT, 0x004F),   // Medium jump forward
                    chord(MOD_CTRL, 0x0050),  // Long jump back
                    chord(MOD_CTRL, 0x004F),  // Long jump forward
                    0x0008,                   // Next frame
                    0x0019,                   // Cycle subtitle track
                    0x0005,                   // Cycle audio track
                ],
                0x0008,
                chord(MOD_ALT, 0x0050),
                chord(MOD_ALT, 0x004F),
            ),
            "firefox" => preset_layer(
                "Reading",
                [
                    chord(primary, 0x0007),             // Bookmark page
                    chord(primary, 0x000D),             // Downloads
                    chord(primary, 0x000B),             // History
                    chord(primary | MOD_SHIFT, 0x0013), // Private window
                    chord(primary, 0x0016),             // Save page
                    chord(primary, 0x0018),             // View source
                    chord(primary | MOD_SHIFT, 0x0005), // Bookmarks toolbar
                    chord(primary, 0x0027),             // Reset zoom
                    chord(primary | MOD_SHIFT, 0x0012), // Bookmark manager
                ],
                chord(primary, 0x0027),
                chord(primary, 0x002D),
                chord(primary | MOD_SHIFT, 0x002E),
            ),
            "adobe_photoshop" => preset_layer(
                "Editing",
                [
                    chord(primary, 0x001D),             // Undo
                    chord(primary | MOD_SHIFT, 0x001D), // Redo
                    chord(primary, 0x000D),             // Duplicate layer
                    chord(primary, 0x0016),             // Save
                    chord(primary, 0x0007),             // Deselect
                    chord(primary, 0x0004),             // Select all
                    chord(primary, 0x0006),             // Copy
                    chord(primary, 0x0019),             // Paste
                    chord(primary, 0x0027),             // Fit on screen
                ],
                chord(primary, 0x0017),
                chord(primary, 0x002D),
                chord(primary | MOD_SHIFT, 0x002E),
            ),
            _ => unreachable!("all built-in presets have an extra layer"),
        };
        preset.layers.push(layer);
        let additional = match preset.id {
            "visual_studio_code" => Some(preset_layer(
                "Files & terminal",
                [
                    chord(primary, 0x0011),              // New file
                    chord(primary, 0x0012),              // Open file
                    chord(primary, 0x0016),              // Save
                    chord(primary | MOD_SHIFT, 0x0016),  // Save as
                    chord(MOD_CTRL, 0x0035),             // Terminal
                    chord(MOD_CTRL | MOD_SHIFT, 0x0035), // New terminal
                    chord(primary | MOD_SHIFT, 0x0008),  // Explorer
                    chord(primary | MOD_SHIFT, 0x0009),  // Search
                    chord(primary | MOD_SHIFT, 0x001B),  // Extensions
                ],
                chord(primary, 0x0016),
                chord(primary, 0x004B),
                chord(primary, 0x004E),
            )),
            "blender" => Some(preset_layer(
                "Views",
                [
                    0x0059, // Front (numpad 1)
                    0x005B, // Right (numpad 3)
                    0x005F, // Top (numpad 7)
                    0x005D, // Perspective/ortho (numpad 5)
                    0x0062, // Camera (numpad 0)
                    0x0063, // Frame selected (numpad decimal)
                    0x004A, // Frame all (Home)
                    0x003C, // Search (F3)
                    0x0011, // Sidebar
                ],
                0x0063,
                0x005A, // Numpad 2: orbit down
                0x0060, // Numpad 8: orbit up
            )),
            "davinci_resolve" => Some(preset_layer(
                "Timeline",
                [
                    0x0042,                   // Insert edit (F9)
                    0x0043,                   // Overwrite edit (F10)
                    0x0044,                   // Replace edit (F11)
                    0x0045,                   // Place on top (F12)
                    chord(primary, 0x0005),   // Blade at playhead
                    chord(MOD_SHIFT, 0x001D), // Fit timeline
                    chord(primary, 0x0015),   // Retime controls
                    0x004A,                   // Start of timeline
                    0x004D,                   // End of timeline
                ],
                0x002C,
                0x0050,
                0x004F,
            )),
            "figma" => Some(preset_layer(
                "Layers",
                [
                    chord(primary | MOD_SHIFT, 0x000A), // Ungroup
                    chord(primary | MOD_ALT, 0x000E),   // Create component
                    chord(primary | MOD_SHIFT, 0x000E), // Place image
                    chord(primary, 0x002F),             // Send backward
                    chord(primary, 0x0030),             // Bring forward
                    chord(primary | MOD_SHIFT, 0x002F), // Send to back
                    chord(primary | MOD_SHIFT, 0x0030), // Bring to front
                    chord(primary | MOD_SHIFT, 0x0012), // Outline stroke
                    chord(primary, 0x0015),             // Rename layer
                ],
                chord(primary, 0x000A),
                chord(primary, 0x002F),
                chord(primary, 0x0030),
            )),
            "adobe_photoshop" => Some(preset_layer(
                "Layers",
                [
                    chord(primary | MOD_SHIFT, 0x0011), // New layer
                    chord(primary, 0x000D),             // Duplicate layer
                    chord(primary, 0x000A),             // Group layers
                    chord(primary, 0x0008),             // Merge layers
                    chord(primary | MOD_SHIFT, 0x0008), // Merge visible
                    chord(primary | MOD_ALT, 0x000A),   // Create clipping mask
                    chord(primary | MOD_SHIFT, 0x000C), // Inverse selection
                    0x0040,                             // Layers panel (F7)
                    chord(primary | MOD_SHIFT, 0x0016), // Save as
                ],
                chord(primary, 0x000D),
                0x004B,
                0x004E,
            )),
            _ => None,
        };
        if let Some(layer) = additional {
            preset.layers.push(layer);
        }
    }
    presets.extend(popular_application_layout_presets(
        platform,
        primary,
        browser_back,
        browser_forward,
    ));
    // Product priority across audiences, not a claim of measured market share.
    presets.sort_by_key(|preset| match preset.id {
        "google_chrome" => 0,
        "visual_studio_code" => 1,
        "adobe_photoshop" => 2,
        "adobe_premiere_pro" => 3,
        "figma" => 6,
        "adobe_illustrator" => 7,
        "blender" => 8,
        "visual_studio" => 9,
        "intellij_idea" => 10,
        "pycharm" => 11,
        "obs_studio" => 12,
        "discord" => 14,
        "firefox" => 15,
        "audacity" => 17,
        _ => 19,
    });
    presets
}

// Additional audience-focused profiles. Keycodes are USB HID usage IDs; 0 is
// intentionally unassigned when an app has no portable default shortcut.
fn popular_application_layout_presets(
    platform: ShortcutPlatform,
    primary: u16,
    browser_back: u16,
    browser_forward: u16,
) -> Vec<ApplicationLayoutPreset> {
    let mac = platform == ShortcutPlatform::MacOs;
    let mut result = Vec::new();
    let mut add = |id,
                   name,
                   executable,
                   identities,
                   summary,
                   summary_ru,
                   layers: Vec<ApplicationLayoutPresetLayer>| {
        result.push(ApplicationLayoutPreset {
            id,
            name,
            executable,
            identities,
            summary,
            summary_ru,
            layers,
        });
    };
    add(
        "google_chrome",
        "Google Chrome",
        "chrome",
        &[
            "chrome.exe",
            "google-chrome",
            "google-chrome-stable",
            "Google Chrome",
            "com.google.Chrome",
        ],
        "Tabs, navigation, bookmarks and page search.",
        "Вкладки, навигация, закладки и поиск на странице.",
        vec![
            preset_layer(
                "Browsing",
                [
                    chord(primary, 0x000F),
                    chord(primary, 0x0017),
                    chord(primary, 0x0009),
                    browser_back,
                    browser_forward,
                    chord(primary | MOD_SHIFT, 0x0017),
                    chord(MOD_CTRL, 0x002B),
                    chord(MOD_CTRL | MOD_SHIFT, 0x002B),
                    0x0044,
                ],
                chord(primary, 0x000F),
                0x004E,
                0x004B,
            ),
            preset_layer(
                "Pages",
                [
                    chord(primary, 0x0007),
                    chord(primary, 0x000D),
                    chord(primary, 0x000B),
                    chord(primary | MOD_SHIFT, 0x0011),
                    chord(primary, 0x0016),
                    chord(primary, 0x0013),
                    chord(primary, 0x0027),
                    chord(primary, 0x002D),
                    chord(primary | MOD_SHIFT, 0x002E),
                ],
                chord(primary, 0x0009),
                chord(primary, 0x002D),
                chord(primary | MOD_SHIFT, 0x002E),
            ),
        ],
    );
    add(
        "adobe_premiere_pro",
        "Adobe Premiere Pro",
        "Adobe Premiere Pro",
        &[
            "Adobe Premiere Pro.exe",
            "Premiere Pro",
            "com.adobe.PremierePro",
        ],
        "Editing tools, transport and timeline; default keyboard map assumed.",
        "Инструменты монтажа, транспорт и таймлайн; предполагается стандартная раскладка.",
        vec![
            preset_layer(
                "Editing",
                [
                    0x0019, 0x0006, 0x0005, 0x0011, 0x001C, 0x0018, 0x000B, 0x001D, 0x002C,
                ],
                0x002C,
                0x000D,
                0x000F,
            ),
            preset_layer(
                "Timeline",
                [
                    0x000C,
                    0x0012,
                    0x000D,
                    0x000E,
                    0x000F,
                    0x0010,
                    chord(primary, 0x000E),
                    chord(primary, 0x001D),
                    chord(primary | MOD_SHIFT, 0x001D),
                ],
                0x0010,
                0x0050,
                0x004F,
            ),
            preset_layer(
                "Project",
                [
                    chord(primary, 0x0016),
                    chord(primary, 0x0012),
                    chord(primary, 0x0011),
                    chord(primary, 0x0006),
                    chord(primary, 0x0019),
                    chord(primary, 0x0004),
                    0x004A,
                    0x004D,
                    0,
                ],
                chord(primary, 0x0016),
                0x004E,
                0x004B,
            ),
        ],
    );
    add(
        "capcut",
        "CapCut",
        "CapCut",
        &["CapCut.exe", "CapCut.app", "com.lemon.lvoverseas"],
        "Portable editing keys; shortcut availability varies by CapCut release.",
        "Общие клавиши монтажа; доступность сочетаний зависит от версии CapCut.",
        vec![
            preset_layer(
                "Editing",
                [
                    0x002C,
                    chord(primary, 0x0005),
                    chord(primary, 0x001D),
                    chord(primary | MOD_SHIFT, 0x001D),
                    chord(primary, 0x0006),
                    chord(primary, 0x0019),
                    0x004C,
                    0x0050,
                    0x004F,
                ],
                0x002C,
                0x0050,
                0x004F,
            ),
            preset_layer(
                "Project",
                [
                    chord(primary, 0x0016),
                    chord(primary, 0x0012),
                    chord(primary, 0x0004),
                    0x004A,
                    0x004D,
                    0,
                    0,
                    0,
                    0,
                ],
                chord(primary, 0x0016),
                0x004E,
                0x004B,
            ),
        ],
    );
    add(
        "adobe_illustrator",
        "Adobe Illustrator",
        "Illustrator",
        &[
            "Illustrator.exe",
            "Adobe Illustrator",
            "com.adobe.illustrator",
        ],
        "Drawing tools, arranging objects and document actions.",
        "Рисование, расположение объектов и действия с документом.",
        vec![
            preset_layer(
                "Tools",
                [
                    0x0019, 0x0004, 0x0013, 0x0017, 0x0010, 0x000F, 0x0005, 0x0008, 0x0006,
                ],
                chord(primary, 0x000A),
                0x002D,
                0x002E,
            ),
            preset_layer(
                "Objects",
                [
                    chord(primary, 0x000A),
                    chord(primary | MOD_SHIFT, 0x000A),
                    chord(primary, 0x0006),
                    chord(primary, 0x0019),
                    chord(primary, 0x001D),
                    chord(primary | MOD_SHIFT, 0x001D),
                    chord(primary, 0x0004),
                    chord(primary, 0x0007),
                    chord(primary | MOD_SHIFT, 0x0007),
                ],
                chord(primary, 0x000A),
                0x004E,
                0x004B,
            ),
            preset_layer(
                "Documents",
                [
                    chord(primary, 0x0011),
                    chord(primary, 0x0012),
                    chord(primary, 0x0016),
                    chord(primary | MOD_SHIFT, 0x0016),
                    chord(primary, 0x0013),
                    chord(primary, 0x0009),
                    0x004A,
                    0x004D,
                    0,
                ],
                chord(primary, 0x0016),
                0x002D,
                0x002E,
            ),
        ],
    );
    add(
        "visual_studio",
        "Visual Studio",
        "devenv",
        &["devenv.exe"],
        "Windows IDE navigation, editing and debugging; manual on other systems.",
        "Навигация, редактирование и отладка в Windows IDE; на других ОС — вручную.",
        vec![
            preset_layer(
                "Editing",
                [
                    chord(primary, 0x0016),
                    chord(primary, 0x0009),
                    chord(primary, 0x000B),
                    chord(primary, 0x001D),
                    chord(primary | MOD_SHIFT, 0x001D),
                    chord(primary, 0x0006),
                    chord(primary, 0x0019),
                    chord(primary, 0x0004),
                    0x003C,
                ],
                chord(primary, 0x0016),
                0x004E,
                0x004B,
            ),
            preset_layer(
                "Debugging",
                [
                    0x003E,
                    chord(MOD_SHIFT, 0x003E),
                    0x0042,
                    0x0043,
                    0x0044,
                    0,
                    0,
                    0,
                    0,
                ],
                0x003E,
                0x0050,
                0x004F,
            ),
            preset_layer(
                "Files",
                [
                    chord(primary, 0x0011),
                    chord(primary, 0x0012),
                    chord(primary | MOD_SHIFT, 0x0016),
                    chord(primary, 0x000A),
                    chord(primary, 0x0013),
                    0,
                    0,
                    0,
                    0,
                ],
                chord(primary, 0x0013),
                0x004E,
                0x004B,
            ),
        ],
    );
    for (id, name, executable, identities) in [
        (
            "intellij_idea",
            "IntelliJ IDEA",
            "idea",
            &[
                "idea64.exe",
                "IntelliJ IDEA",
                "jetbrains-idea",
                "com.jetbrains.intellij",
            ] as &[&str],
        ),
        (
            "pycharm",
            "PyCharm",
            "pycharm",
            &[
                "pycharm64.exe",
                "PyCharm",
                "jetbrains-pycharm",
                "com.jetbrains.pycharm",
            ] as &[&str],
        ),
    ] {
        add(
            id,
            name,
            executable,
            identities,
            "JetBrains editing, navigation and run/debug actions; default keymap assumed.",
            "Редактирование, навигация и запуск/отладка JetBrains; стандартная раскладка.",
            vec![
                preset_layer(
                    "Coding",
                    [
                        chord(primary, 0x0016),
                        chord(primary, 0x0009),
                        chord(primary, 0x001D),
                        chord(primary | MOD_SHIFT, 0x001D),
                        chord(primary, 0x0006),
                        chord(primary, 0x0019),
                        chord(primary, 0x0004),
                        chord(MOD_ALT, 0x0028),
                        0,
                    ],
                    chord(primary, 0x0016),
                    0x004E,
                    0x004B,
                ),
                preset_layer(
                    "Navigation",
                    [
                        chord(primary | MOD_SHIFT, 0x0004),
                        if mac {
                            chord(primary, 0x0012)
                        } else {
                            chord(MOD_CTRL, 0x0011)
                        },
                        if mac {
                            chord(primary, 0x0008)
                        } else {
                            chord(MOD_CTRL, 0x0008)
                        },
                        chord(primary, 0x000A),
                        chord(primary, 0x0013),
                        0,
                        0,
                        0,
                        0,
                    ],
                    chord(primary | MOD_SHIFT, 0x0004),
                    0x004E,
                    0x004B,
                ),
                preset_layer(
                    "Run & files",
                    [
                        if mac { 0 } else { chord(MOD_SHIFT, 0x0043) },
                        if mac { 0 } else { chord(MOD_SHIFT, 0x0042) },
                        chord(primary, 0x0011),
                        chord(primary, 0x0012),
                        chord(primary | MOD_SHIFT, 0x0016),
                        0,
                        0,
                        0,
                        0,
                    ],
                    chord(primary, 0x0016),
                    0x004E,
                    0x004B,
                ),
            ],
        );
    }
    add(
        "discord",
        "Discord",
        "Discord",
        &["Discord.exe", "discord", "com.hnc.Discord"],
        "Quick switch, search and voice controls; verify account shortcuts.",
        "Быстрый переход, поиск и голос; проверьте горячие клавиши аккаунта.",
        vec![
            preset_layer(
                "Navigation",
                [
                    chord(primary, 0x000E),
                    chord(primary, 0x0009),
                    chord(MOD_ALT, 0x0052),
                    chord(MOD_ALT, 0x0051),
                    0x004E,
                    0x004B,
                    0,
                    0,
                    0,
                ],
                chord(primary, 0x000E),
                0x0052,
                0x0051,
            ),
            preset_layer(
                "Voice",
                [
                    chord(primary | MOD_SHIFT, 0x0010),
                    chord(primary | MOD_SHIFT, 0x0007),
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                ],
                0,
                0x00AA,
                0x00A9,
            ),
        ],
    );
    add(
        "streamlabs_desktop",
        "Streamlabs Desktop",
        "Streamlabs Desktop",
        &["Streamlabs Desktop.exe", "slobs", "com.streamlabs.slobs"],
        "Assignable hotkeys only; stream/record actions are not prebound.",
        "Назначаемые горячие клавиши; эфир и запись заранее не привязаны.",
        vec![preset_layer("Hotkeys", [0; 9], 0, 0, 0)],
    );
    result
}

fn preset_layer(
    name: &'static str,
    keys: [u16; 9],
    encoder_press: u16,
    encoder_ccw: u16,
    encoder_cw: u16,
) -> ApplicationLayoutPresetLayer {
    let mut keycodes = default_keycodes();
    keycodes[..keys.len()].copy_from_slice(&keys);
    keycodes[12] = encoder_press;
    keycodes[13] = encoder_ccw;
    keycodes[14] = encoder_cw;
    ApplicationLayoutPresetLayer { name, keycodes }
}

fn preset_layer_is_stock(
    layout: &ApplicationLayout,
    index: usize,
    preset_layer: &ApplicationLayoutPresetLayer,
    default_layers: &[[u16; APPLICATION_LAYOUT_CONTROL_COUNT]],
) -> bool {
    layout.layer_names[index] == preset_layer.name
        && layout.layers[index][..9] == preset_layer.keycodes[..9]
        && (layout.layers[index][9..12] == preset_layer.keycodes[9..12]
            || layout.layers[index][9..12] == default_layers[index][9..12])
        && layout.layers[index][12..] == preset_layer.keycodes[12..]
        && layout.encoder_stacks[index].is_empty()
}

fn retired_preset_is_stock(
    layout: &ApplicationLayout,
    preset: &ApplicationLayoutPreset,
    default_layers: &[[u16; APPLICATION_LAYOUT_CONTROL_COUNT]],
    default_names: &[String],
) -> bool {
    if layout.name != preset.name
        || !executables_match(&layout.executable, preset.executable)
        || !layout.title_contains.is_empty()
        || layout.application_identities
            != normalized_identity_values(
                std::iter::once(preset.executable).chain(preset.identities.iter().copied()),
            )
    {
        return false;
    }
    (0..APPLICATION_LAYOUT_LAYER_COUNT).all(|index| {
        let default_is_stock = || {
            layout.layers[index] == default_layers[index]
                && layout.layer_names[index] == default_names[index]
                && layout.encoder_stacks[index].is_empty()
        };
        match preset.layers.get(index) {
            Some(layer) if index == 0 => {
                preset_layer_is_stock(layout, index, layer, default_layers)
            }
            Some(layer) => {
                preset_layer_is_stock(layout, index, layer, default_layers) || default_is_stock()
            }
            None => default_is_stock(),
        }
    })
}

fn legacy_writer_is_stock(
    layout: &ApplicationLayout,
    default_layers: &[[u16; APPLICATION_LAYOUT_CONTROL_COUNT]],
    default_names: &[String],
) -> bool {
    let primary = if cfg!(target_os = "macos") {
        MOD_GUI
    } else {
        MOD_CTRL
    };
    let redo = if cfg!(target_os = "macos") {
        chord(primary | MOD_SHIFT, 0x001D)
    } else {
        chord(primary, 0x001C)
    };
    let first = preset_layer(
        "Writing",
        [
            chord(primary, 0x0005),
            chord(primary, 0x000C),
            chord(primary, 0x0018),
            chord(primary, 0x001D),
            redo,
            chord(primary, 0x0009),
            chord(primary, 0x000B),
            chord(primary, 0x0016),
            0x0040,
        ],
        chord(primary, 0x0016),
        0x004E,
        0x004B,
    );
    let second = preset_layer(
        "Layout",
        [
            chord(primary, 0x000F),
            chord(primary, 0x0008),
            chord(primary, 0x0015),
            chord(primary, 0x0004),
            chord(primary, 0x0006),
            chord(primary, 0x0019),
            chord(primary, 0x0010),
            chord(primary | MOD_SHIFT, 0x0019),
            chord(primary, 0x0013),
        ],
        chord(primary, 0x0010),
        0x004E,
        0x004B,
    );
    preset_layer_is_stock(layout, 0, &first, default_layers)
        && (preset_layer_is_stock(layout, 1, &second, default_layers)
            || (layout.layers[1] == default_layers[1] && layout.layer_names[1] == default_names[1]))
        && (2..APPLICATION_LAYOUT_LAYER_COUNT).all(|index| {
            layout.layers[index] == default_layers[index]
                && layout.layer_names[index] == default_names[index]
                && layout.encoder_stacks[index].is_empty()
        })
}

#[allow(clippy::too_many_arguments)]
fn preset(
    id: &'static str,
    name: &'static str,
    executable: &'static str,
    identities: &'static [&'static str],
    summary: &'static str,
    summary_ru: &'static str,
    layer_name: &'static str,
    keys: [u16; 9],
    encoder_press: u16,
    encoder_ccw: u16,
    encoder_cw: u16,
) -> ApplicationLayoutPreset {
    let mut keycodes = default_keycodes();
    keycodes[..keys.len()].copy_from_slice(&keys);
    keycodes[12] = encoder_press;
    keycodes[13] = encoder_ccw;
    keycodes[14] = encoder_cw;
    ApplicationLayoutPreset {
        id,
        name,
        executable,
        identities,
        summary,
        summary_ru,
        layers: vec![ApplicationLayoutPresetLayer {
            name: layer_name,
            keycodes,
        }],
    }
}

const fn chord(modifiers: u16, keycode: u16) -> u16 {
    modifiers | keycode
}

fn crc16_snapshot(
    layers: &[[u16; APPLICATION_LAYOUT_CONTROL_COUNT]; APPLICATION_LAYOUT_LAYER_COUNT],
    visuals: &[[u8; APPLICATION_LAYOUT_CONTROL_COUNT]; APPLICATION_LAYOUT_LAYER_COUNT],
    encoder_stacks: &[Vec<EncoderStackAction>; APPLICATION_LAYOUT_LAYER_COUNT],
    name: &[u8],
    layer_names: &[String; APPLICATION_LAYOUT_LAYER_COUNT],
) -> u16 {
    let mut crc = 0xFFFFu16;
    let bytes = layers
        .iter()
        .flatten()
        .flat_map(|value| value.to_le_bytes())
        .chain(visuals.iter().flatten().copied())
        .chain(encoder_stacks.iter().flat_map(|actions| {
            let count = if (2..=APPLICATION_LAYOUT_STACK_SLOTS).contains(&actions.len()) {
                actions.len()
            } else {
                0
            };
            std::iter::once(count as u8).chain((0..APPLICATION_LAYOUT_STACK_SLOTS).flat_map(
                move |slot| {
                    let action = (slot < count).then(|| &actions[slot]);
                    let ccw = action.map_or(0, |a| a.counter_clockwise);
                    let cw = action.map_or(0, |a| a.clockwise);
                    let name =
                        action.map_or(&[][..], |a| application_layout_stack_name_bytes(&a.name));
                    ccw.to_le_bytes()
                        .into_iter()
                        .chain(cw.to_le_bytes())
                        .chain(std::iter::once(name.len() as u8))
                        .chain(name.iter().copied())
                },
            ))
        }))
        .chain(std::iter::once(name.len() as u8))
        .chain(name.iter().copied())
        .chain(layer_names.iter().flat_map(|layer_name| {
            let encoded = application_layout_name_bytes(layer_name);
            std::iter::once(encoded.len() as u8).chain(encoded.iter().copied())
        }));
    for byte in bytes {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn default_keycodes() -> [u16; APPLICATION_LAYOUT_CONTROL_COUNT] {
    [APPLICATION_LAYOUT_UNSET_KEYCODE; APPLICATION_LAYOUT_CONTROL_COUNT]
}

fn default_layer_keycodes() -> Vec<[u16; APPLICATION_LAYOUT_CONTROL_COUNT]> {
    vec![default_keycodes(); APPLICATION_LAYOUT_LAYER_COUNT]
}

pub(crate) fn default_layer_names() -> Vec<String> {
    [
        "Numbers",
        "Navigation",
        "Mouse",
        "Media",
        "Four",
        "Five",
        "Six",
        "Seven",
        "Eight",
        "Nine",
        "Ten",
        "Eleven",
        "Twelve",
        "Thirteen",
        "Fourteen",
        "Fifteen",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn legacy_default_layer_names() -> Vec<String> {
    (0..APPLICATION_LAYOUT_LAYER_COUNT)
        .map(|layer| {
            if layer == 0 {
                "Base".to_owned()
            } else {
                format!("Layer {layer}")
            }
        })
        .collect()
}

fn default_layouts() -> BTreeMap<String, ApplicationLayout> {
    let layout = ApplicationLayout::default_layout();
    [(layout.id.clone(), layout)].into_iter().collect()
}

fn default_layout_id() -> String {
    DEFAULT_APPLICATION_LAYOUT_ID.to_owned()
}

const fn default_revision() -> u32 {
    1
}

const fn default_next_id() -> u32 {
    1
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(executable: &str, title: &str) -> DetectedApplication {
        DetectedApplication {
            executable: executable.to_owned(),
            identities: Vec::new(),
            display_name: executable.to_owned(),
            window_title: title.to_owned(),
        }
    }

    #[test]
    fn default_is_always_present_and_cannot_be_removed() {
        let mut settings = DeviceApplicationLayouts::default();
        assert!(settings.layouts.contains_key(DEFAULT_APPLICATION_LAYOUT_ID));
        assert!(!settings.remove(DEFAULT_APPLICATION_LAYOUT_ID));
        assert_eq!(
            settings.layouts[DEFAULT_APPLICATION_LAYOUT_ID].layer_names,
            [
                "Numbers",
                "Navigation",
                "Mouse",
                "Media",
                "Four",
                "Five",
                "Six",
                "Seven",
                "Eight",
                "Nine",
                "Ten",
                "Eleven",
                "Twelve",
                "Thirteen",
                "Fourteen",
                "Fifteen",
            ]
        );
    }

    #[test]
    fn generated_v4_default_names_migrate_to_stock_firmware_names() {
        let mut settings = DeviceApplicationLayouts::default();
        settings
            .layouts
            .get_mut(DEFAULT_APPLICATION_LAYOUT_ID)
            .unwrap()
            .layer_names = legacy_default_layer_names();

        assert!(settings.normalize());
        assert_eq!(
            settings.layouts[DEFAULT_APPLICATION_LAYOUT_ID].layer_names,
            default_layer_names()
        );
    }

    #[test]
    fn executable_matching_is_case_and_extension_independent() {
        let mut settings = DeviceApplicationLayouts::default();
        settings.create_for_application(&app("Figma.exe", "Draft"));
        assert_ne!(
            settings.resolve(Some(&app("figma", "Other"))),
            DEFAULT_APPLICATION_LAYOUT_ID
        );
    }

    #[test]
    fn generic_identity_aliases_match_desktop_ids_processes_and_window_classes() {
        assert!(executables_match(
            "org.telegram.desktop",
            "telegram-desktop"
        ));
        assert!(executables_match("telegram-desktop", "Telegram"));
        assert!(executables_match("org.telegram.desktop", "Telegram"));
        assert!(!executables_match("telegram-desktop", "kotatogram-desktop"));
        assert!(executables_match("Arduino IDE", "arduino-ide"));
        assert!(!executables_match("code", "codeblocks"));
        assert!(executables_match(
            "org.gnome.Terminal.desktop",
            "gnome-terminal-server"
        ));
        assert!(executables_match("org.kde.konsole", "konsole"));
        assert!(executables_match("ticktick", "TickTick"));
        assert!(executables_match("ticktick", "ticktick_ticktick.desktop"));
    }

    #[test]
    fn generic_runtime_aliases_cannot_turn_telegram_into_a_catch_all() {
        let mut settings = DeviceApplicationLayouts::default();
        let telegram = settings.create_for_application(&DetectedApplication {
            executable: "telegram-desktop".to_owned(),
            identities: vec!["org.telegram.desktop".to_owned()],
            display_name: "Telegram".to_owned(),
            window_title: String::new(),
        });
        settings
            .layouts
            .get_mut(&telegram)
            .unwrap()
            .application_identities
            .extend([
                "desktop".to_owned(),
                "electron".to_owned(),
                "snap".to_owned(),
            ]);
        let vscode = settings.create_for_application(&DetectedApplication {
            executable: "code".to_owned(),
            identities: vec!["electron".to_owned(), "desktop".to_owned()],
            display_name: "Visual Studio Code".to_owned(),
            window_title: String::new(),
        });

        assert!(settings.normalize());
        assert!(!settings.layouts[&telegram]
            .application_identities
            .iter()
            .any(|identity| matches!(identity.as_str(), "desktop" | "electron" | "snap")));
        assert_eq!(
            settings.resolve(Some(&DetectedApplication {
                executable: "code".to_owned(),
                identities: vec!["electron".to_owned(), "desktop".to_owned()],
                display_name: "Visual Studio Code".to_owned(),
                window_title: "main.rs".to_owned(),
            })),
            vscode
        );
    }

    #[test]
    fn official_snap_ticktick_rule_matches_gnome_runtime_identity() {
        let mut settings = DeviceApplicationLayouts::default();
        let layout_id = settings.create_for_application(&DetectedApplication {
            executable: "ticktick".to_owned(),
            identities: normalized_identity_values([
                "ticktick",
                "TickTick",
                "ticktick_ticktick.desktop",
            ]),
            display_name: "TickTick".to_owned(),
            window_title: String::new(),
        });
        let focused = DetectedApplication {
            executable: "ticktick".to_owned(),
            identities: normalized_identity_values(["ticktick_ticktick.desktop", "TickTick"]),
            display_name: "TickTick".to_owned(),
            window_title: "TickTick".to_owned(),
        };

        assert_eq!(settings.resolve(Some(&focused)), layout_id);
    }

    #[test]
    fn official_macos_ticktick_bundle_rule_matches_nsworkspace_identity() {
        let mut settings = DeviceApplicationLayouts::default();
        let layout_id = settings.create_for_application(&DetectedApplication {
            executable: "com.TickTick.task.mac".to_owned(),
            identities: normalized_identity_values([
                "com.TickTick.task.mac",
                "/Applications/TickTick.app/Contents/MacOS/TickTick",
                "TickTick",
            ]),
            display_name: "TickTick".to_owned(),
            window_title: String::new(),
        });
        let focused = DetectedApplication {
            executable: "com.TickTick.task.mac".to_owned(),
            identities: normalized_identity_values([
                "com.TickTick.task.mac",
                "/Applications/TickTick.app/Contents/MacOS/TickTick",
                "TickTick",
            ]),
            display_name: "TickTick".to_owned(),
            window_title: String::new(),
        };

        assert_eq!(settings.resolve(Some(&focused)), layout_id);
    }

    #[test]
    fn macos_bundle_suffix_does_not_match_an_unrelated_application() {
        assert!(!executables_match(
            "com.TickTick.task.mac",
            "com.Other.product.mac"
        ));
    }

    #[test]
    fn distinct_macos_bundle_ids_do_not_conflict_through_shared_aliases() {
        let mut settings = DeviceApplicationLayouts::default();
        settings.create_for_application(&DetectedApplication {
            executable: "com.example.FirstApp".to_owned(),
            identities: vec!["Shared Helper".to_owned(), "finder".to_owned()],
            display_name: "First".to_owned(),
            window_title: String::new(),
        });
        let second = DetectedApplication {
            executable: "com.example.SecondApp".to_owned(),
            identities: vec!["Shared Helper".to_owned(), "finder".to_owned()],
            display_name: "Second".to_owned(),
            window_title: String::new(),
        };

        assert!(!settings.application_rule_exists(&second, "", None));
        assert_eq!(
            settings.resolve(Some(&second)),
            DEFAULT_APPLICATION_LAYOUT_ID
        );
    }

    #[test]
    fn exact_macos_bundle_id_is_a_duplicate_with_an_empty_optional_filter() {
        let mut settings = DeviceApplicationLayouts::default();
        let finder = DetectedApplication {
            executable: "com.apple.finder".to_owned(),
            identities: vec![
                "/System/Library/CoreServices/Finder.app/Contents/MacOS/Finder".to_owned(),
                "Finder".to_owned(),
            ],
            display_name: "Finder".to_owned(),
            window_title: String::new(),
        };
        settings.create_for_application(&finder);

        assert!(settings.application_rule_exists(&finder, "", None));
    }

    #[test]
    fn poisoned_macos_telegram_rule_does_not_capture_ticktick() {
        let mut settings = DeviceApplicationLayouts::default();
        let telegram = settings.create_for_application(&DetectedApplication {
            executable: "ru.keepcoder.Telegram".to_owned(),
            identities: normalized_identity_values([
                "ru.keepcoder.Telegram",
                "/Applications/Telegram.app/Contents/MacOS/Telegram",
            ]),
            display_name: "Telegram".to_owned(),
            window_title: String::new(),
        });
        settings
            .layouts
            .get_mut(&telegram)
            .unwrap()
            .application_identities
            .extend([
                "desktop".to_owned(),
                "electron".to_owned(),
                "mac".to_owned(),
            ]);
        let ticktick = settings.create_for_application(&DetectedApplication {
            executable: "com.TickTick.task.mac".to_owned(),
            identities: normalized_identity_values([
                "com.TickTick.task.mac",
                "/Applications/TickTick.app/Contents/MacOS/TickTick",
            ]),
            display_name: "TickTick".to_owned(),
            window_title: String::new(),
        });

        assert!(settings.normalize());
        assert!(!settings.layouts[&telegram]
            .application_identities
            .iter()
            .any(|identity| matches!(identity.as_str(), "desktop" | "electron" | "mac")));
        assert_eq!(
            settings.resolve(Some(&DetectedApplication {
                executable: "com.TickTick.task.mac".to_owned(),
                identities: normalized_identity_values([
                    "com.TickTick.task.mac",
                    "/Applications/TickTick.app/Contents/MacOS/TickTick",
                ]),
                display_name: "TickTick".to_owned(),
                window_title: String::new(),
            })),
            ticktick
        );
    }

    #[test]
    fn older_layouts_are_enriched_from_the_current_application_catalog() {
        let mut settings = DeviceApplicationLayouts::default();
        let layout_id = settings.create_for_application(&app("gnome-terminal", ""));
        settings
            .layouts
            .get_mut(&layout_id)
            .unwrap()
            .application_identities = Vec::new();

        let catalog = [DetectedApplication {
            executable: "org.gnome.Terminal".to_owned(),
            identities: vec![
                "org.gnome.Terminal.desktop".to_owned(),
                "Gnome-terminal".to_owned(),
            ],
            display_name: "Terminal".to_owned(),
            window_title: String::new(),
        }];

        assert!(settings.enrich_application_identities(&catalog));
        assert_eq!(
            settings.resolve(Some(&app("gnome-terminal-server", "Terminal"))),
            layout_id
        );
    }

    #[test]
    fn vscode_to_telegram_switches_to_each_configured_layout() {
        let mut settings = DeviceApplicationLayouts::default();
        let vscode = settings.create_for_application(&app("com.visualstudio.code", "main.rs"));
        let telegram = settings.create_for_application(&app("org.telegram.desktop", "Telegram"));

        assert_eq!(settings.resolve(Some(&app("code", "main.rs"))), vscode);
        settings.active_layout_id = vscode;
        assert_eq!(
            settings.resolve(Some(&app("Telegram", "Ergohaven — Telegram"))),
            telegram
        );
    }

    #[test]
    fn selected_app_matches_any_captured_identity_without_app_specific_code() {
        let mut selected = app("com.vendor.product.desktop", "");
        selected.identities = vec![
            "product-launcher".to_owned(),
            "VendorProductWindow".to_owned(),
        ];
        let mut settings = DeviceApplicationLayouts::default();
        let layout = settings.create_for_application(&selected);

        let mut foreground = app("product-bin", "Project");
        foreground.identities = vec!["VendorProductWindow".to_owned()];
        assert_eq!(settings.resolve(Some(&foreground)), layout);
    }

    #[test]
    fn vscode_desktop_and_url_handler_ids_match_runtime_process() {
        assert!(executables_match("com.visualstudio.code", "code"));
        assert!(executables_match("code-url-handler", "code"));
        assert!(!executables_match("codeblocks", "code"));
        assert!(executables_match("com.acme.paint.desktop", "paint"));
        assert!(executables_match("io.github.zen_browser.Zen", "zen"));
        assert!(!executables_match("com.acme.paint.desktop", "painter"));
    }

    #[test]
    fn default_names_migrate_per_layer_without_overwriting_custom_names() {
        let mut settings = DeviceApplicationLayouts::default();
        let default = settings
            .layouts
            .get_mut(DEFAULT_APPLICATION_LAYOUT_ID)
            .expect("default layout");
        default.layer_names[0] = "Base".to_owned();
        default.layer_names[1] = "My Navigation".to_owned();
        default.layer_names[2] = "Layer 2".to_owned();

        assert!(settings.normalize());
        let names = &settings
            .layouts
            .get(DEFAULT_APPLICATION_LAYOUT_ID)
            .expect("default layout")
            .layer_names;
        assert_eq!(names[0], "Numbers");
        assert_eq!(names[1], "My Navigation");
        assert_eq!(names[2], "Mouse");
    }

    #[test]
    fn title_specific_layout_wins() {
        let mut settings = DeviceApplicationLayouts::default();
        let generic = settings.create_for_application(&app("code", "main.rs"));
        let specific = settings.create_for_application(&app("code", "README"));
        settings.layouts.get_mut(&specific).unwrap().title_contains = "README".to_owned();
        assert_eq!(
            settings.resolve(Some(&app("code", "README — Visual Studio Code"))),
            specific
        );
        assert_eq!(
            settings.resolve(Some(&app("code", "main.rs — Visual Studio Code"))),
            generic
        );
    }

    #[test]
    fn layout_names_are_unique_case_insensitively() {
        let mut settings = DeviceApplicationLayouts::default();
        let telegram = settings.create_for_application_named(
            &app("telegram-desktop", "Telegram"),
            Some("Telegram"),
            "",
        );

        assert!(settings.layout_name_exists(" telegram ", None));
        assert!(settings.layout_name_exists("TELEGRAM", None));
        assert!(!settings.layout_name_exists("Telegram", Some(&telegram)));
        assert!(!settings.layout_name_exists("Messenger", None));
    }

    #[test]
    fn editing_name_only_keeps_pending_builtin_binding() {
        let mut settings = DeviceApplicationLayouts::default();
        let application = app("firefox", "Firefox");
        let id = settings.create_for_application(&application);
        settings
            .pending_builtin_auto_bind
            .insert(id.clone(), "firefox".to_owned());
        assert!(settings.update_application_rule(&id, &application, "Web", ""));
        assert_eq!(settings.layouts[&id].name, "Web");
        assert_eq!(settings.pending_builtin_auto_bind[&id], "firefox");
        let replacement = app("chromium", "Chromium");
        assert!(settings.update_application_rule(&id, &replacement, "Web", ""));
        assert!(!settings.pending_builtin_auto_bind.contains_key(&id));
    }

    #[test]
    fn normalize_renames_legacy_vial_label_only_for_entropy_executable() {
        let mut settings = DeviceApplicationLayouts::default();
        let entropy =
            settings.create_for_application_named(&app("entropy", "Vial"), Some("Vial"), "");
        let vial =
            settings.create_for_application_named(&app("vial", "Vial GUI"), Some("Vial GUI"), "");
        settings.active_layout_id = entropy.clone();
        let before = settings.layouts[&entropy].clone();

        assert!(settings.normalize());
        let renamed = &settings.layouts[&entropy];
        assert_eq!(renamed.name, "Entropy");
        assert_eq!(renamed.executable, before.executable);
        assert_eq!(renamed.layers, before.layers);
        assert_eq!(renamed.layer_names, before.layer_names);
        assert!(renamed.revision > before.revision);
        assert_eq!(settings.active_layout_id, entropy);
        assert_eq!(settings.layouts[&vial].name, "Vial GUI");

        assert!(!settings.normalize());

        let mut unrelated = DeviceApplicationLayouts::default();
        let vial_id =
            unrelated.create_for_application_named(&app("vial", "Vial"), Some("Vial"), "");
        unrelated.normalize();
        assert_eq!(unrelated.layouts[&vial_id].name, "Vial");
    }

    #[test]
    fn rename_layout_changes_only_the_display_name_and_revision() {
        let mut settings = DeviceApplicationLayouts::default();
        let telegram = settings.create_for_application_named(
            &app("telegram-desktop", "Telegram"),
            Some("Telegram"),
            "",
        );
        let before = settings.layouts.get(&telegram).unwrap().clone();

        assert!(settings.rename_layout(&telegram, " Telegram — работа "));
        let renamed = settings.layouts.get(&telegram).unwrap();
        assert_eq!(renamed.name, "Telegram — работа");
        assert_eq!(renamed.executable, before.executable);
        assert_eq!(
            renamed.application_identities,
            before.application_identities
        );
        assert_eq!(renamed.layers, before.layers);
        assert_eq!(renamed.layer_names, before.layer_names);
        assert_eq!(renamed.automatic_switching, before.automatic_switching);
        assert_ne!(renamed.revision, before.revision);
    }

    #[test]
    fn rename_layout_rejects_default_empty_and_duplicate_names() {
        let mut settings = DeviceApplicationLayouts::default();
        let telegram = settings.create_for_application_named(
            &app("telegram-desktop", "Telegram"),
            Some("Telegram"),
            "",
        );
        let blender =
            settings.create_for_application_named(&app("blender", "Blender"), Some("Blender"), "");

        assert!(!settings.rename_layout(DEFAULT_APPLICATION_LAYOUT_ID, "Primary"));
        assert!(!settings.rename_layout(&telegram, "  "));
        assert!(!settings.rename_layout(&telegram, " blender "));
        assert_eq!(settings.layouts[&telegram].name, "Telegram");
        assert_eq!(settings.layouts[&blender].name, "Blender");
        assert_eq!(
            settings.layouts[DEFAULT_APPLICATION_LAYOUT_ID].name,
            "Default"
        );
    }

    #[test]
    fn duplicate_application_rules_require_distinct_title_filters() {
        let mut settings = DeviceApplicationLayouts::default();
        let vscode = app("com.visualstudio.code", "Entropy");
        let generic = settings.create_for_application_named(&vscode, Some("VS Code"), "");

        assert!(settings.application_rule_exists(&vscode, "", None));
        assert!(!settings.application_rule_exists(&vscode, "Firmware", None));

        let firmware =
            settings.create_for_application_named(&vscode, Some("VS Code Firmware"), "Firmware");
        assert!(settings.application_rule_exists(&vscode, "firmware", None));
        assert!(!settings.application_rule_exists(&vscode, "Entropy", None));
        assert!(!settings.application_rule_exists(&vscode, "", Some(&generic)));
        assert!(!settings.application_rule_exists(&vscode, "Firmware", Some(&firmware),));
    }

    #[test]
    fn legacy_ambiguous_rules_resolve_deterministically() {
        let mut settings = DeviceApplicationLayouts::default();
        let first = settings.create_for_application(&app("telegram-desktop", "Telegram"));
        let second = settings.create_for_application(&app("telegram-desktop", "Telegram"));
        assert!(first < second);

        assert_eq!(settings.resolve(Some(&app("Telegram", "Telegram"))), first);
    }

    #[test]
    fn normalization_repairs_stale_inner_ids_before_deletion() {
        let mut settings = DeviceApplicationLayouts::default();
        let finder = settings.create_for_application(&app("com.apple.finder", "Finder"));
        let telegram = settings.create_for_application(&app("ru.keepcoder.Telegram", "Telegram"));
        settings.layouts.get_mut(&finder).unwrap().layers[0][0] = 0x1111;
        settings.layouts.get_mut(&telegram).unwrap().layers[0][0] = 0x2222;

        settings.layouts.get_mut(&finder).unwrap().id = telegram.clone();
        settings.layouts.get_mut(&telegram).unwrap().id = finder.clone();
        settings.active_layout_id = finder.clone();
        settings.editor_layout_id = telegram.clone();
        assert!(settings.normalize());
        assert_eq!(settings.layouts[&finder].id, finder);
        assert_eq!(settings.layouts[&telegram].id, telegram);

        assert!(settings.remove(&telegram));
        assert_eq!(settings.active_layout_id, finder);
        assert_eq!(settings.editor_layout_id, DEFAULT_APPLICATION_LAYOUT_ID);
        assert_eq!(settings.layouts[&finder].layers[0][0], 0x1111);
        assert_eq!(
            settings.resolve(Some(&app("com.apple.finder", "Finder"))),
            finder
        );
    }

    #[test]
    fn edit_targets_stable_id_even_when_editor_selection_changes() {
        let mut settings = DeviceApplicationLayouts::default();
        let finder = settings.create_for_application(&app("com.apple.finder", "Finder"));
        let telegram = settings.create_for_application(&app("ru.keepcoder.Telegram", "Telegram"));
        let telegram_before = settings.layouts[&telegram].clone();
        settings.editor_layout_id = telegram.clone();

        assert!(settings.update_application_rule(
            &finder,
            &app("com.apple.Safari", "Safari"),
            "Browser",
            "Private"
        ));
        assert_eq!(settings.layouts[&finder].executable, "com.apple.Safari");
        assert_eq!(settings.layouts[&finder].name, "Browser");
        assert_eq!(settings.layouts[&finder].title_contains, "Private");
        assert_eq!(settings.layouts[&telegram], telegram_before);
    }

    #[test]
    fn deleted_profile_id_is_not_reused_after_legacy_settings_normalize() {
        let mut settings = DeviceApplicationLayouts::default();
        let finder = settings.create_for_application(&app("com.apple.finder", "Finder"));
        settings.next_id = 1;
        assert!(settings.normalize());
        assert!(settings.remove(&finder));

        let replacement = settings.create_for_application(&app("com.apple.finder", "Finder"));
        assert_ne!(replacement, finder);
    }

    #[test]
    fn automatic_switching_is_independent_per_layout() {
        let mut settings = DeviceApplicationLayouts::default();
        let manual = settings.create_for_application(&app("calculator", "Calculator"));
        let figma = settings.create_for_application(&app("figma", "Draft"));
        settings.active_layout_id = manual.clone();
        settings
            .layouts
            .get_mut(&figma)
            .unwrap()
            .automatic_switching = false;
        assert_eq!(settings.resolve(Some(&app("figma", "Draft"))), manual);
        settings
            .layouts
            .get_mut(&figma)
            .unwrap()
            .automatic_switching = true;
        assert_eq!(settings.resolve(Some(&app("figma", "Draft"))), figma);
    }

    #[test]
    fn known_applications_always_switch_and_unknown_windows_follow_fallback_setting() {
        let mut settings = DeviceApplicationLayouts::default();
        let vscode = settings.create_for_application(&app("code", "Entropy"));
        let browser = settings.create_for_application(&app("firefox", "Docs"));
        settings.active_layout_id = vscode.clone();

        let resolved_browser = settings.resolve(Some(&app("firefox", "Docs")));
        assert_eq!(resolved_browser, browser);
        settings.active_layout_id = resolved_browser;

        assert_eq!(
            settings.resolve(Some(&app("entropy", "Entropy"))),
            DEFAULT_APPLICATION_LAYOUT_ID
        );

        settings.automatically_return_to_default = false;
        assert_eq!(settings.resolve(Some(&app("code", "Entropy"))), vscode);
        settings.active_layout_id = vscode.clone();
        assert_eq!(settings.resolve(Some(&app("firefox", "Docs"))), browser);
        settings.active_layout_id = browser.clone();
        assert_eq!(settings.resolve(Some(&app("entropy", "Entropy"))), browser);
        assert_eq!(
            settings.resolve(Some(&app("terminal", "Terminal"))),
            browser
        );
        assert_eq!(settings.resolve(None), browser);
    }

    #[test]
    fn legacy_unknown_window_setting_migrates_to_automatic_return_setting() {
        let settings: DeviceApplicationLayouts = serde_json::from_str(
            r#"{
                "unknown_window_uses_default": false
            }"#,
        )
        .unwrap();

        assert!(!settings.automatically_return_to_default);

        let serialized = serde_json::to_value(settings).unwrap();
        assert_eq!(
            serialized["automatically_return_to_default"],
            serde_json::Value::Bool(false)
        );
        assert!(serialized.get("unknown_window_uses_default").is_none());
    }

    #[test]
    fn legacy_global_automatic_switching_is_migrated_once() {
        let mut settings = DeviceApplicationLayouts::default();
        let figma = settings.create_for_application(&app("figma", "Draft"));
        let blender = settings.create_for_application(&app("blender", "Scene"));
        settings.legacy_automatic_switching = Some(false);

        assert!(settings.normalize());
        assert!(!settings.layouts[&figma].automatic_switching);
        assert!(!settings.layouts[&blender].automatic_switching);

        settings
            .layouts
            .get_mut(&figma)
            .unwrap()
            .automatic_switching = true;
        assert!(!settings.normalize());
        assert!(settings.layouts[&figma].automatic_switching);
    }

    #[test]
    fn legacy_single_layer_migrates_without_losing_keycodes() {
        let json = r#"{
            "id":"figma_1",
            "name":"Figma",
            "executable":"figma",
            "keycodes":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15],
            "revision":1
        }"#;
        let mut layout: ApplicationLayout = serde_json::from_str(json).unwrap();
        assert!(layout.normalize());
        assert_eq!(layout.layers.len(), APPLICATION_LAYOUT_LAYER_COUNT);
        assert_eq!(layout.layers[0][0], 1);
        assert_eq!(layout.layers[0][14], 15);
        assert!(layout.encoder_stacks.iter().all(Vec::is_empty));
    }

    #[test]
    fn each_application_layout_keeps_independent_layers_and_names() {
        let mut settings = DeviceApplicationLayouts::default();
        let figma = settings.create_for_application(&app("figma", "Draft"));
        let blender = settings.create_for_application(&app("blender", "Scene"));

        let layout = settings.layouts.get_mut(&figma).unwrap();
        assert!(layout.set_keycode(15, 4, 0x4321));
        assert!(layout.set_layer_name(15, "Components".to_owned()));

        assert_eq!(settings.layouts[&figma].layers[15][4], 0x4321);
        assert_eq!(settings.layouts[&figma].layer_names[15], "Components");
        assert_ne!(settings.layouts[&blender].layers[15][4], 0x4321);
        assert_ne!(settings.layouts[&blender].layer_names[15], "Components");
    }

    #[test]
    fn packet_family_carries_all_controls_and_crc() {
        let mut layout = ApplicationLayout::default_layout();
        layout.name = "Telegram".to_owned();
        for (layer, keycodes) in layout.layers.iter_mut().enumerate() {
            for (index, keycode) in keycodes.iter_mut().enumerate() {
                *keycode = 0x1000 + layer as u16 * 0x100 + index as u16;
            }
        }
        let packets = ApplicationLayoutSnapshot::from_layout(&layout).packets();
        assert_eq!(packets.len(), 130);
        assert_eq!(
            packets.iter().map(|packet| packet[0]).collect::<Vec<_>>(),
            std::iter::once(HID_APPLICATION_LAYOUT_BEGIN)
                .chain(
                    std::iter::repeat(HID_APPLICATION_LAYOUT_KEYCODES)
                        .take(APPLICATION_LAYOUT_LAYER_COUNT * 2),
                )
                .chain(
                    std::iter::repeat(HID_APPLICATION_LAYOUT_VISUALS)
                        .take(APPLICATION_LAYOUT_LAYER_COUNT)
                )
                .chain(
                    std::iter::repeat(HID_APPLICATION_LAYOUT_ENCODER_STACK)
                        .take(APPLICATION_LAYOUT_LAYER_COUNT * APPLICATION_LAYOUT_STACK_SLOTS)
                )
                .chain(
                    std::iter::repeat(HID_APPLICATION_LAYOUT_LAYER_NAME)
                        .take(APPLICATION_LAYOUT_LAYER_COUNT),
                )
                .chain(std::iter::once(HID_APPLICATION_LAYOUT_COMMIT))
                .collect::<Vec<_>>()
        );
        assert_eq!(packets[0][0], HID_APPLICATION_LAYOUT_BEGIN);
        assert_eq!(packets[0][8], APPLICATION_LAYOUT_LAYER_COUNT as u8);
        assert_eq!(packets[0][9], 8);
        assert_eq!(&packets[0][10..18], b"Telegram");
        assert_eq!(packets[1][2], 0);
        assert_eq!(packets[1][3], 0);
        assert_eq!(packets[1][4], 13);
        assert_eq!(packets[2][3], 13);
        assert_eq!(packets[2][4], 2);
        assert_eq!(packets[31][2], 15);
        assert_eq!(packets[31][3], 0);
        assert_eq!(packets[32][2], 15);
        assert_eq!(packets[32][3], 13);
        assert_eq!(packets[113][0], HID_APPLICATION_LAYOUT_LAYER_NAME);
        assert_eq!(packets[113][2], 0);
        assert_eq!(packets[113][3], 7);
        assert_eq!(&packets[113][4..11], b"Numbers");
        assert_eq!(packets[128][2], 15);
        let commit = packets.last().unwrap();
        assert_eq!(commit[0], HID_APPLICATION_LAYOUT_COMMIT);
        assert_ne!(u16::from_le_bytes([commit[7], commit[8]]), 0);
    }

    #[test]
    fn runtime_layout_name_is_utf8_safe_and_fits_one_packet() {
        let mut layout = ApplicationLayout::default_layout();
        layout.name = "Очень длинная раскладка Telegram".to_owned();

        let packets = ApplicationLayoutSnapshot::from_layout(&layout).packets();
        let length = usize::from(packets[0][9]);
        let encoded = &packets[0][10..10 + length];

        assert!(length <= APPLICATION_LAYOUT_NAME_BYTES);
        assert!(std::str::from_utf8(encoded).is_ok());
        assert!(layout
            .name
            .starts_with(std::str::from_utf8(encoded).unwrap()));
    }

    #[test]
    fn runtime_layer_names_are_utf8_safe_and_keepalive_is_one_packet() {
        let mut layout = ApplicationLayout::default_layout();
        layout.layer_names[3] = "Очень длинное имя слоя Медиа".to_owned();

        let snapshot = ApplicationLayoutSnapshot::from_layout(&layout);
        let packets = snapshot.packets();
        let packet = &packets[113 + 3];
        let length = usize::from(packet[3]);
        let encoded = &packet[4..4 + length];

        assert_eq!(packet[0], HID_APPLICATION_LAYOUT_LAYER_NAME);
        assert!(length <= APPLICATION_LAYOUT_NAME_BYTES);
        assert!(std::str::from_utf8(encoded).is_ok());
        assert!(layout.layer_names[3].starts_with(std::str::from_utf8(encoded).unwrap()));

        let keepalive = snapshot.keepalive_packet();
        assert_eq!(keepalive[0], HID_APPLICATION_LAYOUT_KEEPALIVE);
        assert_eq!(keepalive[1], APPLICATION_LAYOUT_PROTOCOL_VERSION);
        assert_eq!(keepalive[2], 1);
        assert_eq!(&keepalive[3..7], &layout.revision.to_le_bytes());
    }

    #[test]
    fn existing_four_layer_layout_expands_to_sixteen_without_data_loss() {
        let json = r#"{
            "id":"figma_1",
            "name":"Figma",
            "executable":"figma",
            "layers":[
                [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15],
                [16,17,18,19,20,21,22,23,24,25,26,27,28,29,30],
                [31,32,33,34,35,36,37,38,39,40,41,42,43,44,45],
                [46,47,48,49,50,51,52,53,54,55,56,57,58,59,60]
            ],
            "layer_names":["Base","Tools","Components","Review"],
            "revision":1
        }"#;
        let mut layout: ApplicationLayout = serde_json::from_str(json).unwrap();

        assert!(layout.normalize());
        assert_eq!(layout.layers.len(), 16);
        assert_eq!(layout.layer_names.len(), 16);
        assert_eq!(layout.layers[3][14], 60);
        assert_eq!(layout.layer_names[3], "Review");
        assert_eq!(layout.layers[15], default_keycodes());
        assert_eq!(layout.layer_names[15], "Fifteen");
    }

    #[test]
    fn master_gate_preserves_manual_choice_and_settings() {
        let mut settings = DeviceApplicationLayouts::default();
        let manual = settings.create_for_application(&app("figma", "Design"));
        settings.active_layout_id = manual.clone();
        settings.automatic_switching_enabled = false;
        assert_eq!(settings.resolve(Some(&app("code", "Editor"))), manual);
        assert_eq!(settings.resolve(None), manual);
        let restored: DeviceApplicationLayouts =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert!(!restored.automatic_switching_enabled);
        assert_eq!(restored.active_layout_id, manual);
        settings.automatic_switching_enabled = true;
        assert_eq!(settings.resolve(None), DEFAULT_APPLICATION_LAYOUT_ID);
    }

    #[test]
    fn preset_provisioning_is_idempotent_and_preserves_user_edits() {
        let mut settings = DeviceApplicationLayouts::default();
        let existing = settings.create_for_application(&app("code", "Work"));
        settings
            .layouts
            .get_mut(&existing)
            .unwrap()
            .set_keycode(0, 0, 0x1234);
        let installed = ["audacity".to_owned(), "visual_studio_code".to_owned()]
            .into_iter()
            .collect();
        assert!(settings.provision_builtin_presets(Some(&installed)));
        assert_eq!(settings.layouts.len(), 15); // Default + existing VS Code + 13 other presets.
        assert_eq!(settings.layouts[&existing].layers[0][0], 0x1234);
        assert!(settings
            .layouts
            .values()
            .any(|layout| layout.name == "Audacity" && layout.automatic_switching));
        assert!(settings
            .layouts
            .values()
            .any(|layout| layout.name == "Blender" && !layout.automatic_switching));
        assert!(!settings.provision_builtin_presets(Some(&installed)));
        assert_eq!(settings.layouts.len(), 15);
    }

    #[test]
    fn catalog_failure_still_provisions_profiles_and_recovers_autobinding() {
        let mut settings = DeviceApplicationLayouts::default();
        assert!(settings.provision_builtin_presets(None));
        assert_eq!(settings.layouts.len(), 15);
        assert_eq!(settings.pending_builtin_auto_bind.len(), 14);
        let audacity_id = settings
            .layouts
            .values()
            .find(|layout| layout.name == "Audacity")
            .unwrap()
            .id
            .clone();
        settings
            .layouts
            .get_mut(&audacity_id)
            .unwrap()
            .set_keycode(0, 0, 0x1234);
        let restored: DeviceApplicationLayouts =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        settings = restored;
        let found = ["audacity".to_owned()].into_iter().collect();
        assert!(settings.provision_builtin_presets(Some(&found)));
        assert_eq!(settings.pending_builtin_auto_bind.len(), 13);
        assert!(settings.layouts[&audacity_id].automatic_switching);
        assert_eq!(settings.layouts[&audacity_id].layers[0][0], 0x1234);
        assert!(!settings.provision_builtin_presets(Some(&found)));
        let later = ["audacity".to_owned(), "blender".to_owned()]
            .into_iter()
            .collect();
        assert!(settings.provision_builtin_presets(Some(&later)));
        assert_eq!(settings.pending_builtin_auto_bind.len(), 12);
        assert!(settings
            .layouts
            .values()
            .any(|layout| layout.name == "Blender" && layout.automatic_switching));
    }

    #[test]
    fn catalog_upgrade_adds_photoshop_and_only_fills_untouched_second_layers() {
        let presets = builtin_application_layout_presets_for(ShortcutPlatform::WindowsLinux);
        let obs = presets
            .iter()
            .find(|preset| preset.id == "obs_studio")
            .unwrap();
        let blender = presets
            .iter()
            .find(|preset| preset.id == "blender")
            .unwrap();
        let mut settings = DeviceApplicationLayouts::default();
        settings
            .layouts
            .get_mut(DEFAULT_APPLICATION_LAYOUT_ID)
            .unwrap()
            .layers[0][9] = 0x1234;
        settings
            .layouts
            .get_mut(DEFAULT_APPLICATION_LAYOUT_ID)
            .unwrap()
            .layers[1][0] = 0x5678;
        let obs_id = settings.create_from_preset(obs);
        let blender_id = settings.create_from_preset(blender);
        settings.layouts.get_mut(&obs_id).unwrap().layers[0][9] = 0x1234;
        settings.layouts.get_mut(&obs_id).unwrap().layers[1] =
            settings.layouts[DEFAULT_APPLICATION_LAYOUT_ID].layers[1];
        settings.layouts.get_mut(&obs_id).unwrap().layer_names[1] =
            default_layer_names()[1].clone();
        settings.layouts.get_mut(&blender_id).unwrap().layers[1][0] = 0x1234;
        settings.builtin_presets_provisioned = true; // Saved by the previous catalog.
        let installed = ["adobe_photoshop".to_owned()].into_iter().collect();

        assert!(settings.provision_builtin_presets(Some(&installed)));
        assert_eq!(settings.layouts[&obs_id].layers[1], obs.layers[1].keycodes);
        assert_eq!(settings.layouts[&blender_id].layers[1][0], 0x1234);
        let photoshop_id = settings
            .layouts
            .values()
            .find(|layout| layout.name == "Adobe Photoshop")
            .unwrap()
            .id
            .clone();
        assert!(settings.layouts[&photoshop_id].automatic_switching);
        assert_eq!(settings.layouts.len(), 11); // Only new catalog entries are provisioned.
        assert!(!settings.provision_builtin_presets(Some(&installed)));
        assert!(settings.remove(&photoshop_id));
        assert!(!settings.provision_builtin_presets(Some(&installed)));
        assert_eq!(settings.layouts.len(), 10);
    }

    #[test]
    fn catalog_revision_four_adds_new_profiles_without_restoring_deleted_ones() {
        let presets = builtin_application_layout_presets_for(ShortcutPlatform::WindowsLinux);
        let code = presets
            .iter()
            .find(|preset| preset.id == "visual_studio_code")
            .unwrap();
        let mut settings = DeviceApplicationLayouts::default();
        let code_id = settings.create_from_preset(code);
        settings.layouts.get_mut(&code_id).unwrap().layers[0][0] = 0x1234;
        settings.builtin_presets_provisioned = true;
        settings.builtin_presets_catalog_revision = 3;

        assert!(settings.provision_builtin_presets(Some(&Default::default())));
        assert_eq!(settings.layouts.len(), 9); // Default, existing Code, seven new profiles.
        assert_eq!(settings.layouts[&code_id].layers[0][0], 0x1234);
        assert!(!settings
            .layouts
            .values()
            .any(|layout| layout.name == "Adobe Photoshop"));
        assert!(settings
            .layouts
            .values()
            .any(|layout| layout.name == "Google Chrome"));
        assert!(!settings.provision_builtin_presets(Some(&Default::default())));
    }

    #[test]
    fn catalog_revision_five_removes_only_untouched_retired_layouts() {
        let presets = all_application_layout_presets_for(ShortcutPlatform::WindowsLinux);
        let mut settings = DeviceApplicationLayouts::default();
        let mut retired_ids = Vec::new();
        for preset in presets
            .iter()
            .filter(|preset| retired_builtin_preset(preset.id))
        {
            retired_ids.push(settings.create_from_preset(preset));
        }
        let edited_id = retired_ids[1].clone();
        settings
            .layouts
            .get_mut(&edited_id)
            .unwrap()
            .set_keycode(0, 0, 0x1234);
        settings.active_layout_id = retired_ids[0].clone();
        settings.editor_layout_id = retired_ids[2].clone();
        settings.builtin_presets_provisioned = true;
        settings.builtin_presets_catalog_revision = 4;

        assert!(settings.provision_builtin_presets(Some(&Default::default())));
        for id in &retired_ids {
            assert_eq!(settings.layouts.contains_key(id), id == &edited_id);
        }
        assert_eq!(settings.active_layout_id, DEFAULT_APPLICATION_LAYOUT_ID);
        assert_eq!(settings.editor_layout_id, DEFAULT_APPLICATION_LAYOUT_ID);
        assert_eq!(settings.layouts[&edited_id].layers[0][0], 0x1234);
        assert!(!settings.provision_builtin_presets(Some(&Default::default())));
    }

    #[test]
    fn catalog_revision_six_removes_blank_streamlabs_but_keeps_customized_layout() {
        let streamlabs = all_application_layout_presets_for(ShortcutPlatform::WindowsLinux)
            .into_iter()
            .find(|preset| preset.id == "streamlabs_desktop")
            .unwrap();
        let mut settings = DeviceApplicationLayouts::default();
        let blank_id = settings.create_from_preset(&streamlabs);
        let mut customized = settings.clone();
        customized
            .layouts
            .get_mut(&blank_id)
            .unwrap()
            .set_keycode(0, 0, 0x1234);
        customized.builtin_presets_provisioned = true;
        customized.builtin_presets_catalog_revision = 5;
        settings.active_layout_id = blank_id.clone();
        settings.editor_layout_id = blank_id.clone();
        settings.builtin_presets_provisioned = true;
        settings.builtin_presets_catalog_revision = 5;

        assert!(settings.provision_builtin_presets(Some(&Default::default())));
        assert!(!settings.layouts.contains_key(&blank_id));
        assert_eq!(settings.active_layout_id, DEFAULT_APPLICATION_LAYOUT_ID);
        assert_eq!(settings.editor_layout_id, DEFAULT_APPLICATION_LAYOUT_ID);
        assert!(!settings.provision_builtin_presets(Some(&Default::default())));
        assert!(customized.provision_builtin_presets(Some(&Default::default())));
        assert_eq!(customized.layouts[&blank_id].layers[0][0], 0x1234);
        assert!(
            !builtin_application_layout_presets_for(ShortcutPlatform::WindowsLinux)
                .iter()
                .any(|preset| preset.id == "streamlabs_desktop")
        );
    }

    #[test]
    fn catalog_revision_three_adds_third_layer_only_to_stock_profiles() {
        let presets = builtin_application_layout_presets_for(ShortcutPlatform::WindowsLinux);
        let code = presets
            .iter()
            .find(|preset| preset.id == "visual_studio_code")
            .unwrap();
        let mut settings = DeviceApplicationLayouts::default();
        let stock_id = settings.create_from_preset(code);
        settings.layouts.get_mut(&stock_id).unwrap().layers[2] = default_keycodes();
        settings.layouts.get_mut(&stock_id).unwrap().layer_names[2] =
            default_layer_names()[2].clone();
        settings.builtin_presets_provisioned = true;
        settings.builtin_presets_catalog_revision = 2;
        assert!(settings.provision_builtin_presets(None));
        assert_eq!(
            settings.layouts[&stock_id].layers[2],
            code.layers[2].keycodes
        );

        let mut edited_settings = DeviceApplicationLayouts::default();
        let edited_id = edited_settings.create_from_preset(code);
        edited_settings.layouts.get_mut(&edited_id).unwrap().layers[2] = default_keycodes();
        edited_settings
            .layouts
            .get_mut(&edited_id)
            .unwrap()
            .layer_names[2] = default_layer_names()[2].clone();
        edited_settings.layouts.get_mut(&edited_id).unwrap().layers[1][0] = 0x1234;
        edited_settings.builtin_presets_provisioned = true;
        edited_settings.builtin_presets_catalog_revision = 2;
        assert!(edited_settings.provision_builtin_presets(None));
        assert_eq!(edited_settings.layouts[&edited_id].layers[1][0], 0x1234);
        assert_eq!(
            edited_settings.layouts[&edited_id].layers[2],
            default_keycodes()
        );
    }

    #[test]
    fn encoder_stack_roundtrip_and_wire_packets_keep_selected_actions() {
        let mut layout = ApplicationLayout::default_layout();
        let actions = vec![
            EncoderStackAction {
                name: "Volume".into(),
                counter_clockwise: 0x00AA,
                clockwise: 0x00A9,
            },
            EncoderStackAction {
                name: "Zoom".into(),
                counter_clockwise: 0x0120,
                clockwise: 0x011E,
            },
        ];
        assert!(layout.set_encoder_stack(3, actions.clone()));
        assert!(layout.set_encoder_stack(
            15,
            vec![
                EncoderStackAction {
                    name: "A".into(),
                    counter_clockwise: 4,
                    clockwise: 5
                },
                EncoderStackAction {
                    name: "B".into(),
                    counter_clockwise: 6,
                    clockwise: 7
                },
                EncoderStackAction {
                    name: "C".into(),
                    counter_clockwise: 8,
                    clockwise: 9
                },
                EncoderStackAction {
                    name: "D".into(),
                    counter_clockwise: 10,
                    clockwise: 11
                },
            ]
        ));
        let restored: ApplicationLayout =
            serde_json::from_str(&serde_json::to_string(&layout).unwrap()).unwrap();
        assert_eq!(restored.encoder_stacks[3], actions);
        let packets = ApplicationLayoutSnapshot::from_layout(&restored).packets();
        let base = 1 + APPLICATION_LAYOUT_LAYER_COUNT * 2 + APPLICATION_LAYOUT_LAYER_COUNT;
        let slot0 = &packets[base + 3 * APPLICATION_LAYOUT_STACK_SLOTS];
        let slot1 = &packets[base + 3 * APPLICATION_LAYOUT_STACK_SLOTS + 1];
        assert_eq!(slot0[0], HID_APPLICATION_LAYOUT_ENCODER_STACK);
        assert_eq!((slot0[2], slot0[3], slot0[4]), (3, 0, 2));
        assert_eq!(&slot0[6..10], &[0xAA, 0, 0xA9, 0]);
        assert_eq!(&slot1[10..14], b"Zoom");
        assert_eq!(packets[base + 15 * APPLICATION_LAYOUT_STACK_SLOTS][4], 4);
        assert_eq!(
            ApplicationLayoutSnapshot::deactivate_packet()[0],
            HID_APPLICATION_LAYOUT_DEACTIVATE
        );
    }

    #[test]
    fn stack_actions_never_use_transparent_or_unset_fallback_codes() {
        let mut layout = ApplicationLayout::default_layout();
        assert!(layout.set_encoder_stack(
            0,
            vec![
                EncoderStackAction {
                    name: "Mute".into(),
                    counter_clockwise: 1,
                    clockwise: u16::MAX
                },
                EncoderStackAction {
                    name: "Volume".into(),
                    counter_clockwise: 0xAA,
                    clockwise: 0xA9
                },
            ]
        ));
        assert_eq!(layout.encoder_stacks[0][0].counter_clockwise, 0);
        assert_eq!(layout.encoder_stacks[0][0].clockwise, 0);
    }

    #[test]
    fn protocol_v7_crc_matches_firmware_field_order() {
        let mut layout = ApplicationLayout::default_layout();
        layout.set_encoder_stack(
            2,
            vec![
                EncoderStackAction {
                    name: "Volume".into(),
                    counter_clockwise: 0xA8,
                    clockwise: 0xA9,
                },
                EncoderStackAction {
                    name: "Zoom".into(),
                    counter_clockwise: 0x120,
                    clockwise: 0x11E,
                },
            ],
        );
        let packets = ApplicationLayoutSnapshot::from_layout(&layout).packets();
        let mut bytes = Vec::new();
        for packet in &packets[1..33] {
            bytes.extend_from_slice(&packet[5..5 + usize::from(packet[4]) * 2]);
        }
        for packet in &packets[33..49] {
            bytes.extend_from_slice(&packet[4..19]);
        }
        for layer in 0..APPLICATION_LAYOUT_LAYER_COUNT {
            let first = 49 + layer * APPLICATION_LAYOUT_STACK_SLOTS;
            bytes.push(packets[first][4]);
            for packet in &packets[first..first + APPLICATION_LAYOUT_STACK_SLOTS] {
                bytes.extend_from_slice(&packet[6..10]);
                bytes.push(packet[5]);
                bytes.extend_from_slice(&packet[10..10 + usize::from(packet[5])]);
            }
        }
        bytes.push(packets[0][9]);
        bytes.extend_from_slice(&packets[0][10..10 + usize::from(packets[0][9])]);
        for packet in &packets[113..129] {
            bytes.push(packet[3]);
            bytes.extend_from_slice(&packet[4..4 + usize::from(packet[3])]);
        }
        let mut crc = 0xffffu16;
        for byte in bytes {
            crc ^= u16::from(byte) << 8;
            for _ in 0..8 {
                crc = if crc & 0x8000 != 0 {
                    (crc << 1) ^ 0x1021
                } else {
                    crc << 1
                };
            }
        }
        assert_eq!(crc.to_le_bytes(), [packets[129][7], packets[129][8]]);
    }

    #[test]
    fn every_bound_builtin_preset_command_has_a_semantic_icon() {
        for platform in [ShortcutPlatform::MacOs, ShortcutPlatform::WindowsLinux] {
            for preset in builtin_application_layout_presets_for(platform) {
                for (layer, definition) in preset.layers.iter().enumerate() {
                    for control in (0..9).chain(12..15) {
                        let visual = crate::action_icons::preset_visual(preset.id, layer, control);
                        if definition.keycodes[control] != 0 {
                            assert!(
                                visual >= 12,
                                "{} layer {} control {} is missing an icon",
                                preset.id,
                                layer,
                                control
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn changed_preset_command_loses_its_old_icon_and_visuals_are_transmitted() {
        let preset = builtin_application_layout_presets_for(ShortcutPlatform::WindowsLinux)
            .into_iter()
            .find(|preset| preset.id == "google_chrome")
            .unwrap();
        let mut settings = DeviceApplicationLayouts::default();
        let id = settings.create_from_preset(&preset);
        let layout = settings.layouts.get(&id).unwrap();
        let snapshot = ApplicationLayoutSnapshot::from_layout(layout);
        assert_ne!(snapshot.visuals[0][0], 0);
        let packets = snapshot.packets();
        assert_eq!(packets[33][0], HID_APPLICATION_LAYOUT_VISUALS);
        assert_eq!(packets[33][4], snapshot.visuals[0][0]);
        assert_eq!(packets[34][4], snapshot.visuals[1][0]);
        let layout = settings.layouts.get_mut(&id).unwrap();
        layout.set_keycode(0, 0, 0x1234);
        let updated = ApplicationLayoutSnapshot::from_layout(layout);
        assert_eq!(updated.visuals[0][0], 0);
        assert_ne!(updated.visuals[0][1], 0);
        assert_ne!(
            updated.packets().last().unwrap()[7..9],
            packets.last().unwrap()[7..9]
        );
    }

    #[test]
    fn audacity_zoom_direction_is_out_then_in_on_both_platforms() {
        for platform in [ShortcutPlatform::MacOs, ShortcutPlatform::WindowsLinux] {
            let preset = builtin_application_layout_presets_for(platform)
                .into_iter()
                .find(|preset| preset.id == "audacity")
                .unwrap();
            let primary = if platform == ShortcutPlatform::MacOs {
                MOD_GUI
            } else {
                MOD_CTRL
            };
            assert_eq!(preset.layers[0].keycodes[13], chord(primary, 0x0020)); // Ctrl/Cmd+3 zoom out
            assert_eq!(preset.layers[0].keycodes[14], chord(primary, 0x001E)); // Ctrl/Cmd+1 zoom in
        }
    }

    #[test]
    fn built_in_presets_do_not_match_each_other() {
        let presets = builtin_application_layout_presets_for(ShortcutPlatform::WindowsLinux);
        for (index, left) in presets.iter().enumerate() {
            for right in presets.iter().skip(index + 1) {
                assert!(
                    !application_identities_match(
                        std::iter::once(left.executable).chain(left.identities.iter().copied()),
                        std::iter::once(right.executable).chain(right.identities.iter().copied()),
                    ),
                    "{} incorrectly matches {}",
                    left.id,
                    right.id,
                );
            }
        }
    }

    #[test]
    fn built_in_presets_use_only_the_layers_their_commands_need() {
        let presets = builtin_application_layout_presets_for(ShortcutPlatform::WindowsLinux);
        assert_eq!(presets.len(), 14);
        assert!(presets.iter().any(|preset| preset.id == "obs_studio"));

        let ids = presets
            .iter()
            .map(|preset| preset.id)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), presets.len());
        for preset in &presets {
            let expected = if matches!(
                preset.id,
                "visual_studio_code"
                    | "blender"
                    | "figma"
                    | "adobe_photoshop"
                    | "adobe_premiere_pro"
                    | "adobe_illustrator"
                    | "visual_studio"
                    | "intellij_idea"
                    | "pycharm"
            ) {
                3
            } else {
                2
            };
            assert_eq!(preset.layers.len(), expected, "{}", preset.id);
            for layer in &preset.layers {
                assert!(layer.keycodes[..9]
                    .iter()
                    .all(|keycode| *keycode != APPLICATION_LAYOUT_UNSET_KEYCODE));
                assert_eq!(
                    &layer.keycodes[9..12],
                    &[APPLICATION_LAYOUT_UNSET_KEYCODE; 3],
                    "{} must leave the stock bottom row available",
                    preset.id
                );
                assert!(layer.keycodes[12..]
                    .iter()
                    .all(|keycode| *keycode != APPLICATION_LAYOUT_UNSET_KEYCODE));
            }
        }
    }

    #[test]
    fn presets_use_command_on_macos_and_ctrl_elsewhere() {
        let mac = builtin_application_layout_presets_for(ShortcutPlatform::MacOs);
        let other = builtin_application_layout_presets_for(ShortcutPlatform::WindowsLinux);
        let mac_code = mac
            .iter()
            .find(|preset| preset.id == "visual_studio_code")
            .unwrap();
        let other_code = other
            .iter()
            .find(|preset| preset.id == "visual_studio_code")
            .unwrap();
        for code in [mac_code, other_code] {
            assert_eq!(code.layers[0].keycodes[2], MOD_CTRL | 0x0035);
            assert_eq!(code.layers[0].keycodes[13], MOD_CTRL | MOD_SHIFT | 0x002B);
            assert_eq!(code.layers[0].keycodes[14], MOD_CTRL | 0x002B);
        }

        for (presets, modifier) in [(&mac, MOD_GUI), (&other, MOD_CTRL)] {
            let figma = presets.iter().find(|preset| preset.id == "figma").unwrap();
            assert_eq!(figma.layers[0].keycodes[13], modifier | 0x002D);
            assert_eq!(figma.layers[0].keycodes[14], modifier | MOD_SHIFT | 0x002E);
            let firefox = presets
                .iter()
                .find(|preset| preset.id == "firefox")
                .unwrap();
            assert_eq!(firefox.layers[0].keycodes[7], MOD_CTRL | MOD_SHIFT | 0x002B);
            assert_eq!(firefox.layers[0].keycodes[8], MOD_CTRL | 0x002B);
        }

        let other_obs = other
            .iter()
            .find(|preset| preset.id == "obs_studio")
            .unwrap();
        assert_eq!(
            other_obs.layers[0].keycodes[1],
            MOD_CTRL | MOD_SHIFT | 0x001D
        );

        let mac_audacity = mac.iter().find(|preset| preset.id == "audacity").unwrap();
        let other_audacity = other.iter().find(|preset| preset.id == "audacity").unwrap();
        assert_eq!(mac_audacity.layers[0].keycodes[8], MOD_GUI | 0x001C);
        assert_eq!(other_audacity.layers[0].keycodes[8], MOD_CTRL | 0x001C);
        assert!(!other_audacity.layers[0].keycodes[..9].contains(&0x0015));

        let mac_firefox = mac.iter().find(|preset| preset.id == "firefox").unwrap();
        let other_firefox = other.iter().find(|preset| preset.id == "firefox").unwrap();
        assert_eq!(mac_firefox.layers[0].keycodes[3], MOD_GUI | 0x002F);
        assert_eq!(mac_firefox.layers[0].keycodes[4], MOD_GUI | 0x0030);
        assert_eq!(other_firefox.layers[0].keycodes[3], MOD_ALT | 0x0050);
        assert_eq!(other_firefox.layers[0].keycodes[4], MOD_ALT | 0x004F);
    }

    #[test]
    fn all_builtin_application_presets_have_a_category() {
        for preset in builtin_application_layout_presets() {
            assert_ne!(
                ApplicationLayoutCategory::for_preset_id(preset.id),
                ApplicationLayoutCategory::Other,
                "builtin preset {} must have a category",
                preset.id
            );
        }
        assert_eq!(
            ApplicationLayoutCategory::for_preset_id("unknown_app"),
            ApplicationLayoutCategory::Other
        );
    }

    #[test]
    fn category_override_is_optional_for_legacy_layouts_and_persists() {
        let mut settings = DeviceApplicationLayouts::default();
        let id = settings.create_for_application(&app("firefox", "Firefox"));
        let legacy = serde_json::to_value(&settings).unwrap();
        assert!(legacy["layouts"][&id].get("category").is_none());
        let mut restored: DeviceApplicationLayouts = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.layouts[&id].category, None);
        assert_eq!(
            restored.layouts[DEFAULT_APPLICATION_LAYOUT_ID].category,
            None
        );

        let revision = restored.layouts[&id].revision;
        assert!(!restored.set_layout_category(
            DEFAULT_APPLICATION_LAYOUT_ID,
            ApplicationLayoutCategory::Audio
        ));
        assert!(!restored.set_layout_category("missing", ApplicationLayoutCategory::Audio));
        assert!(restored.set_layout_category(&id, ApplicationLayoutCategory::Audio));
        assert_eq!(restored.layouts[&id].revision, revision + 1);
        assert!(!restored.set_layout_category(&id, ApplicationLayoutCategory::Audio));
        assert_eq!(restored.layouts[&id].revision, revision + 1);
        let json = serde_json::to_value(&restored).unwrap();
        assert_eq!(json["layouts"][&id]["category"], "audio");
        assert!(json["layouts"][DEFAULT_APPLICATION_LAYOUT_ID]
            .get("category")
            .is_none());
        let round_trip: DeviceApplicationLayouts = serde_json::from_value(json).unwrap();
        assert_eq!(
            round_trip.layouts[&id].category,
            Some(ApplicationLayoutCategory::Audio)
        );
        assert_eq!(round_trip.layouts[&id].revision, revision + 1);
    }

    #[test]
    fn editable_categories_keep_stable_ids_and_reassign_on_delete() {
        let mut settings = DeviceApplicationLayouts::default();
        let firefox = settings.create_for_application(&app("firefox", "Firefox"));
        let editor = settings.create_for_application(&app("code", "Code"));
        assert_eq!(
            settings.category_id_for_layout(&settings.layouts[&firefox]),
            "browsers"
        );
        assert!(!settings.remove_category("other"));
        let id = settings.create_category("Work").expect("new category");
        assert!(!settings.create_category("  work ").is_some());
        assert!(settings.set_layout_category_id(&firefox, &id));
        assert!(settings.rename_category(&id, "Projects"));
        let json = serde_json::to_string(&settings).unwrap();
        let mut restored: DeviceApplicationLayouts = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.category_names[&id], "Projects");
        assert_eq!(
            restored.category_id_for_layout(&restored.layouts[&firefox]),
            id
        );
        assert!(restored.remove_category(&id));
        assert_eq!(
            restored.category_id_for_layout(&restored.layouts[&firefox]),
            "other"
        );
        assert!(!restored.category_exists(&id));
        let next = restored.create_category("New").unwrap();
        assert_ne!(next, id);
        assert!(restored.remove_category("development"));
        assert_eq!(
            restored.category_id_for_layout(&restored.layouts[&editor]),
            "other"
        );
        assert!(restored.removed_categories.contains("development"));
        let json = serde_json::to_string(&restored).unwrap();
        let round_trip: DeviceApplicationLayouts = serde_json::from_str(&json).unwrap();
        assert!(!round_trip.category_exists("development"));
        assert_eq!(
            round_trip.category_id_for_layout(&round_trip.layouts[&editor]),
            "other"
        );
    }
}
