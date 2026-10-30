use super::portable_settings::{
    ent_portable_setting_contracts_match, ent_portable_setting_import_value,
    ent_portable_setting_is_known, ent_portable_setting_is_valid, ent_portable_setting_spec,
    write_entlayout_portable_setting, EntPortableSetting, EntPortableSettingWidth,
    ENT_PORTABLE_QSIDS,
};
use super::*;
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const ENTLAYOUT_FORMAT: &str = "entropy.layout";
const ENTLAYOUT_VERSION: u16 = 7;
// Complete Macropad device documents add every host profile and matching data.
const MACROPAD_ENTLAYOUT_VERSION: u16 = 9;

fn entlayout_version_supported(version: u16) -> bool {
    (1..=ENTLAYOUT_VERSION).contains(&version)
}

// Version 8 is the older explicitly scoped application document. Non-Macropad
// ordinary device exports retain version 7 and their original schema.
const APPLICATION_ENTLAYOUT_VERSION: u16 = 8;

#[derive(Serialize, Deserialize)]
struct ApplicationEntLayoutFile {
    format: String,
    version: u16,
    #[serde(default)]
    scope: Option<String>,
    profile: crate::application_layouts::ApplicationLayout,
}

enum EntLayoutDocument {
    Device(EntLayoutFile),
    Application(crate::application_layouts::ApplicationLayout),
}

fn read_entlayout_document(data: &str) -> Result<EntLayoutDocument> {
    let value: serde_json::Value = serde_json::from_str(data)?;
    match (value["format"].as_str(), value.get("scope")) {
        (Some(ENTLAYOUT_FORMAT), None) => {
            Ok(EntLayoutDocument::Device(serde_json::from_str(data)?))
        }
        (Some(ENTLAYOUT_FORMAT), Some(scope)) if scope == "application" => {
            let file: ApplicationEntLayoutFile = serde_json::from_str(data)?;
            if file.version != APPLICATION_ENTLAYOUT_VERSION {
                bail!("Unsupported application layout format or version");
            }
            Ok(EntLayoutDocument::Application(file.profile))
        }
        _ => bail!("Unsupported layout format or scope"),
    }
}

pub(super) fn read_application_entlayout(
    path: &Path,
) -> Result<crate::application_layouts::ApplicationLayout> {
    validate_application_entlayout_extension(path)?;
    match read_entlayout_document(&std::fs::read_to_string(path)?)? {
        EntLayoutDocument::Application(profile) => Ok(profile),
        EntLayoutDocument::Device(_) => {
            bail!("Device layout cannot be imported into an application target")
        }
    }
}

pub(super) fn write_application_entlayout(
    path: &Path,
    profile: crate::application_layouts::ApplicationLayout,
    language: crate::i18n::Language,
) -> Result<()> {
    validate_application_entlayout_extension(path)?;
    write_entlayout_file(
        path,
        &ApplicationEntLayoutFile {
            format: ENTLAYOUT_FORMAT.into(),
            version: APPLICATION_ENTLAYOUT_VERSION,
            scope: Some("application".into()),
            profile,
        },
        language,
    )
}

fn validate_application_entlayout_extension(path: &Path) -> Result<()> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("entlayout"))
    {
        bail!("Application layout files require the .entlayout extension");
    }
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
struct EntLayoutFile {
    format: String,
    version: u16,
    created_by: String,
    created_at: String,
    keyboard: EntLayoutKeyboard,
    data: EntLayoutData,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_application_profiles"
    )]
    application_layouts: Option<EntLayoutProfiles>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pictograms: Option<Vec<EntLayoutPictogram>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    application_categories: Vec<(String, String)>,
}

// Complete Macropad imports own a small filesystem transaction. Legacy writers
// intentionally retain their historical best-effort behavior.
#[derive(Default)]
struct EntLayoutSidecars {
    files: Vec<(
        PathBuf,
        Option<Vec<u8>>,
        Option<tempfile::NamedTempFile>,
        bool,
    )>,
    complete: bool,
}

impl EntLayoutSidecars {
    fn stage(&mut self, path: PathBuf, value: &impl Serialize) -> Result<()> {
        use std::io::Write;
        let original = match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Cannot stage metadata {}", path.display()))
            }
        };
        let parent = path.parent().context("Invalid metadata path")?;
        std::fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        let bytes = serde_json::to_vec(value)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        if std::fs::read(temporary.path())? != bytes {
            bail!("Metadata staging verification failed");
        }
        self.files.push((path, original, Some(temporary), false));
        Ok(())
    }

    fn persist(&mut self) -> Result<()> {
        for (path, _, temporary, applied) in &mut self.files {
            temporary
                .take()
                .context("Missing staged metadata")?
                .persist(&*path)
                .with_context(|| format!("Cannot persist metadata {}", path.display()))?;
            *applied = true;
        }
        Ok(())
    }
}

impl Drop for EntLayoutSidecars {
    fn drop(&mut self) {
        if self.complete {
            return;
        }
        for (path, original, _, applied) in self.files.iter().rev() {
            if !applied {
                continue;
            }
            let result = if let Some(bytes) = original {
                std::fs::write(path, bytes)
            } else {
                std::fs::remove_file(path)
            };
            if let Err(error) = result {
                log::error!("Metadata rollback failed for {}: {error}", path.display());
            }
        }
    }
}

type EntLayoutProfiles =
    std::collections::BTreeMap<String, crate::application_layouts::ApplicationLayout>;

fn deserialize_application_profiles<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<EntLayoutProfiles>, D::Error> {
    struct Profiles(EntLayoutProfiles);
    impl<'de> Deserialize<'de> for Profiles {
        fn deserialize<D: serde::Deserializer<'de>>(
            deserializer: D,
        ) -> std::result::Result<Self, D::Error> {
            struct Visitor;
            impl<'de> serde::de::Visitor<'de> for Visitor {
                type Value = Profiles;
                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("unique application layout IDs")
                }
                fn visit_map<M: serde::de::MapAccess<'de>>(
                    self,
                    mut access: M,
                ) -> std::result::Result<Profiles, M::Error> {
                    let mut profiles = EntLayoutProfiles::new();
                    while let Some((id, profile)) = access
                        .next_entry::<String, crate::application_layouts::ApplicationLayout>(
                    )? {
                        if profiles.insert(id, profile).is_some() {
                            return Err(serde::de::Error::custom(
                                "duplicate application layout ID",
                            ));
                        }
                    }
                    Ok(Profiles(profiles))
                }
            }
            deserializer.deserialize_map(Visitor)
        }
    }
    Option::<Profiles>::deserialize(deserializer)
        .map(|profiles| profiles.map(|profiles| profiles.0))
}

#[derive(Clone, Serialize, Deserialize)]
struct EntLayoutPictogram {
    tap_dance: bool,
    slot: usize,
    bitmap: Vec<u8>,
    color: [u8; 3],
}

#[derive(Clone, Serialize, Deserialize)]
struct EntLayoutKeyboard {
    name: String,
    firmware: FirmwareProtocol,
    keyboard_id: Option<u64>,
    layout_hash: String,
    layers: usize,
    keys: usize,
    encoders: usize,
    rows: usize,
    cols: usize,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntLayoutData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_layout: Option<EntLayoutSourceLayout>,
    keymap: Vec<Vec<u16>>,
    #[serde(default, skip_serializing_if = "native_keymap_is_empty")]
    native_keymap: Vec<Vec<Option<String>>>,
    encoder_keymap: Vec<Vec<u16>>,
    encoder_visibility: Vec<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    layout_element_visibility: Option<LayoutElementVisibility>,
    layout_options: Option<u32>,
    layer_names: Vec<String>,
    text_expander: EntTextExpanderData,
    macros: EntMacroData,
    combos: EntComboData,
    tap_dance: EntTapDanceData,
    key_overrides: EntKeyOverrideData,
    alt_repeat: EntAltRepeatData,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    portable_settings: Vec<EntPortableSetting>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct EntPortableSettingsImportReport {
    imported: usize,
    unsupported: usize,
    incompatible: usize,
    failed: usize,
}

impl EntPortableSettingsImportReport {
    fn total(self) -> usize {
        self.imported + self.unsupported + self.incompatible + self.failed
    }

    fn record_imported(&mut self) {
        self.imported += 1;
    }

    fn record_unsupported(&mut self) {
        self.unsupported += 1;
    }

    fn record_incompatible(&mut self) {
        self.incompatible += 1;
    }

    fn record_failed(&mut self) {
        self.failed += 1;
    }

    fn summary(&self, language: crate::i18n::Language) -> String {
        if self.total() == 0 {
            return crate::i18n::tr_catalog(language, "entlayout.none").to_owned();
        }

        let imported = self.imported.to_string();
        let total = self.total().to_string();
        if self.imported == self.total() {
            return crate::i18n::tr_catalog_format(
                language,
                "entlayout.portable_summary_full",
                &[("imported", &imported), ("total", &total)],
            );
        }

        let mut reasons = Vec::new();
        for (count, key) in [
            (self.unsupported, "entlayout.portable_unsupported"),
            (self.incompatible, "entlayout.portable_incompatible"),
            (self.failed, "entlayout.portable_failed"),
        ] {
            if count > 0 {
                reasons.push(crate::i18n::tr_catalog_format(
                    language,
                    key,
                    &[("count", &count.to_string())],
                ));
            }
        }
        crate::i18n::tr_catalog_format(
            language,
            "entlayout.portable_summary_skipped",
            &[
                ("imported", &imported),
                ("total", &total),
                ("reasons", &reasons.join(", ")),
            ],
        )
    }
}

struct EntPortableSettingsImportPlan {
    compatible: Vec<EntPortableSetting>,
    report: EntPortableSettingsImportReport,
}

fn plan_entlayout_portable_settings_import(
    settings: &[EntPortableSetting],
    target_settings: &[EntPortableSetting],
) -> EntPortableSettingsImportPlan {
    let mut compatible = Vec::new();
    let mut report = EntPortableSettingsImportReport::default();
    let mut seen = std::collections::BTreeSet::new();

    for setting in settings {
        if setting.module.is_none() && !ent_portable_setting_is_known(setting) {
            report.record_incompatible();
            continue;
        }
        if ((setting.module.is_some() || setting.is_layer_led_color())
            && settings
                .iter()
                .filter(|entry| entry.qsid == setting.qsid)
                .count()
                != 1)
            || !seen.insert(setting.qsid)
            || !ent_portable_setting_is_valid(setting)
        {
            report.record_incompatible();
            continue;
        }
        let Some(target) = target_settings
            .iter()
            .find(|target| target.qsid == setting.qsid)
        else {
            report.record_unsupported();
            continue;
        };
        if !ent_portable_setting_contracts_match(setting, target) {
            report.record_incompatible();
            continue;
        }
        if setting.is_layer_led_color()
            && target_settings
                .iter()
                .filter(|entry| entry.id == setting.id)
                .any(|expected| {
                    let mut entries = settings.iter().filter(|entry| entry.qsid == expected.qsid);
                    let matches = entries.next().is_some_and(|entry| {
                        entry.value == setting.value
                            && ent_portable_setting_contracts_match(entry, expected)
                    });
                    !matches || entries.next().is_some()
                })
        {
            report.record_incompatible();
            continue;
        }
        let mut setting = setting.clone();
        setting.value = ent_portable_setting_import_value(&setting, target);
        // The layer LED reader only reads the primary register. Its cached
        // value cannot prove that linked registers match after a partial write.
        let has_linked_colors = setting.is_layer_led_color()
            && target_settings
                .iter()
                .any(|other| other.id == setting.id && other.qsid != setting.qsid);
        if setting.value == target.value && !has_linked_colors {
            // Already matching values are successful imports without another
            // flash write or BLE round trip.
            report.record_imported();
            continue;
        }
        compatible.push(setting);
    }

    EntPortableSettingsImportPlan { compatible, report }
}

fn native_keymap_is_empty(keymap: &[Vec<Option<String>>]) -> bool {
    keymap.iter().all(|layer| layer.iter().all(Option::is_none))
}

fn encode_native_key_action(action: rmk_types::action::KeyAction) -> Option<String> {
    let mut encoded = [0u8; 24];
    postcard::to_slice(&action, &mut encoded)
        .ok()
        .map(|bytes| BASE64_STANDARD.encode(bytes))
}

fn decode_native_key_action(encoded: &str) -> Result<rmk_types::action::KeyAction> {
    let bytes = BASE64_STANDARD
        .decode(encoded)
        .context("invalid base64 RMK key action")?;
    postcard::from_bytes(&bytes).context("invalid RMK key action payload")
}

fn entlayout_dynamic_binding(
    keycode: u16,
    native_action: Option<&str>,
) -> Result<crate::keyboard::KeyBinding> {
    native_action
        .map(decode_native_key_action)
        .transpose()
        .map(|action| {
            action
                .map(crate::keyboard::KeyBinding::Rmk)
                .unwrap_or_else(|| crate::keyboard::KeyBinding::Vial(keycode))
        })
}

fn entlayout_native_key_action(
    bundle: &EntLayoutFile,
    layer: usize,
    key: usize,
) -> Result<Option<rmk_types::action::KeyAction>> {
    bundle
        .data
        .native_keymap
        .get(layer)
        .and_then(|layer| layer.get(key))
        .and_then(Option::as_deref)
        .map(decode_native_key_action)
        .transpose()
}

#[derive(Clone, Serialize, Deserialize)]
struct EntLayoutSourceLayout {
    keys: Vec<EntLayoutSourceKey>,
    encoders: Vec<EntLayoutSourceEncoder>,
    custom_keycodes: Vec<EntLayoutSourceCustomKeycode>,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntLayoutSourceKey {
    index: usize,
    row: u8,
    col: u8,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rotation: f32,
    rotation_x: f32,
    rotation_y: f32,
    label: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntLayoutSourceEncoder {
    visual_index: usize,
    encoder_idx: u8,
    direction: u8,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rotation: f32,
    rotation_x: f32,
    rotation_y: f32,
    label: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntLayoutSourceCustomKeycode {
    index: usize,
    name: String,
    label: String,
    title: String,
}

struct EntLayoutImportMapping {
    exact_layout: bool,
    key_mapping: Vec<Option<usize>>,
    encoder_mapping: Vec<Option<usize>>,
}

impl EntLayoutImportMapping {
    fn mapped_keys(&self) -> usize {
        self.key_mapping.iter().filter(|idx| idx.is_some()).count()
    }

    fn mapped_encoders(&self) -> usize {
        self.encoder_mapping
            .iter()
            .filter(|idx| idx.is_some())
            .count()
    }
}

fn map_entlayout_element_visibility(
    source: &LayoutElementVisibility,
    source_layout: Option<&EntLayoutSourceLayout>,
    mapping: &EntLayoutImportMapping,
    target: &KeyboardLayout,
    current: &LayoutElementVisibility,
) -> LayoutElementVisibility {
    if mapping.exact_layout {
        return LayoutElementVisibility {
            hidden_keys: source
                .hidden_keys
                .iter()
                .copied()
                .filter(|&(row, col)| {
                    target
                        .keys
                        .iter()
                        .any(|key| key.row == row && key.col == col)
                })
                .collect(),
            hidden_encoders: source
                .hidden_encoders
                .iter()
                .copied()
                .filter(|index| {
                    target
                        .encoders
                        .iter()
                        .any(|encoder| encoder.encoder_idx == *index)
                })
                .collect(),
        };
    }

    // Universal import changes only positions that were actually mapped.
    let Some(source_layout) = source_layout else {
        return current.clone();
    };
    let mut result = current.clone();
    for (source_idx, key) in source_layout.keys.iter().enumerate() {
        let Some(target_idx) = mapping.key_mapping.get(source_idx).copied().flatten() else {
            continue;
        };
        let Some(target_key) = target.keys.get(target_idx) else {
            continue;
        };
        let position = (target_key.row, target_key.col);
        if source.hidden_keys.contains(&(key.row, key.col)) {
            result.hidden_keys.insert(position);
        } else {
            result.hidden_keys.remove(&position);
        }
    }

    // Both directions of an encoder belong to one cosmetic group. Do not
    // transfer a preference if either side is unmapped, split across groups,
    // or conflated with another source group on the target.
    let mut source_groups: std::collections::BTreeMap<u8, Vec<u8>> =
        std::collections::BTreeMap::new();
    let mut target_sources: std::collections::BTreeMap<u8, Vec<u8>> =
        std::collections::BTreeMap::new();
    let mut target_visuals: std::collections::BTreeMap<u8, std::collections::BTreeSet<usize>> =
        std::collections::BTreeMap::new();
    for (source_idx, encoder) in source_layout.encoders.iter().enumerate() {
        let Some(target_idx) = mapping.encoder_mapping.get(source_idx).copied().flatten() else {
            continue;
        };
        let Some(target_encoder) = target.encoders.get(target_idx) else {
            continue;
        };
        source_groups
            .entry(encoder.encoder_idx)
            .or_default()
            .push(target_encoder.encoder_idx);
        target_sources
            .entry(target_encoder.encoder_idx)
            .or_default()
            .push(encoder.encoder_idx);
        target_visuals
            .entry(target_encoder.encoder_idx)
            .or_default()
            .insert(target_idx);
    }
    for (source_index, target_indices) in source_groups {
        let source_count = source_layout
            .encoders
            .iter()
            .filter(|encoder| encoder.encoder_idx == source_index)
            .count();
        if target_indices.len() != source_count
            || target_indices
                .iter()
                .any(|index| *index != target_indices[0])
        {
            continue;
        }
        let target_index = target_indices[0];
        let target_count = target
            .encoders
            .iter()
            .filter(|encoder| encoder.encoder_idx == target_index)
            .count();
        if !target_sources.get(&target_index).is_some_and(|sources| {
            sources.len() == target_count
                && target_visuals
                    .get(&target_index)
                    .is_some_and(|indices| indices.len() == target_count)
                && sources.iter().all(|index| *index == source_index)
        }) {
            continue;
        }
        if source.hidden_encoders.contains(&source_index) {
            result.hidden_encoders.insert(target_index);
        } else {
            result.hidden_encoders.remove(&target_index);
        }
    }
    result
}

#[derive(Clone, Copy)]
struct EntLayoutKeycodeImportLimits {
    layer_count: usize,
    macro_count: usize,
    tap_dance_count: usize,
}

#[derive(Clone, Copy)]
enum EntLayoutSkipReason {
    UnsupportedSlot,
    UnmappableCustomKeycode,
    MissingLayer,
    MissingMacroSlot,
    MissingTapDanceSlot,
}

/// What an import's firmware writes did not land: a line for each failure,
/// and the dynamic sections a write failed in.
#[derive(Debug, Default)]
struct EntFirmwareFailures {
    lines: Vec<String>,
    sections: Vec<&'static str>,
}

impl EntFirmwareFailures {
    fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    fn push(&mut self, line: String) {
        self.lines.push(line);
    }

    /// A failed write of the dynamic section `label_key` names.
    fn push_section(&mut self, label_key: &'static str, line: String) {
        self.sections.push(label_key);
        self.lines.push(line);
    }
}

#[derive(Default)]
struct EntLayoutSectionImportStats {
    imported: usize,
    unsupported_slot: usize,
    unmappable_custom_keycode: usize,
    missing_layer: usize,
    missing_macro_slot: usize,
    missing_tap_dance_slot: usize,
}

impl EntLayoutSectionImportStats {
    fn skipped(&self) -> usize {
        self.unsupported_slot
            + self.unmappable_custom_keycode
            + self.missing_layer
            + self.missing_macro_slot
            + self.missing_tap_dance_slot
    }

    fn record_imported(&mut self) {
        self.imported += 1;
    }

    fn record_skipped(&mut self, reason: EntLayoutSkipReason) {
        match reason {
            EntLayoutSkipReason::UnsupportedSlot => self.unsupported_slot += 1,
            EntLayoutSkipReason::UnmappableCustomKeycode => self.unmappable_custom_keycode += 1,
            EntLayoutSkipReason::MissingLayer => self.missing_layer += 1,
            EntLayoutSkipReason::MissingMacroSlot => self.missing_macro_slot += 1,
            EntLayoutSkipReason::MissingTapDanceSlot => self.missing_tap_dance_slot += 1,
        }
    }

    fn summary(&self, language: crate::i18n::Language, label_key: &'static str) -> String {
        let label = crate::i18n::tr_catalog(language, label_key);
        let imported = self.imported.to_string();
        let total = (self.imported + self.skipped()).to_string();
        if self.skipped() == 0 {
            return crate::i18n::tr_catalog_format(
                language,
                "entlayout.summary_full",
                &[
                    ("section", label),
                    ("imported", &imported),
                    ("total", &total),
                ],
            );
        }
        let mut reasons = Vec::new();
        if self.unsupported_slot > 0 {
            reasons.push(crate::i18n::tr_catalog_format(
                language,
                "entlayout.unsupported_slot",
                &[("count", &self.unsupported_slot.to_string())],
            ));
        }
        if self.unmappable_custom_keycode > 0 {
            reasons.push(crate::i18n::tr_catalog_format(
                language,
                "entlayout.unmappable_custom_keycode",
                &[("count", &self.unmappable_custom_keycode.to_string())],
            ));
        }
        if self.missing_layer > 0 {
            reasons.push(crate::i18n::tr_catalog_format(
                language,
                "entlayout.missing_layer",
                &[("count", &self.missing_layer.to_string())],
            ));
        }
        if self.missing_macro_slot > 0 {
            reasons.push(crate::i18n::tr_catalog_format(
                language,
                "entlayout.missing_macro_slot",
                &[("count", &self.missing_macro_slot.to_string())],
            ));
        }
        if self.missing_tap_dance_slot > 0 {
            reasons.push(crate::i18n::tr_catalog_format(
                language,
                "entlayout.missing_tap_dance_slot",
                &[("count", &self.missing_tap_dance_slot.to_string())],
            ));
        }
        crate::i18n::tr_catalog_format(
            language,
            "entlayout.summary_skipped",
            &[
                ("section", label),
                ("imported", &imported),
                ("total", &total),
                ("reasons", &reasons.join(", ")),
            ],
        )
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct EntTextExpanderData {
    enabled: bool,
    app_blacklist: String,
    rule_files: Vec<String>,
    primary_rules: Vec<crate::text_expander::TextExpansionRule>,
    extra_files: Vec<EntTextExpanderRuleFile>,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntTextExpanderRuleFile {
    file_name: String,
    rules: Vec<crate::text_expander::TextExpansionRule>,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntMacroData {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    bytecode_base64: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    texts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    names: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    descriptions: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntComboData {
    entries: Vec<EntComboEntry>,
    names: Vec<String>,
    #[serde(default)]
    colors: Vec<u32>,
    term: Option<u16>,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntComboEntry {
    keys: [u16; 4],
    output: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    native_output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    layer: Option<u8>,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntTapDanceData {
    entries: Vec<EntTapDanceEntry>,
    names: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntTapDanceEntry {
    on_tap: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    native_on_tap: Option<String>,
    on_hold: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    native_on_hold: Option<String>,
    on_double_tap: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    native_on_double_tap: Option<String>,
    on_tap_hold: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    native_on_tap_hold: Option<String>,
    tapping_term: u16,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntKeyOverrideData {
    entries: Vec<EntKeyOverrideEntry>,
    names: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntKeyOverrideEntry {
    trigger: u16,
    replacement: u16,
    layers: u16,
    trigger_mods: u8,
    negative_mod_mask: u8,
    suppressed_mods: u8,
    options: u8,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntAltRepeatData {
    entries: Vec<EntAltRepeatEntry>,
    names: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntAltRepeatEntry {
    keycode: u16,
    alt_keycode: u16,
    allowed_mods: u8,
    options: u8,
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn export_entlayout_dialog(&mut self) {
        // The safe snapshot gate also protects unattended backups. Explicit
        // export must recover names before that gate, not only after the picker.
        if self.selected_layer_names_uncertain() && self.hid_write_task_active() {
            // A worker temporarily owns HID; resume through the existing full-load
            // queue instead of misreporting a disconnected keyboard or retrying SET.
            self.deferred_full_layout_action = Some(DeferredFullLayoutAction::ExportEntlayout);
            return;
        }
        if let Err(error) = self.confirm_uncertain_layer_names() {
            self.status_msg = error.to_string();
            return;
        }
        let Some(bundle) = self.entlayout_snapshot() else {
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "entlayout.connect_keyboard_before_exporting_layout",
            )
            .into();
            return;
        };
        let file_name = format!("{}.entlayout", device_id_slug(&bundle.keyboard.name));
        // The picker runs on a worker thread; the actual write happens in
        // write_entlayout_export once a path comes back. Snapshotting again then
        // keeps the export current even if the dialog was open for a while.
        self.spawn_file_dialog(
            crate::app::file_dialog::FileDialogAction::ExportEntlayout,
            rfd::FileDialog::new()
                .add_filter("Entropy layout", &["entlayout"])
                .set_file_name(&file_name),
            true,
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn write_entlayout_export(&mut self, path: &Path) {
        if let Err(error) = self.confirm_uncertain_layer_names() {
            self.status_msg = error.to_string();
            return;
        }
        if let Err(error) = self.prepare_entlayout_pictograms() {
            self.status_msg =
                format!("Layout export failed while reading shared pictograms: {error}");
            return;
        }
        let Some(bundle) = self.entlayout_snapshot() else {
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "entlayout.connect_keyboard_before_exporting_layout",
            )
            .into();
            return;
        };
        match self
            .validate_entlayout_file(&bundle)
            .and_then(|()| write_entlayout_file(path, &bundle, self.app_settings.language))
        {
            Ok(()) => {
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "entlayout.exported_layout",
                    &[("path", &path.display().to_string())],
                )
            }
            Err(e) => {
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "entlayout.export_failed",
                    &[("error", &e.to_string())],
                )
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn import_entlayout_dialog(&mut self) {
        self.spawn_file_dialog(
            crate::app::file_dialog::FileDialogAction::ImportEntlayout,
            rfd::FileDialog::new().add_filter("Entropy layout", &["entlayout"]),
            false,
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn begin_entlayout_import(&mut self, path: std::path::PathBuf) {
        // Remember which connection this import belongs to; the deferred write
        // in handle_pending_imports re-checks it before touching the device.
        self.pending_entlayout_import_path = Some((path, self.connection_generation));
        self.import_progress_started_at = None;
        self.import_progress_title =
            crate::i18n::tr_catalog(self.app_settings.language, "entlayout.importing_layout")
                .into();
        self.import_progress_body = crate::i18n::tr_catalog(
            self.app_settings.language,
            "entlayout.applying_layout_import",
        )
        .into();
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn import_entlayout_from_path(&mut self, path: &Path) -> Result<String> {
        let lang = self.app_settings.language;
        let data = std::fs::read_to_string(path).with_context(|| {
            crate::i18n::tr_catalog_format(
                lang,
                "entlayout.failed_to_read",
                &[("path", &path.display().to_string())],
            )
        })?;
        let bundle = match read_entlayout_document(&data).context(
            crate::i18n::tr_catalog(lang, "entlayout.invalid_entlayout_json"),
        )? {
            EntLayoutDocument::Device(bundle) => bundle,
            EntLayoutDocument::Application(_) => bail!(match lang {
                crate::i18n::Language::Russian => "Файл содержит только одну раскладку приложения, а не полную резервную копию Macropad. Создайте полную копию через «Экспорт раскладки» в исходном приложении; данные не изменены.",
                crate::i18n::Language::English => "This file contains a single application layout, not a complete Macropad backup. Create a complete backup using Export layout in the source application; no data was changed.",
            }),
        };
        // Reject malformed documents before even read-only preparation. Resolve
        // cold capability only after schema/resource validation, before backup.
        self.validate_entlayout_file_with_capability(&bundle, false)?;
        if bundle.application_layouts.is_some() {
            self.prepare_entlayout_pictograms()?;
        }
        self.validate_entlayout_file(&bundle)?;
        let backup_path = self.write_entlayout_auto_backup()?;
        let (firmware_failures, mapping, portable_settings) = self.apply_entlayout(&bundle)?;
        Ok(self.entlayout_import_report(
            &bundle,
            &backup_path,
            &firmware_failures,
            &mapping,
            &portable_settings,
        ))
    }

    /// Read the production library on a cold connection, independently of the
    /// settings UI. Only explicit QUERY command/format rejection proves lack of
    /// support; transport, status and data-read errors must abort portability.
    #[cfg(not(target_arch = "wasm32"))]
    fn prepare_entlayout_pictograms(&mut self) -> Result<()> {
        // Optional connect reads may fail without disconnecting the keyboard.
        // Retry only the read-only handshake; never interpret a timeout as v7.
        let retry_probe = self
            .selected_device
            .and_then(|index| self.device_manager.devices().get(index))
            .filter(|device| device.is_ergohaven_display_macropad())
            .is_some_and(|device| {
                self.device_about_info.as_ref().is_some_and(|info| {
                    info.application_layout_probe_failed
                        && info.vendor_id == device.vendor_id
                        && info.product_id == device.product_id
                        && info.path == device.path
                })
            });
        if retry_probe {
            let supported = self
                .hid_device
                .as_ref()
                .context("No active keyboard connection")?
                .probe_application_layout_protocol()
                .context("Application layout capability is unconfirmed; export refused")?;
            let info = self.device_about_info.as_mut().unwrap();
            info.supports_application_layouts = supported;
            info.application_layout_probe_failed = false;
        }
        if self.application_layout_device_key().is_none() {
            return Ok(());
        }
        let state = &self.display_settings.pictograms;
        if state.loading || state.saving || self.vial_hid_task.is_some() {
            bail!("Shared pictogram storage is busy");
        }
        if state.supported == Some(false) || (state.supported == Some(true) && state.loaded) {
            return Ok(());
        }
        let result = self
            .hid_device
            .as_ref()
            .context("No active keyboard connection")?
            .load_pictograms(&std::sync::atomic::AtomicU32::new(0));
        match result {
            Ok(library) => {
                let state = &mut self.display_settings.pictograms;
                state.library = library;
                state.supported = Some(true);
                state.loaded = true;
                state.load_failure = None;
                Ok(())
            }
            Err(error)
                if self.display_settings.pictograms.supported.is_none()
                    && matches!(
                        error.to_string().as_str(),
                        "pictogram command 0xC0 is unsupported" | "unsupported pictogram format"
                    ) =>
            {
                self.display_settings.pictograms.supported = Some(false);
                self.display_settings.pictograms.loaded = false;
                Ok(())
            }
            Err(error) => Err(error.context("Failed to prepare shared pictograms")),
        }
    }

    /// The `.entlayout` document for the connected keyboard, as the file
    /// dialog export would write it; `None` before a keyboard is connected.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn entlayout_export_json(&mut self) -> Option<Result<String>> {
        self.layout.as_ref()?;
        if let Err(error) = self.confirm_uncertain_layer_names() {
            return Some(Err(error));
        }
        if let Err(error) = self.prepare_entlayout_pictograms() {
            return Some(Err(error));
        }
        let bundle = self.entlayout_snapshot()?;
        if let Err(error) = self.validate_entlayout_file(&bundle) {
            return Some(Err(error));
        }
        Some(
            serde_json::to_string_pretty(&bundle).context(crate::i18n::tr_catalog(
                self.app_settings.language,
                "entlayout.failed_to_serialize",
            )),
        )
    }

    fn entlayout_snapshot(&self) -> Option<EntLayoutFile> {
        // Even callers without a transport (including automatic backups) must
        // not serialize a last-known cache as confirmed after a failed SET/GET.
        #[cfg(not(target_arch = "wasm32"))]
        if self.selected_layer_names_uncertain() {
            return None;
        }
        let layout = self.layout.as_ref()?;
        // A never-opened application settings page must not downgrade a capable
        // Macropad backup. Default inherits firmware without publishing host data.
        let fresh_settings = self.application_layout_device_key().map(|_| {
            let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
            let profile = settings
                .layouts
                .get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
                .unwrap();
            for (target, source) in profile.layer_names.iter_mut().zip(&self.layer_names) {
                *target = source.clone();
            }
            settings
        });
        let application_settings = self
            .application_layout_settings()
            .or(fresh_settings.as_ref());
        // The ordinary layout file is always Default, never the foreground
        // application's host overlay. Include local Default edits even while
        // their standalone firmware write is still pending.
        let default_profile = self.selected_device.and_then(|_| {
            application_settings.and_then(|settings| {
                settings
                    .layouts
                    .get(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
            })
        });
        let default_layout = default_profile.map(|profile| {
            let mut rendered =
                Self::application_layout_rendered_copy_for_profile(layout, Some(profile));
            rendered.layers.truncate(layout.layers.len());
            rendered
                .encoder_layers
                .truncate(layout.encoder_layers.len());
            // Fresh/migrated profiles use UNSET as an inheritance marker.
            // It is not a portable keycode and must never replace firmware data.
            for (target, source) in rendered.layers.iter_mut().zip(&layout.layers) {
                for (target, source) in target.iter_mut().zip(source) {
                    if target.vial_keycode()
                        == crate::application_layouts::APPLICATION_LAYOUT_UNSET_KEYCODE
                    {
                        *target = *source;
                    }
                }
            }
            for (target, source) in rendered
                .encoder_layers
                .iter_mut()
                .zip(&layout.encoder_layers)
            {
                for (target, source) in target.iter_mut().zip(source) {
                    if *target == crate::application_layouts::APPLICATION_LAYOUT_UNSET_KEYCODE {
                        *target = *source;
                    }
                }
            }
            rendered
        });
        let layout = default_layout.as_ref().unwrap_or(layout);
        let keyboard = EntLayoutKeyboard {
            name: self.current_device_name_if_known(layout),
            firmware: self.firmware,
            keyboard_id: self.current_keyboard_id,
            layout_hash: entlayout_hash(layout),
            layers: layout.layers.len(),
            keys: layout.keys.len(),
            encoders: layout.encoder_count(),
            rows: layout.rows,
            cols: layout.cols,
        };
        Some(EntLayoutFile {
            format: ENTLAYOUT_FORMAT.to_owned(),
            version: if application_settings.is_some() {
                MACROPAD_ENTLAYOUT_VERSION
            } else {
                ENTLAYOUT_VERSION
            },
            created_by: "Entropy".to_owned(),
            created_at: chrono::Local::now().to_rfc3339(),
            keyboard,
            application_categories: application_settings
                .map(|settings| {
                    let referenced: std::collections::BTreeSet<_> = settings
                        .layouts
                        .values()
                        .map(|profile| settings.category_id_for_layout(profile))
                        .collect();
                    settings
                        .category_names
                        .iter()
                        .filter(|(id, _)| referenced.contains(*id))
                        .map(|(id, name)| (id.clone(), name.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            application_layouts: application_settings.map(|settings| settings.layouts.clone()),
            pictograms: application_settings.and_then(|_| {
                let state = &self.display_settings.pictograms;
                state.loaded.then(|| {
                    [PictogramKind::Macro, PictogramKind::TapDance]
                        .into_iter()
                        .flat_map(|kind| {
                            (0..256).filter_map(move |slot| {
                                state
                                    .library
                                    .bitmap(kind, slot)
                                    .map(|bitmap| EntLayoutPictogram {
                                        tap_dance: kind == PictogramKind::TapDance,
                                        slot,
                                        bitmap: bitmap.to_vec(),
                                        color: state.library.color(kind, slot).unwrap(),
                                    })
                            })
                        })
                        .collect()
                })
            }),
            data: EntLayoutData {
                source_layout: Some(entlayout_source_layout(layout)),
                keymap: layout
                    .layers
                    .iter()
                    .map(|layer| layer.iter().map(|binding| binding.vial_keycode()).collect())
                    .collect(),
                native_keymap: layout
                    .layers
                    .iter()
                    .map(|layer| {
                        layer
                            .iter()
                            .map(|binding| binding.rmk_action().and_then(encode_native_key_action))
                            .collect()
                    })
                    .collect(),
                encoder_keymap: layout.encoder_layers.clone(),
                encoder_visibility: self.encoder_visibility.clone(),
                layout_element_visibility: Some(
                    self.current_layout_element_visibility()
                        .cloned()
                        .unwrap_or_default(),
                ),
                layout_options: self.layout_options_value,
                layer_names: default_profile
                    .map(|profile| {
                        profile
                            .layer_names
                            .iter()
                            .take(layout.layers.len())
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_else(|| self.layer_names.clone()),
                text_expander: if application_settings.is_some() {
                    EntTextExpanderData::default()
                } else {
                    self.text_expander_entlayout_snapshot()
                },
                macros: EntMacroData {
                    bytecode_base64: self
                        .keycode_picker
                        .macro_texts
                        .iter()
                        .map(|bytes| BASE64_STANDARD.encode(bytes))
                        .collect(),
                    texts: Vec::new(),
                    names: self.keycode_picker.macro_names.clone(),
                    descriptions: trailing_non_empty_strings(
                        &self.keycode_picker.macro_descriptions,
                    ),
                },
                combos: EntComboData {
                    entries: self
                        .combo_entries
                        .iter()
                        .map(|entry| EntComboEntry {
                            keys: entry.keys,
                            output: entry.output.vial_keycode(),
                            native_output: entry
                                .output
                                .rmk_action()
                                .and_then(encode_native_key_action),
                            layer: entry.layer,
                        })
                        .collect(),
                    names: self.combo_names.clone(),
                    colors: self.combo_colors.clone(),
                    term: self.combo_term,
                },
                tap_dance: EntTapDanceData {
                    entries: self
                        .keycode_picker
                        .tap_dance_entries
                        .iter()
                        .map(|entry| EntTapDanceEntry {
                            on_tap: entry.on_tap.vial_keycode(),
                            native_on_tap: entry
                                .on_tap
                                .rmk_action()
                                .and_then(encode_native_key_action),
                            on_hold: entry.on_hold.vial_keycode(),
                            native_on_hold: entry
                                .on_hold
                                .rmk_action()
                                .and_then(encode_native_key_action),
                            on_double_tap: entry.on_double_tap.vial_keycode(),
                            native_on_double_tap: entry
                                .on_double_tap
                                .rmk_action()
                                .and_then(encode_native_key_action),
                            on_tap_hold: entry.on_tap_hold.vial_keycode(),
                            native_on_tap_hold: entry
                                .on_tap_hold
                                .rmk_action()
                                .and_then(encode_native_key_action),
                            tapping_term: entry.tapping_term,
                        })
                        .collect(),
                    names: self.keycode_picker.tap_dance_names.clone(),
                },
                key_overrides: EntKeyOverrideData {
                    entries: self
                        .key_override_entries
                        .iter()
                        .map(|entry| EntKeyOverrideEntry {
                            trigger: entry.trigger,
                            replacement: entry.replacement,
                            layers: entry.layers,
                            trigger_mods: entry.trigger_mods,
                            negative_mod_mask: entry.negative_mod_mask,
                            suppressed_mods: entry.suppressed_mods,
                            options: entry.options.bits(),
                        })
                        .collect(),
                    names: self.key_override_names.clone(),
                },
                alt_repeat: EntAltRepeatData {
                    entries: self
                        .alt_repeat_entries
                        .iter()
                        .map(|entry| EntAltRepeatEntry {
                            keycode: entry.keycode,
                            alt_keycode: entry.alt_keycode,
                            allowed_mods: entry.allowed_mods,
                            options: entry.options.bits(),
                        })
                        .collect(),
                    names: self.alt_repeat_names.clone(),
                },
                portable_settings: self.entlayout_portable_settings_snapshot(),
            },
        })
    }

    fn text_expander_entlayout_snapshot(&self) -> EntTextExpanderData {
        let rule_files =
            normalize_text_expander_rule_files(&self.app_settings.text_expander_rule_files);
        let extra_files = rule_files
            .iter()
            .filter_map(|file_name| {
                load_text_expansion_rules_from_path(&text_expander_extra_rules_path(file_name)).map(
                    |rules| EntTextExpanderRuleFile {
                        file_name: file_name.clone(),
                        rules,
                    },
                )
            })
            .collect();
        EntTextExpanderData {
            enabled: self.app_settings.text_expander_enabled,
            app_blacklist: self.app_settings.text_expander_app_blacklist.clone(),
            rule_files,
            primary_rules: self.app_settings.text_expansion_rules.clone(),
            extra_files,
        }
    }

    fn current_device_name_if_known(&self, layout: &KeyboardLayout) -> String {
        if self.current_device_name.is_empty() {
            layout.name.clone()
        } else {
            self.current_device_name.clone()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn write_entlayout_auto_backup(&self) -> Result<PathBuf> {
        let bundle = self.entlayout_snapshot().context(crate::i18n::tr_catalog(
            self.app_settings.language,
            "entlayout.cannot_create_backup_without_keyboard",
        ))?;
        let dir = storage_config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("entropy")
            .join("backups");
        std::fs::create_dir_all(&dir).context(crate::i18n::tr_catalog(
            self.app_settings.language,
            "entlayout.failed_to_create_backup_directory",
        ))?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let path = dir.join(format!(
            "auto-backup-{}-{stamp}.entlayout",
            device_id_slug(&bundle.keyboard.name)
        ));
        write_entlayout_file(&path, &bundle, self.app_settings.language)?;
        Ok(path)
    }

    fn validate_entlayout_file(&self, bundle: &EntLayoutFile) -> Result<()> {
        self.validate_entlayout_file_with_capability(bundle, true)
    }

    fn validate_entlayout_file_with_capability(
        &self,
        bundle: &EntLayoutFile,
        require_confirmed: bool,
    ) -> Result<()> {
        if bundle.format != ENTLAYOUT_FORMAT {
            bail!(
                "{}",
                crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "entlayout.unsupported_format",
                    &[("format", &bundle.format)]
                )
            );
        }
        if !entlayout_version_supported(bundle.version)
            && bundle.version != MACROPAD_ENTLAYOUT_VERSION
        {
            bail!(
                "{}",
                crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "entlayout.unsupported_version",
                    &[("version", &bundle.version.to_string())]
                )
            );
        }
        let _layout = self.layout.as_ref().context(crate::i18n::tr_catalog(
            self.app_settings.language,
            "entlayout.connect_target_keyboard",
        ))?;
        if let Some(pictograms) = &bundle.pictograms {
            if bundle.version != MACROPAD_ENTLAYOUT_VERSION
                || (require_confirmed && self.display_settings.pictograms.supported != Some(true))
            {
                bail!("Pictogram import requires a confirmed supported target library");
            }
            let mut seen = std::collections::BTreeSet::new();
            for icon in pictograms {
                if icon.slot >= 256
                    || icon.bitmap.len() != PICTOGRAM_BYTES
                    || !seen.insert((icon.tap_dance, icon.slot))
                {
                    bail!("Invalid or duplicate imported pictogram");
                }
            }
        }
        if (bundle.version == MACROPAD_ENTLAYOUT_VERSION) != bundle.application_layouts.is_some() {
            bail!("Invalid complete Macropad layout version/payload");
        }
        if let Some(profiles) = &bundle.application_layouts {
            let mut category_ids = std::collections::BTreeSet::new();
            let mut category_names = std::collections::BTreeSet::new();
            for (id, name) in &bundle.application_categories {
                if id.trim().is_empty()
                    || name.trim().is_empty()
                    || name.trim().chars().count() > 48
                    || !category_ids.insert(id.as_str())
                    || !category_names.insert(name.trim().to_lowercase())
                {
                    bail!("Invalid or duplicate application category");
                }
            }
            for profile in profiles.values() {
                if let Some(id) = profile.custom_category_id.as_deref() {
                    if id.starts_with("custom:") && !category_ids.contains(id) {
                        bail!("Missing application category definition: {id}");
                    }
                }
            }
            let geometry = bundle
                .data
                .source_layout
                .as_ref()
                .context("Complete Macropad layout requires source geometry")?;
            if bundle.keyboard.keys != geometry.keys.len()
                || bundle.keyboard.layers != bundle.data.keymap.len()
                || geometry.encoders.len() != bundle.data.encoder_keymap.first().map_or(0, Vec::len)
                || bundle.data.layer_names.len() != bundle.keyboard.layers
            {
                bail!("Inconsistent complete Macropad geometry");
            }
            if geometry.keys.iter().any(|key| {
                key.row as usize >= bundle.keyboard.rows
                    || key.col as usize >= bundle.keyboard.cols
                    || !key.x.is_finite()
                    || !key.y.is_finite()
                    || !key.w.is_finite()
                    || !key.h.is_finite()
                    || key.w <= 0.0
                    || key.h <= 0.0
            }) || geometry.encoders.iter().any(|encoder| {
                encoder.direction > 1
                    || !encoder.x.is_finite()
                    || !encoder.y.is_finite()
                    || encoder.w <= 0.0
                    || encoder.h <= 0.0
            }) {
                bail!("Invalid complete Macropad source geometry");
            }
            if require_confirmed {
                self.application_layout_device_key()
                    .context("Complete layout requires a Macropad target")?;
            }
            if !profiles.contains_key(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID) {
                bail!("Complete Macropad layout is missing Default");
            }
            let macro_bytes = entmacro_bytecode(&bundle.data.macros)?;
            if macro_bytes.len() > self.keycode_picker.macro_count {
                bail!("Target has insufficient macro slots");
            }

            let target_limits = self.entlayout_keycode_import_limits();
            let shared_limits = EntLayoutKeycodeImportLimits {
                layer_count: target_limits.layer_count.min(bundle.keyboard.layers),
                macro_count: target_limits
                    .macro_count
                    .min(entmacro_count(&bundle.data.macros)),
                tap_dance_count: target_limits
                    .tap_dance_count
                    .min(bundle.data.tap_dance.entries.len()),
            };
            if bundle.data.tap_dance.entries.len() > shared_limits.tap_dance_count
                || bundle.data.combos.entries.len() > self.combo_entries.len()
                || bundle.data.key_overrides.entries.len() > self.key_override_entries.len()
                || bundle.data.alt_repeat.entries.len() > self.alt_repeat_entries.len()
            {
                bail!("Target has insufficient shared resource slots");
            }
            for code in bundle
                .data
                .keymap
                .iter()
                .flatten()
                .chain(bundle.data.encoder_keymap.iter().flatten())
            {
                check_entlayout_keycode(*code, bundle, _layout, shared_limits)
                    .map_err(|_| anyhow::anyhow!("Unrestorable firmware resource: {code:#06x}"))?;
            }
            for encoded in bundle
                .data
                .native_keymap
                .iter()
                .flatten()
                .flatten()
                .chain(
                    bundle
                        .data
                        .combos
                        .entries
                        .iter()
                        .filter_map(|entry| entry.native_output.as_ref()),
                )
                .chain(
                    bundle
                        .data
                        .tap_dance
                        .entries
                        .iter()
                        .flat_map(|entry| {
                            [
                                entry.native_on_tap.as_ref(),
                                entry.native_on_hold.as_ref(),
                                entry.native_on_double_tap.as_ref(),
                                entry.native_on_tap_hold.as_ref(),
                            ]
                        })
                        .flatten(),
                )
            {
                check_complete_native_action(encoded, bundle, _layout, shared_limits)?;
            }
            for entry in &bundle.data.tap_dance.entries {
                check_enttap_dance_entry(entry, bundle, _layout, shared_limits)
                    .map_err(|_| anyhow::anyhow!("Unrestorable shared Tap Dance dependency"))?;
            }
            for entry in &bundle.data.combos.entries {
                check_entcombo_entry(entry, bundle, _layout, shared_limits)
                    .map_err(|_| anyhow::anyhow!("Unrestorable shared Combo dependency"))?;
            }
            for entry in &bundle.data.key_overrides.entries {
                check_entkey_override_entry(entry, bundle, _layout, shared_limits)
                    .map_err(|_| anyhow::anyhow!("Unrestorable shared key override dependency"))?;
            }
            for entry in &bundle.data.alt_repeat.entries {
                check_entalt_repeat_entry(entry, bundle, _layout, shared_limits)
                    .map_err(|_| anyhow::anyhow!("Unrestorable shared Alt Repeat dependency"))?;
            }
            let limits = EntLayoutKeycodeImportLimits {
                layer_count: 16,
                macro_count: self
                    .keycode_picker
                    .macro_count
                    .min(entmacro_count(&bundle.data.macros)),
                tap_dance_count: self
                    .keycode_picker
                    .tap_dance_entries
                    .len()
                    .min(bundle.data.tap_dance.entries.len()),
            };
            let mut merged = self
                .application_layout_settings()
                .map(|settings| settings.layouts.clone())
                .unwrap_or_default();
            merged.extend(profiles.clone());
            let candidates: Vec<_> = merged
                .iter()
                .filter(|(id, _)| {
                    id.as_str() != crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID
                })
                .collect();
            for (index, (id, profile)) in candidates.iter().enumerate() {
                for (other_id, other) in &candidates[index + 1..] {
                    // Do not reject unrelated, pre-existing configurations.
                    if !profiles.contains_key(*id) && !profiles.contains_key(*other_id) {
                        continue;
                    }
                    if profile.title_contains.trim().to_lowercase()
                        == other.title_contains.trim().to_lowercase()
                        && crate::application_layouts::application_identities_match(
                            std::iter::once(profile.executable.as_str())
                                .chain(profile.application_identities.iter().map(String::as_str)),
                            std::iter::once(other.executable.as_str())
                                .chain(other.application_identities.iter().map(String::as_str)),
                        )
                    {
                        bail!("Ambiguous application identity for {id} and {other_id}");
                    }
                }
            }
            for (id, profile) in profiles {
                if id.is_empty()
                    || id != &profile.id
                    || profile.layers.len() != 16
                    || profile.layer_names.len() != 16
                    || profile.encoder_stacks.len() != 16
                    || profile
                        .encoder_stacks
                        .iter()
                        .any(|stack| !matches!(stack.len(), 0 | 2..=4))
                {
                    bail!("Invalid application profile dimensions or identity: {id}");
                }
                if let Some(existing) = self
                    .application_layout_settings()
                    .and_then(|settings| settings.layouts.get(id))
                {
                    if id != crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID
                        && existing.executable != profile.executable
                    {
                        bail!(
                            "Ambiguous application identity for {id}; existing executable differs"
                        );
                    }
                }
                for code in profile.layers.iter().flatten().copied().chain(
                    profile
                        .encoder_stacks
                        .iter()
                        .flatten()
                        .flat_map(|action| [action.counter_clockwise, action.clockwise]),
                ) {
                    if code != crate::application_layouts::APPLICATION_LAYOUT_UNSET_KEYCODE {
                        check_entlayout_keycode(code, bundle, _layout, limits).map_err(|_| {
                            anyhow::anyhow!("Unmappable application resource in {id}: {code:#06x}")
                        })?;
                    }
                }
            }
        }
        if bundle.data.keymap.is_empty() {
            bail!(
                "{}",
                crate::i18n::tr_catalog(self.app_settings.language, "entlayout.keymap_empty",)
            );
        }
        let source_key_count = bundle_source_key_count(bundle);
        if source_key_count == 0 {
            bail!(
                "{}",
                crate::i18n::tr_catalog(self.app_settings.language, "entlayout.no_source_keys",)
            );
        }
        if bundle
            .data
            .keymap
            .iter()
            .any(|layer| layer.len() != source_key_count)
        {
            bail!(
                "{}",
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "entlayout.inconsistent_key_counts",
                )
            );
        }
        if !bundle.data.native_keymap.is_empty()
            && (bundle.data.native_keymap.len() != bundle.data.keymap.len()
                || bundle
                    .data
                    .native_keymap
                    .iter()
                    .any(|layer| layer.len() != source_key_count))
        {
            bail!("inconsistent native RMK key-action counts");
        }
        for layer in &bundle.data.native_keymap {
            for encoded in layer.iter().flatten() {
                decode_native_key_action(encoded)?;
            }
        }
        for combo in &bundle.data.combos.entries {
            if let Some(encoded) = combo.native_output.as_deref() {
                decode_native_key_action(encoded)?;
            }
            if combo
                .layer
                .is_some_and(|layer| layer as usize >= bundle.keyboard.layers)
            {
                bail!("Combo layer is outside the source layout");
            }
        }
        for tap_dance in &bundle.data.tap_dance.entries {
            for encoded in [
                tap_dance.native_on_tap.as_deref(),
                tap_dance.native_on_hold.as_deref(),
                tap_dance.native_on_double_tap.as_deref(),
                tap_dance.native_on_tap_hold.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                decode_native_key_action(encoded)?;
            }
        }
        let source_encoder_count = bundle_source_encoder_count(bundle);
        if bundle
            .data
            .encoder_keymap
            .iter()
            .any(|layer| layer.len() != source_encoder_count)
        {
            bail!(
                "{}",
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "entlayout.inconsistent_encoder_counts",
                )
            );
        }
        // The first pass is pure: reject all malformed source structures and
        // references before capability preparation or any other HID request.
        // Query target capacity only in the strict pass, after the read-only
        // capability retry has had a chance to confirm the matching Macropad.
        if require_confirmed && bundle.application_layouts.is_some() {
            let macro_bytes = entmacro_bytecode(&bundle.data.macros)?;
            if !macro_bytes.is_empty() {
                let size = self
                    .hid_device
                    .as_ref()
                    .context("No active keyboard connection")?
                    .get_macro_buffer_size()? as usize;
                // Empty target slots also require their NUL separator.
                let required = macro_bytes
                    .iter()
                    .map(Vec::len)
                    .sum::<usize>()
                    .checked_add(self.keycode_picker.macro_count)
                    .context("Macro buffer size overflow")?;
                if required > size {
                    bail!("Target has insufficient macro bytecode capacity");
                }
            }
        }
        Ok(())
    }

    fn entlayout_import_mapping(&self, bundle: &EntLayoutFile) -> Result<EntLayoutImportMapping> {
        let layout = self.layout.as_ref().context(crate::i18n::tr_catalog(
            self.app_settings.language,
            "entlayout.connect_target_keyboard",
        ))?;
        let matching_geometry = (bundle.keyboard.layout_hash == entlayout_hash(layout)
            || entlayout_matches_offset_alternatives(bundle, layout))
            && bundle_source_key_count(bundle) == layout.keys.len()
            && bundle_source_encoder_count(bundle) == layout.encoders.len();
        let exact_layout = matching_geometry && bundle.keyboard.layers == layout.layers.len();

        if matching_geometry {
            return Ok(EntLayoutImportMapping {
                exact_layout,
                key_mapping: (0..bundle_source_key_count(bundle)).map(Some).collect(),
                encoder_mapping: (0..bundle_source_encoder_count(bundle)).map(Some).collect(),
            });
        }

        Ok(EntLayoutImportMapping {
            exact_layout: false,
            key_mapping: universal_key_mapping(bundle, layout),
            encoder_mapping: universal_encoder_mapping(bundle, layout),
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn apply_entlayout(
        &mut self,
        bundle: &EntLayoutFile,
    ) -> Result<(
        EntFirmwareFailures,
        EntLayoutImportMapping,
        EntPortableSettingsImportReport,
    )> {
        self.hid_device
            .as_ref()
            .context("No active keyboard connection")?;
        let mapping = self.entlayout_import_mapping(bundle)?;
        let mut sidecars = EntLayoutSidecars::default();
        if bundle.application_layouts.is_some() && !self.current_device_is_likely_rmk() {
            let name = &self.current_device_name;
            sidecars.stage(
                macro_metadata_path(name),
                &MacroMetadata {
                    names: normalized_strings(
                        &bundle.data.macros.names,
                        self.keycode_picker.macro_count,
                    ),
                    descriptions: normalized_strings(
                        &bundle.data.macros.descriptions,
                        self.keycode_picker.macro_count,
                    ),
                },
            )?;
            sidecars.stage(
                combo_names_path(name),
                &normalized_strings(&bundle.data.combos.names, self.combo_entries.len()),
            )?;
            let mut colors = bundle.data.combos.colors.clone();
            migrate_legacy_combo_default_colors(&mut colors);
            normalize_combo_colors(&mut colors, self.combo_entries.len());
            sidecars.stage(combo_colors_path(name), &colors)?;
            sidecars.stage(
                tap_dance_names_path(name),
                &normalized_strings(
                    &bundle.data.tap_dance.names,
                    self.keycode_picker.tap_dance_entries.len(),
                ),
            )?;
            sidecars.stage(
                key_override_names_path(name),
                &normalized_strings(
                    &bundle.data.key_overrides.names,
                    self.key_override_entries.len(),
                ),
            )?;
            sidecars.stage(
                alt_repeat_names_path(name),
                &normalized_strings(&bundle.data.alt_repeat.names, self.alt_repeat_entries.len()),
            )?;
        }
        let mut staged_settings = None;
        let mut staged_pictograms = None;
        if let Some(profiles) = &bundle.application_layouts {
            let key = self
                .application_layout_device_key()
                .context("Select a Macropad")?;
            let mut settings = self.app_settings.clone();
            let target = settings.application_layouts.entry(key).or_default();
            let mut category_mapping = std::collections::BTreeMap::new();
            for (id, name) in &bundle.application_categories {
                let mapped = target
                    .category_names
                    .iter()
                    .find(|(existing_id, existing_name)| {
                        target.category_exists(existing_id)
                            && existing_name.eq_ignore_ascii_case(name.trim())
                    })
                    .map(|(id, _)| id.clone());
                let mapped = match mapped {
                    Some(id) => id,
                    None => target
                        .create_category(name)
                        .context("Cannot allocate imported application category")?,
                };
                category_mapping.insert(id.clone(), mapped);
            }
            for (id, source) in profiles {
                let mut profile = source.clone();
                let source_category = source.custom_category_id.clone().unwrap_or_else(|| {
                    source
                        .category
                        .unwrap_or_else(|| {
                            crate::application_layouts::application_layout_category_for_executable(
                                &source.executable,
                            )
                        })
                        .id()
                        .to_owned()
                });
                if let Some(mapped) = category_mapping.get(&source_category) {
                    profile.custom_category_id = Some(mapped.clone());
                }
                let layout = self.layout.as_ref().context("Missing target layout")?;
                let limits = EntLayoutKeycodeImportLimits {
                    layer_count: 16,
                    ..self.entlayout_keycode_import_limits()
                };
                for code in profile.layers.iter_mut().flatten().chain(
                    profile
                        .encoder_stacks
                        .iter_mut()
                        .flatten()
                        .flat_map(|action| [&mut action.counter_clockwise, &mut action.clockwise]),
                ) {
                    if *code != crate::application_layouts::APPLICATION_LAYOUT_UNSET_KEYCODE {
                        *code = check_entlayout_keycode(*code, bundle, layout, limits).map_err(
                            |_| anyhow::anyhow!("Unmappable application resource in {id}"),
                        )?;
                    }
                }
                target.layouts.insert(id.clone(), profile);
            }
            if let (Some(source), Some(layout), Some(device_key)) = (
                bundle.data.layout_element_visibility.as_ref(),
                self.layout.as_ref(),
                self.layout_element_visibility_device_key(),
            ) {
                let current = settings
                    .layout_element_visibility
                    .get(&device_key)
                    .cloned()
                    .unwrap_or_default();
                let mapped = map_entlayout_element_visibility(
                    source,
                    bundle.data.source_layout.as_ref(),
                    &mapping,
                    layout,
                    &current,
                );
                if mapped == LayoutElementVisibility::default() {
                    settings.layout_element_visibility.remove(&device_key);
                } else {
                    settings
                        .layout_element_visibility
                        .insert(device_key, mapped);
                }
            }
            let destination = app_settings_path();
            let parent = destination.parent().context("Invalid settings path")?;
            std::fs::create_dir_all(parent)?;
            let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
            use std::io::Write;
            temporary.write_all(&serde_json::to_vec_pretty(&settings)?)?;
            temporary.as_file().sync_all()?;
            if destination.is_dir() {
                bail!("Failed to save Macropad layouts: destination is a directory");
            }
            staged_settings = Some((temporary, destination, settings));
        }
        sidecars.persist()?;
        if let Some(pictograms) = &bundle.pictograms {
            let hid = self
                .hid_device
                .as_ref()
                .context("No active keyboard connection")?;
            let progress = std::sync::atomic::AtomicU32::new(0);
            let mut library = if self.display_settings.pictograms.loaded {
                self.display_settings.pictograms.library.clone()
            } else {
                hid.load_pictograms(&progress)?
            };
            for icon in pictograms {
                let kind = if icon.tap_dance {
                    PictogramKind::TapDance
                } else {
                    PictogramKind::Macro
                };
                if library.bitmap(kind, icon.slot) == Some(icon.bitmap.as_slice())
                    && library.color(kind, icon.slot) == Some(icon.color)
                {
                    continue;
                }
                // A failed/partial upload can already have changed firmware.
                // Keep the old bytes for display, but never claim they are still
                // confirmed; only publish readback after all import gates pass.
                self.display_settings.pictograms.loaded = false;
                library.set_colored(kind, icon.slot, &icon.bitmap, icon.color);
                library = hid.upload_pictogram_slot(library, kind, icon.slot, &progress)?;
            }
            if !pictograms.is_empty() {
                let confirmed = hid.load_pictograms(&progress)?;
                for icon in pictograms {
                    let kind = if icon.tap_dance {
                        PictogramKind::TapDance
                    } else {
                        PictogramKind::Macro
                    };
                    if confirmed.bitmap(kind, icon.slot) != Some(icon.bitmap.as_slice())
                        || confirmed.color(kind, icon.slot) != Some(icon.color)
                    {
                        bail!("Imported pictogram readback mismatch");
                    }
                }
                staged_pictograms = Some(confirmed);
            }
        }
        let (mut firmware_failures, portable_settings) =
            self.apply_entlayout_firmware_state(bundle, &mapping)?;
        // Complete backups must finish every supported HID operation before host
        // settings publication or sidecar commit. Firmware itself is not atomic.
        #[cfg(not(target_arch = "wasm32"))]
        if bundle.application_layouts.is_some() {
            let mut names = self.layer_names.clone();
            for (target, source) in names
                .iter_mut()
                .take(self.layer_count)
                .zip(&bundle.data.layer_names)
            {
                *target = source.clone();
            }
            names.truncate(self.layer_count);
            let failed = self.write_layer_names_to_firmware(&names);
            if !failed.is_empty() {
                firmware_failures.push(format!("layer names (layers {failed:?})"));
            }
        }
        if bundle.application_layouts.is_some() && !firmware_failures.is_empty() {
            bail!(
                "Macropad resource import failed; host profiles were not published: {}",
                firmware_failures.lines.join(", ")
            );
        }
        if let Some((temporary, destination, settings)) = staged_settings {
            temporary
                .persist(&destination)
                .context("Failed to save Macropad layouts")?;
            self.app_settings = settings;
        }
        if let Some(library) = staged_pictograms {
            self.display_settings.pictograms.library = library;
            self.display_settings.pictograms.loaded = true;
        }
        sidecars.complete = true;
        self.apply_entlayout_local_state(bundle, &mapping)?;
        if bundle
            .data
            .portable_settings
            .iter()
            .any(|setting| setting.module.is_some())
        {
            // Imported display options cannot override the committed firmware
            // module word (failed/skipped writes keep the target cache).
            self.sync_firmware_managed_layout_options();
        }
        self.refresh_layer_picker_content_flags();

        // Persist the imported layer names into firmware now that self.layer_names
        // holds the final target values. The shared writer continues past a
        // failing layer and reports which ones failed, so stale firmware slots
        // aren't left to win over the local names on the next reconnect.
        #[cfg(not(target_arch = "wasm32"))]
        if bundle.application_layouts.is_none() {
            let names = self.layer_names.clone();
            let failed = self.write_layer_names_to_firmware(&names);
            if !failed.is_empty() {
                let lang = self.app_settings.language;
                let layers = failed
                    .iter()
                    .map(|layer| layer.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                firmware_failures.push(crate::i18n::tr_catalog_format(
                    lang,
                    "entlayout.firmware_failure",
                    &[
                        (
                            "section",
                            crate::i18n::tr_catalog(lang, "entlayout.layer_names"),
                        ),
                        ("error", &format!("layers {layers}")),
                    ],
                ));
            }
        }

        Ok((firmware_failures, mapping, portable_settings))
    }

    fn entlayout_keycode_import_limits(&self) -> EntLayoutKeycodeImportLimits {
        EntLayoutKeycodeImportLimits {
            layer_count: self.layer_count,
            macro_count: self.keycode_picker.macro_count,
            tap_dance_count: self.keycode_picker.tap_dance_entries.len(),
        }
    }

    pub(crate) fn current_device_is_likely_rmk(&self) -> bool {
        let selected_device = self
            .selected_device
            .and_then(|idx| self.device_manager.devices().get(idx));
        let mut parts = vec![self.current_device_name.as_str()];
        if let Some(device) = selected_device {
            parts.push(device.name.as_str());
            parts.push(device.manufacturer.as_str());
        }
        parts
            .iter()
            .any(|part| part.to_ascii_lowercase().contains("rmk"))
    }

    fn entlayout_dynamic_import_report(
        &self,
        bundle: &EntLayoutFile,
        failed: &[&'static str],
    ) -> String {
        if self.current_device_is_likely_rmk() {
            return "firmware-backed dynamic sections skipped for RMK safety mode".to_owned();
        }
        let Some(layout) = self.layout.as_ref() else {
            return "none".to_owned();
        };
        let limits = self.entlayout_keycode_import_limits();
        let mut macro_stats = EntLayoutSectionImportStats::default();
        for idx in 0..entmacro_count(&bundle.data.macros) {
            if idx < self.keycode_picker.macro_count {
                macro_stats.record_imported();
            } else {
                macro_stats.record_skipped(EntLayoutSkipReason::UnsupportedSlot);
            }
        }

        let mut combo_stats = EntLayoutSectionImportStats::default();
        for (idx, entry) in bundle.data.combos.entries.iter().enumerate() {
            if idx >= self.combo_entries.len() {
                combo_stats.record_skipped(EntLayoutSkipReason::UnsupportedSlot);
                continue;
            }
            match check_entcombo_entry(entry, bundle, layout, limits) {
                Ok(()) => combo_stats.record_imported(),
                Err(reason) => combo_stats.record_skipped(reason),
            }
        }

        let mut tap_dance_stats = EntLayoutSectionImportStats::default();
        for (idx, entry) in bundle.data.tap_dance.entries.iter().enumerate() {
            if idx >= self.keycode_picker.tap_dance_entries.len() {
                tap_dance_stats.record_skipped(EntLayoutSkipReason::UnsupportedSlot);
                continue;
            }
            match check_enttap_dance_entry(entry, bundle, layout, limits) {
                Ok(()) => tap_dance_stats.record_imported(),
                Err(reason) => tap_dance_stats.record_skipped(reason),
            }
        }

        let mut key_override_stats = EntLayoutSectionImportStats::default();
        for (idx, entry) in bundle.data.key_overrides.entries.iter().enumerate() {
            if idx >= self.key_override_entries.len() {
                key_override_stats.record_skipped(EntLayoutSkipReason::UnsupportedSlot);
                continue;
            }
            match check_entkey_override_entry(entry, bundle, layout, limits) {
                Ok(()) => key_override_stats.record_imported(),
                Err(reason) => key_override_stats.record_skipped(reason),
            }
        }

        let mut alt_repeat_stats = EntLayoutSectionImportStats::default();
        for (idx, entry) in bundle.data.alt_repeat.entries.iter().enumerate() {
            if idx >= self.alt_repeat_entries.len() {
                alt_repeat_stats.record_skipped(EntLayoutSkipReason::UnsupportedSlot);
                continue;
            }
            match check_entalt_repeat_entry(entry, bundle, layout, limits) {
                Ok(()) => alt_repeat_stats.record_imported(),
                Err(reason) => alt_repeat_stats.record_skipped(reason),
            }
        }

        let lang = self.app_settings.language;
        // A section whose write failed counts nothing as imported.
        let summary = |stats: EntLayoutSectionImportStats, label_key| {
            if failed.contains(&label_key) {
                crate::i18n::tr_catalog_format(
                    lang,
                    "entlayout.summary_failed",
                    &[("section", crate::i18n::tr_catalog(lang, label_key))],
                )
            } else {
                stats.summary(lang, label_key)
            }
        };
        [
            summary(macro_stats, "entlayout.macros"),
            summary(combo_stats, "entlayout.combos"),
            summary(tap_dance_stats, "entlayout.tap_dance"),
            summary(key_override_stats, "entlayout.key_overrides"),
            summary(alt_repeat_stats, "entlayout.alt_repeat"),
        ]
        .join("; ")
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn entlayout_import_report(
        &self,
        bundle: &EntLayoutFile,
        backup_path: &Path,
        firmware_failures: &EntFirmwareFailures,
        mapping: &EntLayoutImportMapping,
        portable_settings: &EntPortableSettingsImportReport,
    ) -> String {
        let lang = self.app_settings.language;
        let tr = |key| crate::i18n::tr_catalog(lang, key).to_owned();
        let mut imported = vec![
            tr("entlayout.keymap"),
            tr("entlayout.encoder_keymap"),
            tr("entlayout.encoder_visibility"),
            tr("entlayout.layer_names"),
        ];
        if let Some(profiles) = &bundle.application_layouts {
            imported.push(match lang {
                crate::i18n::Language::Russian => format!("Раскладки приложений: {} (Default и все приложения; правила сопоставления восстановлены)", profiles.len()),
                crate::i18n::Language::English => format!("Application layouts: {} (Default and all applications; app matching restored)", profiles.len()),
            });
            if bundle
                .pictograms
                .as_ref()
                .is_some_and(|icons| !icons.is_empty())
            {
                imported.push(match lang {
                    crate::i18n::Language::Russian => "Пиктограммы".into(),
                    crate::i18n::Language::English => "Pictograms".into(),
                });
            }
        } else {
            imported.push(tr("entlayout.text_expander"));
        }
        let mut skipped = Vec::new();
        if bundle.data.layout_element_visibility.is_some() {
            let has_target = self.layout_element_visibility_device_key().is_some();
            let has_source = mapping.exact_layout || bundle.data.source_layout.is_some();
            let has_mapped_positions = mapping.exact_layout
                || mapping.key_mapping.iter().any(Option::is_some)
                || mapping.encoder_mapping.iter().any(Option::is_some);
            if has_target && has_source && has_mapped_positions {
                imported.push(tr("entlayout.layout_element_visibility"));
                if !mapping.exact_layout
                    && (mapping.key_mapping.iter().any(Option::is_none)
                        || mapping.encoder_mapping.iter().any(Option::is_none))
                {
                    skipped.push(tr("entlayout.layout_element_visibility_unmapped"));
                }
            } else {
                skipped.push(tr("entlayout.layout_element_visibility"));
            }
        }

        if mapping.exact_layout {
            if bundle.data.layout_options.is_some() && self.layout_options_value.is_some() {
                imported.push(tr("entlayout.layout_options"));
            } else if bundle.data.layout_options.is_some() {
                skipped.push(tr("entlayout.layout_options_unsupported"));
            }
        } else if bundle.data.layout_options.is_some() {
            skipped.push(tr("entlayout.layout_options_exact_only"));
        }

        if self.current_device_is_likely_rmk() {
            skipped.push(tr("entlayout.firmware_dynamic_sections_rmk"));
        } else {
            // A section the file leaves empty is neither imported nor skipped:
            // there is nothing in it to lose.
            let sections = [
                (
                    "entlayout.macros",
                    entmacro_count(&bundle.data.macros) > 0,
                    self.keycode_picker.macro_count > 0,
                ),
                (
                    "entlayout.combos",
                    !bundle.data.combos.entries.is_empty(),
                    !self.combo_entries.is_empty(),
                ),
                (
                    "entlayout.tap_dance",
                    !bundle.data.tap_dance.entries.is_empty(),
                    !self.keycode_picker.tap_dance_entries.is_empty(),
                ),
                (
                    "entlayout.key_overrides",
                    !bundle.data.key_overrides.entries.is_empty(),
                    !self.key_override_entries.is_empty(),
                ),
                (
                    "entlayout.alt_repeat",
                    !bundle.data.alt_repeat.entries.is_empty(),
                    !self.alt_repeat_entries.is_empty(),
                ),
            ];
            for (section, in_file, on_board) in sections {
                if !in_file {
                    continue;
                }
                if on_board {
                    imported.push(tr(section));
                } else {
                    skipped.push(crate::i18n::tr_catalog_format(
                        lang,
                        "entlayout.not_available",
                        &[("section", crate::i18n::tr_catalog(lang, section))],
                    ));
                }
            }
        }

        let none = crate::i18n::tr_catalog(lang, "entlayout.none");
        let skipped = if skipped.is_empty() {
            none.to_owned()
        } else {
            skipped.join(", ")
        };
        let dynamic_report =
            self.entlayout_dynamic_import_report(bundle, &firmware_failures.sections);
        // Whatever else landed, a write that failed is no complete import.
        let complete = crate::i18n::tr_catalog(
            lang,
            if firmware_failures.is_empty() {
                "entlayout.import_complete"
            } else {
                "entlayout.import_complete_with_errors"
            },
        );
        let firmware_failures = if firmware_failures.is_empty() {
            none.to_owned()
        } else {
            firmware_failures.lines.join(", ")
        };
        let mode = if mapping.exact_layout {
            crate::i18n::tr_catalog(lang, "entlayout.exact")
        } else {
            crate::i18n::tr_catalog(lang, "entlayout.universal")
        };
        let firmware_failures = if firmware_failures == none {
            none.to_owned()
        } else {
            firmware_failures
                .split(", ")
                .map(|failure| format!("• {failure}"))
                .collect::<Vec<_>>()
                .join(
                    "
",
                )
        };
        let skipped = if skipped == none {
            none.to_owned()
        } else {
            skipped
                .split(", ")
                .map(|entry| format!("• {entry}"))
                .collect::<Vec<_>>()
                .join(
                    "
",
                )
        };
        let imported = imported
            .iter()
            .map(|section| format!("• {section}"))
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        let dynamic_report = dynamic_report
            .split("; ")
            .map(|section| format!("• {section}"))
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        let portable_settings = portable_settings
            .summary(lang)
            .split("; ")
            .map(|section| format!("• {section}"))
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        format!(
            "{complete}

{mode_label}: {mode}

{mapped_layout}:
• {keys}: {mapped_keys}/{total_keys}
• {encoder_slots}: {mapped_encoders}/{total_encoders}

{imported_sections}:
{imported}

{dynamic_sections}:
{dynamic_report}

{portable_settings_label}:
{portable_settings}

{skipped_label}:
{skipped}

{firmware_issues}:
{firmware_failures}

{auto_backup}:
{backup_path}",
            complete = complete,
            mode_label = crate::i18n::tr_catalog(lang, "entlayout.mode"),
            mapped_layout = crate::i18n::tr_catalog(lang, "entlayout.mapped_layout"),
            keys = crate::i18n::tr_catalog(lang, "entlayout.keys"),
            encoder_slots = crate::i18n::tr_catalog(lang, "entlayout.encoder_slots"),
            imported_sections = crate::i18n::tr_catalog(lang, "entlayout.imported_sections"),
            dynamic_sections = crate::i18n::tr_catalog(lang, "entlayout.dynamic_sections"),
            portable_settings_label = crate::i18n::tr_catalog(lang, "entlayout.portable_settings"),
            skipped_label = crate::i18n::tr_catalog(lang, "entlayout.skipped"),
            firmware_issues = crate::i18n::tr_catalog(lang, "entlayout.firmware_issues"),
            auto_backup = crate::i18n::tr_catalog(lang, "entlayout.auto_backup"),
            mapped_keys = mapping.mapped_keys(),
            total_keys = mapping.key_mapping.len(),
            mapped_encoders = mapping.mapped_encoders(),
            total_encoders = mapping.encoder_mapping.len(),
            imported = imported,
            dynamic_report = dynamic_report,
            portable_settings = portable_settings,
            skipped = skipped,
            firmware_failures = firmware_failures,
            backup_path = backup_path.display()
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn apply_entlayout_firmware_state(
        &mut self,
        bundle: &EntLayoutFile,
        mapping: &EntLayoutImportMapping,
    ) -> Result<(EntFirmwareFailures, EntPortableSettingsImportReport)> {
        let lang = self.app_settings.language;
        let Some(hid) = &self.hid_device else {
            bail!(
                "{}",
                crate::i18n::tr_catalog(lang, "entlayout.no_active_keyboard_connection")
            );
        };
        let layout = self
            .layout
            .as_ref()
            .context(crate::i18n::tr_catalog(
                lang,
                "entlayout.no_connected_layout",
            ))?
            .clone();
        let limits = self.entlayout_keycode_import_limits();
        let supports_rmk_native_key_actions = self.keycode_picker.supports_rmk_native_key_actions;
        let supports_rmk_native_combo_output = self.keycode_picker.supports_rmk_native_combo_output;
        let supports_rmk_native_tap_dance_actions =
            self.keycode_picker.supports_rmk_native_tap_dance_actions;
        let supports_rmk_combo_layers = self.supports_rmk_combo_layers;
        let mut failures = EntFirmwareFailures::default();
        let target_portable_settings = self.entlayout_portable_settings_snapshot();
        let EntPortableSettingsImportPlan {
            compatible: portable_settings,
            report: mut portable_settings_report,
        } = plan_entlayout_portable_settings_import(
            &bundle.data.portable_settings,
            &target_portable_settings,
        );
        let mut verified_portable_settings = Vec::new();
        let mut failed_layer_colors = std::collections::BTreeSet::new();

        if let Err(err) = (|| -> Result<()> {
            for (source_layer_idx, layer_codes) in bundle.data.keymap.iter().enumerate() {
                let Some(target_layer_idx) = map_layer_index(source_layer_idx, self.layer_count)
                else {
                    continue;
                };
                for (source_key_idx, keycode) in layer_codes.iter().copied().enumerate() {
                    let Some(target_key_idx) =
                        mapping.key_mapping.get(source_key_idx).copied().flatten()
                    else {
                        continue;
                    };
                    let binding = match entlayout_native_key_action(
                        bundle,
                        source_layer_idx,
                        source_key_idx,
                    )? {
                        Some(action) => {
                            if !supports_rmk_native_key_actions {
                                bail!("target firmware does not support lossless RMK key actions");
                            }
                            crate::keyboard::KeyBinding::Rmk(action)
                        }
                        None => map_entlayout_keycode(keycode, bundle, &layout, limits).into(),
                    };
                    if layout
                        .layers
                        .get(target_layer_idx)
                        .and_then(|layer| layer.get(target_key_idx))
                        .copied()
                        == Some(binding)
                    {
                        continue;
                    }
                    let key = &layout.keys[target_key_idx];
                    match binding {
                        crate::keyboard::KeyBinding::Vial(keycode) => {
                            hid.set_keycode(target_layer_idx as u8, key.row, key.col, keycode)?;
                        }
                        crate::keyboard::KeyBinding::Rmk(action) => {
                            hid.set_rmk_key_action(
                                target_layer_idx as u8,
                                key.row,
                                key.col,
                                action,
                            )?;
                        }
                    }
                }
            }
            Ok(())
        })() {
            failures.push(crate::i18n::tr_catalog_format(
                lang,
                "entlayout.firmware_failure",
                &[
                    ("section", crate::i18n::tr_catalog(lang, "entlayout.keymap")),
                    ("error", &err.to_string()),
                ],
            ));
        }

        // Layer names are written after local state sets self.layer_names, via
        // the shared write_layer_names_to_firmware writer (see apply_entlayout),
        // so a single failing layer no longer aborts the rest.

        if let Err(err) = (|| -> Result<()> {
            for (source_layer_idx, layer_codes) in bundle.data.encoder_keymap.iter().enumerate() {
                let Some(target_layer_idx) = map_layer_index(source_layer_idx, self.layer_count)
                else {
                    continue;
                };
                for (source_encoder_idx, keycode) in layer_codes.iter().copied().enumerate() {
                    let Some(target_encoder_idx) = mapping
                        .encoder_mapping
                        .get(source_encoder_idx)
                        .copied()
                        .flatten()
                    else {
                        continue;
                    };
                    let keycode = map_entlayout_keycode(keycode, bundle, &layout, limits);
                    if layout
                        .encoder_layers
                        .get(target_layer_idx)
                        .and_then(|layer| layer.get(target_encoder_idx))
                        .copied()
                        == Some(keycode)
                    {
                        continue;
                    }
                    let encoder = &layout.encoders[target_encoder_idx];
                    hid.set_encoder(
                        target_layer_idx as u8,
                        encoder.encoder_idx,
                        encoder.direction,
                        keycode,
                    )?;
                }
            }
            Ok(())
        })() {
            failures.push(crate::i18n::tr_catalog_format(
                lang,
                "entlayout.firmware_failure",
                &[
                    (
                        "section",
                        crate::i18n::tr_catalog(lang, "entlayout.encoder_keymap"),
                    ),
                    ("error", &err.to_string()),
                ],
            ));
        }

        if mapping.exact_layout {
            if let Some(options) = bundle.data.layout_options {
                if self.layout_options_value.is_some() && self.layout_options_value != Some(options)
                {
                    if let Err(err) = hid.set_layout_options(options) {
                        failures.push(crate::i18n::tr_catalog_format(
                            lang,
                            "entlayout.firmware_failure",
                            &[
                                (
                                    "section",
                                    crate::i18n::tr_catalog(lang, "entlayout.layout_options"),
                                ),
                                ("error", &err.to_string()),
                            ],
                        ));
                    }
                }
            }
        }

        for setting in portable_settings {
            let result = write_entlayout_portable_setting(hid, &setting);
            match result {
                Ok(()) => {
                    portable_settings_report.record_imported();
                    verified_portable_settings.push(setting.clone());
                }
                Err(error) => {
                    if setting.is_layer_led_color() {
                        failed_layer_colors.insert(setting.id.clone());
                    }
                    portable_settings_report.record_failed();
                    failures.push(crate::i18n::tr_catalog_format(
                        lang,
                        "entlayout.portable_setting_failure",
                        &[
                            ("setting", &setting.id),
                            ("qsid", &setting.qsid.to_string()),
                            ("error", &error.to_string()),
                        ],
                    ));
                }
            }
        }

        verified_portable_settings.retain(|setting| !failed_layer_colors.contains(&setting.id));

        if self.current_device_is_likely_rmk() {
            for setting in &verified_portable_settings {
                self.apply_entlayout_portable_setting_local(setting);
            }
            return Ok((failures, portable_settings_report));
        }

        if entmacro_count(&bundle.data.macros) > 0 && self.keycode_picker.macro_count > 0 {
            if let Err(err) = (|| -> Result<()> {
                let mut macro_texts = entmacro_bytecode(&bundle.data.macros)?;
                macro_texts.resize(self.keycode_picker.macro_count, Vec::new());
                macro_texts.truncate(self.keycode_picker.macro_count);

                let mut current_macro_texts = self.keycode_picker.macro_texts.clone();
                current_macro_texts.resize(self.keycode_picker.macro_count, Vec::new());
                current_macro_texts.truncate(self.keycode_picker.macro_count);
                if macro_texts == current_macro_texts {
                    return Ok(());
                }

                let size = hid.get_macro_buffer_size()?;
                let buf = crate::hid::HidDevice::encode_macros(&macro_texts, size)?;
                hid.set_macro_buffer(&buf)?;
                if bundle.version == MACROPAD_ENTLAYOUT_VERSION {
                    let confirmed =
                        hid.get_macro_buffer(size, self.keycode_picker.macro_count as u8)?;
                    if crate::hid::HidDevice::parse_macros(
                        &confirmed,
                        self.keycode_picker.macro_count as u8,
                    ) != macro_texts
                    {
                        bail!("Macro bytecode readback mismatch");
                    }
                }
                Ok(())
            })() {
                failures.push_section("entlayout.macros", format!("macros ({err})"));
            }
        }

        if let Err(err) = (|| -> Result<()> {
            for (idx, combo) in bundle
                .data
                .combos
                .entries
                .iter()
                .take(self.combo_entries.len())
                .enumerate()
            {
                let Some(combo) = remap_entcombo_entry(combo, bundle, &layout, limits) else {
                    continue;
                };
                let output =
                    entlayout_dynamic_binding(combo.output, combo.native_output.as_deref())?;
                if output.rmk_action().is_some() && !supports_rmk_native_combo_output {
                    bail!("target firmware does not support native RMK Combo outputs");
                }
                if combo.layer.is_some() && !supports_rmk_combo_layers {
                    bail!("target firmware does not support layer-specific Combos");
                }
                let current = &self.combo_entries[idx];
                if current.keys != combo.keys || current.output != output {
                    hid.set_combo_binding(idx as u8, combo.keys, output)?;
                }
                if supports_rmk_combo_layers && current.layer != combo.layer {
                    hid.set_rmk_combo_layer(idx as u8, combo.layer)?;
                }
            }
            Ok(())
        })() {
            failures.push_section("entlayout.combos", format!("combos ({err})"));
        }
        if let Some(term) = bundle.data.combos.term {
            if self.combo_term.is_some() && self.combo_term != Some(term) {
                if let Err(err) = hid.set_qmk_setting_u16(2, term) {
                    failures.push_section("entlayout.combos", format!("combo term ({err})"));
                }
            }
        }

        if let Err(err) = (|| -> Result<()> {
            for (idx, td) in bundle
                .data
                .tap_dance
                .entries
                .iter()
                .take(self.keycode_picker.tap_dance_entries.len())
                .enumerate()
            {
                let Some(td) = remap_enttap_dance_entry(td, bundle, &layout, limits) else {
                    continue;
                };
                let actions = [
                    entlayout_dynamic_binding(td.on_tap, td.native_on_tap.as_deref())?,
                    entlayout_dynamic_binding(td.on_hold, td.native_on_hold.as_deref())?,
                    entlayout_dynamic_binding(
                        td.on_double_tap,
                        td.native_on_double_tap.as_deref(),
                    )?,
                    entlayout_dynamic_binding(td.on_tap_hold, td.native_on_tap_hold.as_deref())?,
                ];
                if actions.iter().any(|binding| binding.rmk_action().is_some())
                    && !supports_rmk_native_tap_dance_actions
                {
                    bail!("target firmware does not support native RMK Tap Dance actions");
                }
                let current = &self.keycode_picker.tap_dance_entries[idx];
                if current.on_tap == actions[0]
                    && current.on_hold == actions[1]
                    && current.on_double_tap == actions[2]
                    && current.on_tap_hold == actions[3]
                    && current.tapping_term == td.tapping_term
                {
                    continue;
                }
                hid.set_tap_dance_bindings(idx as u8, actions, td.tapping_term)?;
            }
            Ok(())
        })() {
            failures.push_section("entlayout.tap_dance", format!("tap dance ({err})"));
        }

        if let Err(err) = (|| -> Result<()> {
            for (idx, ko) in bundle
                .data
                .key_overrides
                .entries
                .iter()
                .take(self.key_override_entries.len())
                .enumerate()
            {
                let Some(ko) = remap_entkey_override_entry(ko, bundle, &layout, limits) else {
                    continue;
                };
                let current = &self.key_override_entries[idx];
                if current.trigger == ko.trigger
                    && current.replacement == ko.replacement
                    && current.layers == ko.layers
                    && current.trigger_mods == ko.trigger_mods
                    && current.negative_mod_mask == ko.negative_mod_mask
                    && current.suppressed_mods == ko.suppressed_mods
                    && current.options.bits() == ko.options
                {
                    continue;
                }
                hid.set_key_override(
                    idx as u8,
                    ko.trigger,
                    ko.replacement,
                    ko.layers,
                    ko.trigger_mods,
                    ko.negative_mod_mask,
                    ko.suppressed_mods,
                    ko.options,
                )?;
            }
            Ok(())
        })() {
            failures.push_section("entlayout.key_overrides", format!("key overrides ({err})"));
        }

        if let Err(err) = (|| -> Result<()> {
            for (idx, ar) in bundle
                .data
                .alt_repeat
                .entries
                .iter()
                .take(self.alt_repeat_entries.len())
                .enumerate()
            {
                let Some(ar) = remap_entalt_repeat_entry(ar, bundle, &layout, limits) else {
                    continue;
                };
                let current = &self.alt_repeat_entries[idx];
                if current.keycode == ar.keycode
                    && current.alt_keycode == ar.alt_keycode
                    && current.allowed_mods == ar.allowed_mods
                    && current.options.bits() == ar.options
                {
                    continue;
                }
                hid.set_alt_repeat_key(
                    idx as u8,
                    ar.keycode,
                    ar.alt_keycode,
                    ar.allowed_mods,
                    ar.options,
                )?;
            }
            Ok(())
        })() {
            failures.push_section("entlayout.alt_repeat", format!("alt repeat ({err})"));
        }
        for setting in &verified_portable_settings {
            self.apply_entlayout_portable_setting_local(setting);
        }
        Ok((failures, portable_settings_report))
    }

    fn apply_entlayout_local_state(
        &mut self,
        bundle: &EntLayoutFile,
        mapping: &EntLayoutImportMapping,
    ) -> Result<()> {
        let limits = self.entlayout_keycode_import_limits();
        let supports_rmk_native_key_actions = self.keycode_picker.supports_rmk_native_key_actions;
        if let Some(layout) = &mut self.layout {
            let mut target_layers = layout.layers.clone();
            for (source_layer_idx, layer_codes) in bundle.data.keymap.iter().enumerate() {
                let Some(target_layer_idx) = map_layer_index(source_layer_idx, target_layers.len())
                else {
                    continue;
                };
                for (source_key_idx, keycode) in layer_codes.iter().copied().enumerate() {
                    let Some(target_key_idx) =
                        mapping.key_mapping.get(source_key_idx).copied().flatten()
                    else {
                        continue;
                    };
                    if let Some(slot) = target_layers
                        .get_mut(target_layer_idx)
                        .and_then(|layer| layer.get_mut(target_key_idx))
                    {
                        match entlayout_native_key_action(bundle, source_layer_idx, source_key_idx)?
                        {
                            Some(action) if supports_rmk_native_key_actions => {
                                *slot = action.into();
                            }
                            Some(_) => {}
                            None => {
                                *slot =
                                    map_entlayout_keycode(keycode, bundle, layout, limits).into();
                            }
                        }
                    }
                }
            }
            layout.layers = target_layers;

            let mut target_encoder_layers = layout.encoder_layers.clone();
            for (source_layer_idx, layer_codes) in bundle.data.encoder_keymap.iter().enumerate() {
                let Some(target_layer_idx) =
                    map_layer_index(source_layer_idx, target_encoder_layers.len())
                else {
                    continue;
                };
                for (source_encoder_idx, keycode) in layer_codes.iter().copied().enumerate() {
                    let Some(target_encoder_idx) = mapping
                        .encoder_mapping
                        .get(source_encoder_idx)
                        .copied()
                        .flatten()
                    else {
                        continue;
                    };
                    if let Some(slot) = target_encoder_layers
                        .get_mut(target_layer_idx)
                        .and_then(|layer| layer.get_mut(target_encoder_idx))
                    {
                        *slot = map_entlayout_keycode(keycode, bundle, layout, limits);
                    }
                }
            }
            layout.encoder_layers = target_encoder_layers;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.deferred_device_load.is_staged() {
            for source_layer_idx in 0..bundle.data.keymap.len() {
                if let Some(target_layer_idx) = map_layer_index(source_layer_idx, self.layer_count)
                {
                    self.deferred_device_load
                        .set_layer_status(target_layer_idx, DeferredLoadStatus::NotLoaded);
                }
            }
        }
        let mut encoder_visibility = self.encoder_visibility.clone();
        encoder_visibility.resize(
            self.layout
                .as_ref()
                .map(|layout| layout.encoder_count())
                .unwrap_or(0),
            true,
        );
        for (source_idx, visible) in bundle.data.encoder_visibility.iter().copied().enumerate() {
            let Some(target_idx) = mapping.encoder_mapping.get(source_idx).copied().flatten()
            else {
                continue;
            };
            if let Some(slot) = encoder_visibility.get_mut(target_idx) {
                *slot = visible;
            }
        }
        self.encoder_visibility = encoder_visibility;
        if !self.current_encoder_visibility_id.is_empty() {
            save_encoder_visibility(
                &self.encoder_visibility,
                &self.current_encoder_visibility_id,
            );
        }

        if mapping.exact_layout && self.layout_options_value.is_some() {
            self.layout_options_value = bundle.data.layout_options;
        }
        for (idx, name) in bundle
            .data
            .layer_names
            .iter()
            .take(self.layer_names.len().min(self.layer_count))
            .enumerate()
        {
            self.layer_names[idx] = name.clone();
        }
        self.layer_names.truncate(self.layer_count);
        if !self.current_device_name.is_empty() {
            save_layer_names(&self.layer_names, &self.current_device_name);
        }

        if let (Some(source), Some(layout), Some(device_key)) = (
            bundle.data.layout_element_visibility.as_ref(),
            self.layout.as_ref(),
            self.layout_element_visibility_device_key(),
        ) {
            let current = self
                .app_settings
                .layout_element_visibility
                .get(&device_key)
                .cloned()
                .unwrap_or_default();
            let mapped = map_entlayout_element_visibility(
                source,
                bundle.data.source_layout.as_ref(),
                mapping,
                layout,
                &current,
            );
            if mapped == LayoutElementVisibility::default() {
                self.app_settings
                    .layout_element_visibility
                    .remove(&device_key);
            } else {
                self.app_settings
                    .layout_element_visibility
                    .insert(device_key, mapped);
            }
        }

        // Macropad renders Default from host profile storage, not self.layout.
        // Update only imported positions in that profile; other applications,
        // encoder stacks and unmapped controls must remain untouched.
        if let (Some(layout), Some(device_key), true) = (
            self.layout.as_ref(),
            self.selected_device
                .and_then(|_| self.application_layout_device_key()),
            bundle.application_layouts.is_none(),
        ) {
            if let Some(profile) = self
                .app_settings
                .application_layouts
                .get_mut(&device_key)
                .and_then(|settings| {
                    settings
                        .layouts
                        .get_mut(crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID)
                })
            {
                for source_layer in 0..bundle.data.keymap.len() {
                    let Some(layer) = map_layer_index(source_layer, self.layer_count) else {
                        continue;
                    };
                    for target in mapping.key_mapping.iter().flatten().copied() {
                        if let Some(control) = Self::application_control_for_key(layout, target) {
                            profile.set_keycode(layer, control, layout.get_keycode(layer, target));
                        }
                    }
                }
                for source_layer in 0..bundle.data.encoder_keymap.len() {
                    let Some(layer) = map_layer_index(source_layer, self.layer_count) else {
                        continue;
                    };
                    for target in mapping.encoder_mapping.iter().flatten().copied() {
                        if let Some(control) = Self::application_control_for_encoder(layout, target)
                        {
                            profile.set_keycode(
                                layer,
                                control,
                                layout.get_encoder_keycode(layer, target),
                            );
                        }
                    }
                }
                for (layer, name) in bundle
                    .data
                    .layer_names
                    .iter()
                    .take(self.layer_count)
                    .enumerate()
                {
                    profile.set_layer_name(layer, name.clone());
                }
            }
        }

        if bundle.application_layouts.is_none() {
            self.app_settings.text_expander_enabled = bundle.data.text_expander.enabled;
            self.app_settings.text_expander_app_blacklist =
                bundle.data.text_expander.app_blacklist.clone();
            self.app_settings.text_expander_rule_files =
                normalize_text_expander_rule_files(&bundle.data.text_expander.rule_files);
            self.app_settings.text_expansion_rules =
                bundle.data.text_expander.primary_rules.clone();
            save_app_settings(&self.app_settings);
            save_text_expansion_rules(&self.app_settings.text_expansion_rules);
            for file in &bundle.data.text_expander.extra_files {
                if let Some(file_name) = normalize_text_expander_rules_file_name(&file.file_name) {
                    let path = text_expander_extra_rules_path(&file_name);
                    if let Ok(json) = serde_json::to_string_pretty(&file.rules) {
                        std::fs::write(path, json)?;
                    }
                }
            }
            self.text_expander_rules_signature =
                text_expander_rules_signature(&self.app_settings.text_expander_rule_files);
            self.sync_text_expander_runtime();
        }

        if self.current_device_is_likely_rmk() {
            return Ok(());
        }

        self.keycode_picker.macro_texts = entmacro_bytecode(&bundle.data.macros)?;
        self.keycode_picker.macro_texts.resize(
            self.keycode_picker
                .macro_count
                .max(entmacro_count(&bundle.data.macros)),
            Vec::new(),
        );
        self.keycode_picker
            .macro_texts
            .truncate(self.keycode_picker.macro_count);
        self.keycode_picker.macro_names = normalized_strings(
            &bundle.data.macros.names,
            self.keycode_picker
                .macro_count
                .max(bundle.data.macros.names.len()),
        );
        self.keycode_picker
            .macro_names
            .truncate(self.keycode_picker.macro_count);
        self.keycode_picker.macro_descriptions = normalized_strings(
            &bundle.data.macros.descriptions,
            self.keycode_picker
                .macro_count
                .max(bundle.data.macros.descriptions.len()),
        );
        self.keycode_picker
            .macro_descriptions
            .truncate(self.keycode_picker.macro_count);
        if bundle.application_layouts.is_none() {
            save_macro_metadata(
                &self.keycode_picker.macro_names,
                &self.keycode_picker.macro_descriptions,
                &self.current_device_name,
            );
        }
        self.keycode_picker.macro_metadata_dirty = false;
        self.keycode_picker.mark_macros_clean();

        if let Some(layout) = &self.layout {
            let mut combo_entries = self.combo_entries.clone();
            for (idx, entry) in bundle
                .data
                .combos
                .entries
                .iter()
                .take(combo_entries.len())
                .enumerate()
            {
                if let Some(entry) = remap_entcombo_entry(entry, bundle, layout, limits) {
                    let layer = if self.supports_rmk_combo_layers {
                        entry.layer
                    } else {
                        combo_entries[idx].layer
                    };
                    combo_entries[idx] = ComboEntry {
                        keys: entry.keys,
                        output: entlayout_dynamic_binding(
                            entry.output,
                            entry.native_output.as_deref(),
                        )?,
                        layer,
                    };
                }
            }
            self.combo_entries = combo_entries;

            let mut tap_dance_entries = self.keycode_picker.tap_dance_entries.clone();
            for (idx, entry) in bundle
                .data
                .tap_dance
                .entries
                .iter()
                .take(tap_dance_entries.len())
                .enumerate()
            {
                if let Some(entry) = remap_enttap_dance_entry(entry, bundle, layout, limits) {
                    tap_dance_entries[idx] = crate::keycode_picker::TapDanceEntry {
                        on_tap: entlayout_dynamic_binding(
                            entry.on_tap,
                            entry.native_on_tap.as_deref(),
                        )?,
                        on_hold: entlayout_dynamic_binding(
                            entry.on_hold,
                            entry.native_on_hold.as_deref(),
                        )?,
                        on_double_tap: entlayout_dynamic_binding(
                            entry.on_double_tap,
                            entry.native_on_double_tap.as_deref(),
                        )?,
                        on_tap_hold: entlayout_dynamic_binding(
                            entry.on_tap_hold,
                            entry.native_on_tap_hold.as_deref(),
                        )?,
                        tapping_term: entry.tapping_term,
                    };
                }
            }
            self.keycode_picker.tap_dance_entries = tap_dance_entries;

            let mut key_override_entries = self.key_override_entries.clone();
            for (idx, entry) in bundle
                .data
                .key_overrides
                .entries
                .iter()
                .take(key_override_entries.len())
                .enumerate()
            {
                if let Some(entry) = remap_entkey_override_entry(entry, bundle, layout, limits) {
                    let mut mapped = KeyOverrideEntry {
                        trigger: entry.trigger,
                        replacement: entry.replacement,
                        layers: entry.layers,
                        trigger_mods: entry.trigger_mods,
                        negative_mod_mask: entry.negative_mod_mask,
                        suppressed_mods: entry.suppressed_mods,
                        options: KeyOverrideOptionsState::from_bits(entry.options),
                    };
                    Self::normalize_key_override_entry(&mut mapped);
                    key_override_entries[idx] = mapped;
                }
            }
            self.key_override_entries = key_override_entries;

            let mut alt_repeat_entries = self.alt_repeat_entries.clone();
            for (idx, entry) in bundle
                .data
                .alt_repeat
                .entries
                .iter()
                .take(alt_repeat_entries.len())
                .enumerate()
            {
                if let Some(entry) = remap_entalt_repeat_entry(entry, bundle, layout, limits) {
                    let mut mapped = AltRepeatKeyEntry {
                        keycode: entry.keycode,
                        alt_keycode: entry.alt_keycode,
                        allowed_mods: entry.allowed_mods,
                        options: AltRepeatKeyOptionsState::from_bits(entry.options),
                    };
                    Self::normalize_alt_repeat_entry(&mut mapped);
                    alt_repeat_entries[idx] = mapped;
                }
            }
            self.alt_repeat_entries = alt_repeat_entries;
        }

        self.combo_names = normalized_strings(&bundle.data.combos.names, self.combo_entries.len());
        self.combo_colors = bundle.data.combos.colors.clone();
        migrate_legacy_combo_default_colors(&mut self.combo_colors);
        normalize_combo_colors(&mut self.combo_colors, self.combo_entries.len());
        self.combo_term = bundle.data.combos.term.or(self.combo_term);
        if bundle.application_layouts.is_none() {
            save_combo_names(&self.combo_names, &self.current_device_name);
            save_combo_colors(&self.combo_colors, &self.current_device_name);
        }
        self.combo_synced_entries = self.combo_entries.clone();
        self.combo_dirty = false;
        self.combo_edit_revision = self.combo_edit_revision.wrapping_add(1);
        self.combo_attempted_revision = None;
        self.combo_names_dirty = false;
        self.combo_colors_dirty = false;
        self.combo_term_dirty = false;

        self.keycode_picker.tap_dance_names = normalized_strings(
            &bundle.data.tap_dance.names,
            self.keycode_picker.tap_dance_entries.len(),
        );
        if bundle.application_layouts.is_none() {
            save_tap_dance_names(
                &self.keycode_picker.tap_dance_names,
                &self.current_device_name,
            );
        }
        self.keycode_picker.tap_dance_dirty = false;

        self.key_override_names = normalized_strings(
            &bundle.data.key_overrides.names,
            self.key_override_entries.len(),
        );
        if bundle.application_layouts.is_none() {
            save_key_override_names(&self.key_override_names, &self.current_device_name);
        }

        self.alt_repeat_names =
            normalized_strings(&bundle.data.alt_repeat.names, self.alt_repeat_entries.len());
        if bundle.application_layouts.is_none() {
            save_alt_repeat_names(&self.alt_repeat_names, &self.current_device_name);
        }

        #[cfg(not(target_arch = "wasm32"))]
        if self.deferred_device_load.is_staged() {
            for section in [
                DeferredLoadSection::Macros,
                DeferredLoadSection::Combos,
                DeferredLoadSection::TapDance,
                DeferredLoadSection::KeyOverrides,
                DeferredLoadSection::AltRepeat,
            ] {
                if self.deferred_device_load.section_supported(section) {
                    self.deferred_device_load
                        .set_section_status(section, DeferredLoadStatus::NotLoaded);
                }
            }
        }

        Ok(())
    }
}

fn entlayout_source_layout(layout: &KeyboardLayout) -> EntLayoutSourceLayout {
    EntLayoutSourceLayout {
        keys: layout
            .keys
            .iter()
            .enumerate()
            .map(|(index, key)| EntLayoutSourceKey {
                index,
                row: key.row,
                col: key.col,
                x: key.x,
                y: key.y,
                w: key.w,
                h: key.h,
                rotation: key.rotation,
                rotation_x: key.rotation_x,
                rotation_y: key.rotation_y,
                label: key.label.clone(),
            })
            .collect(),
        encoders: layout
            .encoders
            .iter()
            .enumerate()
            .map(|(visual_index, encoder)| EntLayoutSourceEncoder {
                visual_index,
                encoder_idx: encoder.encoder_idx,
                direction: encoder.direction,
                x: encoder.x,
                y: encoder.y,
                w: encoder.w,
                h: encoder.h,
                rotation: encoder.rotation,
                rotation_x: encoder.rotation_x,
                rotation_y: encoder.rotation_y,
                label: encoder.label.clone(),
            })
            .collect(),
        custom_keycodes: layout
            .custom_keycodes
            .iter()
            .enumerate()
            .map(|(index, custom)| EntLayoutSourceCustomKeycode {
                index,
                name: custom.name.clone(),
                label: custom.label.clone(),
                title: custom.title.clone(),
            })
            .collect(),
    }
}

fn bundle_source_key_count(bundle: &EntLayoutFile) -> usize {
    bundle
        .data
        .source_layout
        .as_ref()
        .map(|source| source.keys.len())
        .unwrap_or(bundle.keyboard.keys)
}

fn bundle_source_encoder_count(bundle: &EntLayoutFile) -> usize {
    bundle
        .data
        .source_layout
        .as_ref()
        .map(|source| source.encoders.len())
        .unwrap_or_else(|| {
            bundle
                .data
                .encoder_keymap
                .iter()
                .map(Vec::len)
                .max()
                .unwrap_or(bundle.keyboard.encoders)
        })
}

fn map_layer_index(source_idx: usize, target_len: usize) -> Option<usize> {
    (source_idx < target_len).then_some(source_idx)
}

fn universal_key_mapping(bundle: &EntLayoutFile, layout: &KeyboardLayout) -> Vec<Option<usize>> {
    let Some(source) = bundle.data.source_layout.as_ref() else {
        let len = bundle_source_key_count(bundle);
        return (0..len)
            .map(|idx| (idx < layout.keys.len()).then_some(idx))
            .collect();
    };
    let source_points: Vec<_> = source.keys.iter().map(source_key_point).collect();
    let target_points: Vec<_> = layout.keys.iter().map(target_key_point).collect();
    let (source_bounds, target_bounds) =
        (points_bounds(&source_points), points_bounds(&target_points));
    let mut used = vec![false; layout.keys.len()];
    source
        .keys
        .iter()
        .zip(source_points.iter().copied())
        .map(|(source_key, source_point)| {
            let normalized = normalize_point(source_point, source_bounds);
            let mut best: Option<(usize, f32)> = None;
            for (target_idx, target_key) in layout.keys.iter().enumerate() {
                if used[target_idx] {
                    continue;
                }
                let target_normalized = normalize_point(target_points[target_idx], target_bounds);
                let mut score = point_distance_squared(normalized, target_normalized);
                let size_penalty = ((source_key.w - target_key.w).abs()
                    + (source_key.h - target_key.h).abs())
                    * 0.03;
                score += size_penalty;
                if source_key.label == target_key.label {
                    score *= 0.65;
                }
                if source_key.row == target_key.row && source_key.col == target_key.col {
                    score *= 0.75;
                }
                if best
                    .map(|(_, best_score)| score < best_score)
                    .unwrap_or(true)
                {
                    best = Some((target_idx, score));
                }
            }
            let (target_idx, score) = best?;
            let threshold = if source.keys.len() == layout.keys.len() {
                0.045
            } else {
                0.032
            };
            if score <= threshold {
                used[target_idx] = true;
                Some(target_idx)
            } else {
                None
            }
        })
        .collect()
}

fn universal_encoder_mapping(
    bundle: &EntLayoutFile,
    layout: &KeyboardLayout,
) -> Vec<Option<usize>> {
    let Some(source) = bundle.data.source_layout.as_ref() else {
        let len = bundle_source_encoder_count(bundle);
        return (0..len)
            .map(|idx| (idx < layout.encoders.len()).then_some(idx))
            .collect();
    };
    let source_points: Vec<_> = source.encoders.iter().map(source_encoder_point).collect();
    let target_points: Vec<_> = layout.encoders.iter().map(target_encoder_point).collect();
    let (source_bounds, target_bounds) =
        (points_bounds(&source_points), points_bounds(&target_points));
    let mut used = vec![false; layout.encoders.len()];
    source
        .encoders
        .iter()
        .zip(source_points.iter().copied())
        .map(|(source_encoder, source_point)| {
            let normalized = normalize_point(source_point, source_bounds);
            let mut best: Option<(usize, f32)> = None;
            for (target_idx, target_encoder) in layout.encoders.iter().enumerate() {
                if used[target_idx] || source_encoder.direction != target_encoder.direction {
                    continue;
                }
                let target_normalized = normalize_point(target_points[target_idx], target_bounds);
                let mut score = point_distance_squared(normalized, target_normalized);
                if source_encoder.encoder_idx == target_encoder.encoder_idx {
                    score *= 0.8;
                }
                if best
                    .map(|(_, best_score)| score < best_score)
                    .unwrap_or(true)
                {
                    best = Some((target_idx, score));
                }
            }
            let (target_idx, score) = best?;
            if score <= 0.05 {
                used[target_idx] = true;
                Some(target_idx)
            } else {
                None
            }
        })
        .collect()
}

fn source_key_point(key: &EntLayoutSourceKey) -> (f32, f32) {
    rotated_layout_point(
        key.x + key.w * 0.5,
        key.y + key.h * 0.5,
        key.rotation_x,
        key.rotation_y,
        key.rotation,
    )
}

fn target_key_point(key: &crate::keyboard::PhysicalKey) -> (f32, f32) {
    rotated_layout_point(
        key.x + key.w * 0.5,
        key.y + key.h * 0.5,
        key.rotation_x,
        key.rotation_y,
        key.rotation,
    )
}

fn source_encoder_point(encoder: &EntLayoutSourceEncoder) -> (f32, f32) {
    rotated_layout_point(
        encoder.x + encoder.w * 0.5,
        encoder.y + encoder.h * 0.5,
        encoder.rotation_x,
        encoder.rotation_y,
        encoder.rotation,
    )
}

fn target_encoder_point(encoder: &crate::keyboard::PhysicalEncoder) -> (f32, f32) {
    rotated_layout_point(
        encoder.x + encoder.w * 0.5,
        encoder.y + encoder.h * 0.5,
        encoder.rotation_x,
        encoder.rotation_y,
        encoder.rotation,
    )
}

fn rotated_layout_point(
    x: f32,
    y: f32,
    origin_x: f32,
    origin_y: f32,
    rotation_deg: f32,
) -> (f32, f32) {
    if rotation_deg == 0.0 {
        return (x, y);
    }
    let angle = rotation_deg.to_radians();
    let dx = x - origin_x;
    let dy = y - origin_y;
    (
        origin_x + dx * angle.cos() - dy * angle.sin(),
        origin_y + dx * angle.sin() + dy * angle.cos(),
    )
}

fn points_bounds(points: &[(f32, f32)]) -> (f32, f32, f32, f32) {
    if points.is_empty() {
        return (0.0, 0.0, 1.0, 1.0);
    }
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in points {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    (min_x, min_y, max_x, max_y)
}

fn normalize_point(
    (x, y): (f32, f32),
    (min_x, min_y, max_x, max_y): (f32, f32, f32, f32),
) -> (f32, f32) {
    let w = (max_x - min_x).max(0.001);
    let h = (max_y - min_y).max(0.001);
    ((x - min_x) / w, (y - min_y) / h)
}

fn point_distance_squared(a: (f32, f32), b: (f32, f32)) -> f32 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    dx * dx + dy * dy
}

fn map_entlayout_keycode(
    keycode: u16,
    bundle: &EntLayoutFile,
    layout: &KeyboardLayout,
    limits: EntLayoutKeycodeImportLimits,
) -> u16 {
    try_map_entlayout_keycode(keycode, bundle, layout, limits).unwrap_or(0)
}

fn try_map_entlayout_keycode(
    keycode: u16,
    bundle: &EntLayoutFile,
    layout: &KeyboardLayout,
    limits: EntLayoutKeycodeImportLimits,
) -> Option<u16> {
    if bundle.version == MACROPAD_ENTLAYOUT_VERSION {
        return check_entlayout_keycode(keycode, bundle, layout, limits).ok();
    }
    let keycode = map_custom_keycode_by_name(keycode, bundle, layout).flatten()?;
    validate_macro_keycode(keycode, limits.macro_count)?;
    validate_tap_dance_keycode(keycode, limits.tap_dance_count)?;
    map_layer_keycode(keycode, limits.layer_count)
}

fn map_custom_keycode_by_name(
    keycode: u16,
    bundle: &EntLayoutFile,
    layout: &KeyboardLayout,
) -> Option<Option<u16>> {
    const QK_KB: u16 = 0x7E00;
    if keycode < QK_KB {
        return Some(Some(keycode));
    }
    let source_idx = keycode.checked_sub(QK_KB)? as usize;
    let Some(source_layout) = bundle.data.source_layout.as_ref() else {
        return Some((source_idx < layout.custom_keycodes.len()).then_some(keycode));
    };
    let Some(source_custom) = source_layout
        .custom_keycodes
        .iter()
        .find(|custom| custom.index == source_idx)
    else {
        return Some(Some(keycode));
    };
    let target_idx = layout
        .custom_keycodes
        .iter()
        .position(|custom| custom.name == source_custom.name)?;
    Some(Some(QK_KB + target_idx as u16))
}

fn validate_macro_keycode(keycode: u16, macro_count: usize) -> Option<()> {
    if (0x7700..=0x77FF).contains(&keycode) {
        (((keycode - 0x7700) as usize) < macro_count).then_some(())
    } else {
        Some(())
    }
}

fn validate_tap_dance_keycode(keycode: u16, tap_dance_count: usize) -> Option<()> {
    if (0x5700..=0x57FF).contains(&keycode) {
        (((keycode - 0x5700) as usize) < tap_dance_count).then_some(())
    } else {
        Some(())
    }
}

fn map_layer_keycode(keycode: u16, target_layer_count: usize) -> Option<u16> {
    if (0x5200..0x5300).contains(&keycode) {
        let op = (keycode >> 5) & 0x7;
        if op != 5 {
            let target = (keycode & 0x1F) as usize;
            if target >= target_layer_count {
                return None;
            }
        }
    } else if keycode & 0xF000 == 0x4000 {
        let target = ((keycode >> 8) & 0xF) as usize;
        if target >= target_layer_count {
            return None;
        }
    }
    Some(keycode)
}

fn remap_entcombo_entry(
    entry: &EntComboEntry,
    bundle: &EntLayoutFile,
    layout: &KeyboardLayout,
    limits: EntLayoutKeycodeImportLimits,
) -> Option<EntComboEntry> {
    if entry
        .layer
        .is_some_and(|layer| layer as usize >= layout.layers.len())
    {
        return None;
    }
    let mut keys = [0u16; 4];
    for (idx, keycode) in entry.keys.iter().copied().enumerate() {
        keys[idx] = try_map_entlayout_keycode(keycode, bundle, layout, limits)?;
    }
    Some(EntComboEntry {
        keys,
        output: try_map_entlayout_keycode(entry.output, bundle, layout, limits)?,
        native_output: entry.native_output.clone(),
        layer: entry.layer,
    })
}

fn remap_enttap_dance_entry(
    entry: &EntTapDanceEntry,
    bundle: &EntLayoutFile,
    layout: &KeyboardLayout,
    limits: EntLayoutKeycodeImportLimits,
) -> Option<EntTapDanceEntry> {
    Some(EntTapDanceEntry {
        on_tap: try_map_entlayout_keycode(entry.on_tap, bundle, layout, limits)?,
        native_on_tap: entry.native_on_tap.clone(),
        on_hold: try_map_entlayout_keycode(entry.on_hold, bundle, layout, limits)?,
        native_on_hold: entry.native_on_hold.clone(),
        on_double_tap: try_map_entlayout_keycode(entry.on_double_tap, bundle, layout, limits)?,
        native_on_double_tap: entry.native_on_double_tap.clone(),
        on_tap_hold: try_map_entlayout_keycode(entry.on_tap_hold, bundle, layout, limits)?,
        native_on_tap_hold: entry.native_on_tap_hold.clone(),
        tapping_term: entry.tapping_term,
    })
}

fn remap_entkey_override_entry(
    entry: &EntKeyOverrideEntry,
    bundle: &EntLayoutFile,
    layout: &KeyboardLayout,
    limits: EntLayoutKeycodeImportLimits,
) -> Option<EntKeyOverrideEntry> {
    Some(EntKeyOverrideEntry {
        trigger: try_map_entlayout_keycode(entry.trigger, bundle, layout, limits)?,
        replacement: try_map_entlayout_keycode(entry.replacement, bundle, layout, limits)?,
        layers: entry.layers,
        trigger_mods: entry.trigger_mods,
        negative_mod_mask: entry.negative_mod_mask,
        suppressed_mods: entry.suppressed_mods,
        options: entry.options,
    })
}

fn remap_entalt_repeat_entry(
    entry: &EntAltRepeatEntry,
    bundle: &EntLayoutFile,
    layout: &KeyboardLayout,
    limits: EntLayoutKeycodeImportLimits,
) -> Option<EntAltRepeatEntry> {
    Some(EntAltRepeatEntry {
        keycode: try_map_entlayout_keycode(entry.keycode, bundle, layout, limits)?,
        alt_keycode: try_map_entlayout_keycode(entry.alt_keycode, bundle, layout, limits)?,
        allowed_mods: entry.allowed_mods,
        options: entry.options,
    })
}

fn check_complete_native_action(
    encoded: &str,
    bundle: &EntLayoutFile,
    layout: &KeyboardLayout,
    limits: EntLayoutKeycodeImportLimits,
) -> Result<()> {
    use rmk_types::action::{Action, KeyAction};
    let check = |action| -> Result<()> {
        let code = match action {
            Action::TriggerMacro(slot) => Some(0x7700 + u16::from(slot)),
            Action::LayerOn(layer)
            | Action::LayerOff(layer)
            | Action::LayerToggle(layer)
            | Action::DefaultLayer(layer)
            | Action::LayerToggleOnly(layer)
            | Action::OneShotLayer(layer)
            | Action::PersistentDefaultLayer(layer)
            | Action::LayerOnWithModifier(layer, _) => {
                if usize::from(layer) >= limits.layer_count {
                    bail!("Unavailable native layer");
                }
                None
            }
            Action::User(slot) => Some(0x7e00 + u16::from(slot)),
            _ => None,
        };
        if let Some(code) = code {
            check_entlayout_keycode(code, bundle, layout, limits)
                .map_err(|_| anyhow::anyhow!("Unavailable native resource: {code:#06x}"))?;
        }
        Ok(())
    };
    match decode_native_key_action(encoded)? {
        KeyAction::Single(action) | KeyAction::Tap(action) => check(action)?,
        KeyAction::TapHold(tap, hold, _) => {
            check(tap)?;
            check(hold)?;
        }
        KeyAction::Morse(slot) => {
            check_entlayout_keycode(0x5700 + u16::from(slot), bundle, layout, limits)
                .map_err(|_| anyhow::anyhow!("Unavailable native Tap Dance"))?;
        }
        _ => {}
    }
    Ok(())
}

fn check_entlayout_keycode(
    keycode: u16,
    bundle: &EntLayoutFile,
    layout: &KeyboardLayout,
    limits: EntLayoutKeycodeImportLimits,
) -> std::result::Result<u16, EntLayoutSkipReason> {
    let keycode = check_custom_keycode_by_name(keycode, bundle, layout)?;
    check_macro_keycode(keycode, limits.macro_count)?;
    check_tap_dance_keycode(keycode, limits.tap_dance_count)?;
    check_layer_keycode(keycode, limits.layer_count)
}

fn check_custom_keycode_by_name(
    keycode: u16,
    bundle: &EntLayoutFile,
    layout: &KeyboardLayout,
) -> std::result::Result<u16, EntLayoutSkipReason> {
    const QK_KB: u16 = 0x7E00;
    if keycode < QK_KB {
        return Ok(keycode);
    }
    let source_idx = keycode
        .checked_sub(QK_KB)
        .map(|idx| idx as usize)
        .ok_or(EntLayoutSkipReason::UnmappableCustomKeycode)?;
    let Some(source_layout) = bundle.data.source_layout.as_ref() else {
        return if source_idx < layout.custom_keycodes.len() {
            Ok(keycode)
        } else {
            Err(EntLayoutSkipReason::UnmappableCustomKeycode)
        };
    };
    let Some(source_custom) = source_layout
        .custom_keycodes
        .iter()
        .find(|custom| custom.index == source_idx)
    else {
        if bundle.version == MACROPAD_ENTLAYOUT_VERSION
            && (0x7E00..=0x7EFF).contains(&keycode)
            && !matches!(keycode, 0x7E05 | 0x7E06)
        {
            return Err(EntLayoutSkipReason::UnmappableCustomKeycode);
        }
        return Ok(keycode);
    };
    let target_idx = layout
        .custom_keycodes
        .iter()
        .position(|custom| custom.name == source_custom.name)
        .ok_or(EntLayoutSkipReason::UnmappableCustomKeycode)?;
    Ok(QK_KB + target_idx as u16)
}

fn check_macro_keycode(
    keycode: u16,
    macro_count: usize,
) -> std::result::Result<(), EntLayoutSkipReason> {
    if (0x7700..=0x77FF).contains(&keycode) && (keycode - 0x7700) as usize >= macro_count {
        Err(EntLayoutSkipReason::MissingMacroSlot)
    } else {
        Ok(())
    }
}

fn check_tap_dance_keycode(
    keycode: u16,
    tap_dance_count: usize,
) -> std::result::Result<(), EntLayoutSkipReason> {
    if (0x5700..=0x57FF).contains(&keycode) && (keycode - 0x5700) as usize >= tap_dance_count {
        Err(EntLayoutSkipReason::MissingTapDanceSlot)
    } else {
        Ok(())
    }
}

fn check_layer_keycode(
    keycode: u16,
    target_layer_count: usize,
) -> std::result::Result<u16, EntLayoutSkipReason> {
    if (0x5200..0x5300).contains(&keycode) {
        let op = (keycode >> 5) & 0x7;
        if op != 5 {
            let target = (keycode & 0x1F) as usize;
            if target >= target_layer_count {
                return Err(EntLayoutSkipReason::MissingLayer);
            }
        }
    } else if keycode & 0xF000 == 0x4000 {
        let target = ((keycode >> 8) & 0xF) as usize;
        if target >= target_layer_count {
            return Err(EntLayoutSkipReason::MissingLayer);
        }
    }
    Ok(keycode)
}

fn check_entcombo_entry(
    entry: &EntComboEntry,
    bundle: &EntLayoutFile,
    layout: &KeyboardLayout,
    limits: EntLayoutKeycodeImportLimits,
) -> std::result::Result<(), EntLayoutSkipReason> {
    if entry
        .layer
        .is_some_and(|layer| layer as usize >= layout.layers.len())
    {
        return Err(EntLayoutSkipReason::MissingLayer);
    }
    for keycode in entry.keys {
        check_entlayout_keycode(keycode, bundle, layout, limits)?;
    }
    check_entlayout_keycode(entry.output, bundle, layout, limits)?;
    Ok(())
}

fn check_enttap_dance_entry(
    entry: &EntTapDanceEntry,
    bundle: &EntLayoutFile,
    layout: &KeyboardLayout,
    limits: EntLayoutKeycodeImportLimits,
) -> std::result::Result<(), EntLayoutSkipReason> {
    check_entlayout_keycode(entry.on_tap, bundle, layout, limits)?;
    check_entlayout_keycode(entry.on_hold, bundle, layout, limits)?;
    check_entlayout_keycode(entry.on_double_tap, bundle, layout, limits)?;
    check_entlayout_keycode(entry.on_tap_hold, bundle, layout, limits)?;
    Ok(())
}

fn check_entkey_override_entry(
    entry: &EntKeyOverrideEntry,
    bundle: &EntLayoutFile,
    layout: &KeyboardLayout,
    limits: EntLayoutKeycodeImportLimits,
) -> std::result::Result<(), EntLayoutSkipReason> {
    check_entlayout_keycode(entry.trigger, bundle, layout, limits)?;
    check_entlayout_keycode(entry.replacement, bundle, layout, limits)?;
    Ok(())
}

fn check_entalt_repeat_entry(
    entry: &EntAltRepeatEntry,
    bundle: &EntLayoutFile,
    layout: &KeyboardLayout,
    limits: EntLayoutKeycodeImportLimits,
) -> std::result::Result<(), EntLayoutSkipReason> {
    check_entlayout_keycode(entry.keycode, bundle, layout, limits)?;
    check_entlayout_keycode(entry.alt_keycode, bundle, layout, limits)?;
    Ok(())
}

fn write_entlayout_file(
    path: &Path,
    bundle: &impl Serialize,
    language: crate::i18n::Language,
) -> Result<()> {
    let json = serde_json::to_string_pretty(bundle).context(crate::i18n::tr_catalog(
        language,
        "entlayout.failed_to_serialize",
    ))?;
    std::fs::write(path, json).with_context(|| {
        crate::i18n::tr_catalog_format(
            language,
            "entlayout.failed_to_write",
            &[("path", &path.display().to_string())],
        )
    })
}

fn normalized_strings(values: &[String], len: usize) -> Vec<String> {
    let mut out = values.to_vec();
    out.resize(len, String::new());
    out.truncate(len);
    out
}

fn trailing_non_empty_strings(values: &[String]) -> Vec<String> {
    let len = values
        .iter()
        .rposition(|value| !value.trim().is_empty())
        .map(|idx| idx + 1)
        .unwrap_or(0);
    values[..len].to_vec()
}

fn entmacro_count(macros: &EntMacroData) -> usize {
    if !macros.bytecode_base64.is_empty() {
        macros.bytecode_base64.len()
    } else {
        macros.texts.len()
    }
}

fn entmacro_bytecode(macros: &EntMacroData) -> Result<Vec<Vec<u8>>> {
    if !macros.bytecode_base64.is_empty() {
        macros
            .bytecode_base64
            .iter()
            .enumerate()
            .map(|(idx, encoded)| {
                BASE64_STANDARD
                    .decode(encoded)
                    .with_context(|| format!("invalid macro bytecode_base64 at index {idx}"))
            })
            .collect()
    } else {
        Ok(macros
            .texts
            .iter()
            .map(|text| text.as_bytes().to_vec())
            .collect())
    }
}

/// Accept old offset-alternative backups only when their saved fingerprint and
/// all identities still match, and alignment alone produces the target geometry.
fn entlayout_matches_offset_alternatives(bundle: &EntLayoutFile, layout: &KeyboardLayout) -> bool {
    let Some(source) = &bundle.data.source_layout else {
        return false;
    };
    if source.keys.len() != layout.keys.len() || source.encoders.len() != layout.encoders.len() {
        return false;
    }
    let mut aligned = layout.clone();
    for (idx, (saved, key)) in source.keys.iter().zip(&mut aligned.keys).enumerate() {
        if saved.index != idx
            || (
                saved.row,
                saved.col,
                saved.w,
                saved.h,
                saved.rotation,
                &saved.label,
            ) != (key.row, key.col, key.w, key.h, key.rotation, &key.label)
        {
            return false;
        }
        (key.x, key.y, key.rotation_x, key.rotation_y) =
            (saved.x, saved.y, saved.rotation_x, saved.rotation_y);
    }
    for (idx, (saved, encoder)) in source
        .encoders
        .iter()
        .zip(&mut aligned.encoders)
        .enumerate()
    {
        if saved.visual_index != idx
            || (
                saved.encoder_idx,
                saved.direction,
                saved.w,
                saved.h,
                saved.rotation,
                &saved.label,
            ) != (
                encoder.encoder_idx,
                encoder.direction,
                encoder.w,
                encoder.h,
                encoder.rotation,
                &encoder.label,
            )
        {
            return false;
        }
        (encoder.x, encoder.y, encoder.rotation_x, encoder.rotation_y) =
            (saved.x, saved.y, saved.rotation_x, saved.rotation_y);
    }
    // Preserve the existing hash contract, including matrix and option metadata.
    if entlayout_hash(&aligned) != bundle.keyboard.layout_hash {
        return false;
    }
    crate::keyboard::normalize_layout_option_positions(&mut aligned.keys, &mut aligned.encoders);
    let same_position =
        |a: [f32; 4], b: [f32; 4]| a.into_iter().zip(b).all(|(a, b)| (a - b).abs() <= 0.0001);
    // Rotated bounds can differ slightly between floating-point implementations.
    aligned.keys.iter().zip(&layout.keys).all(|(a, b)| {
        same_position(
            [a.x, a.y, a.rotation_x, a.rotation_y],
            [b.x, b.y, b.rotation_x, b.rotation_y],
        )
    }) && aligned.encoders.iter().zip(&layout.encoders).all(|(a, b)| {
        same_position(
            [a.x, a.y, a.rotation_x, a.rotation_y],
            [b.x, b.y, b.rotation_x, b.rotation_y],
        )
    })
}

fn entlayout_hash(layout: &KeyboardLayout) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    fn feed(hash: &mut u64, bytes: &[u8]) {
        for byte in bytes {
            *hash ^= *byte as u64;
            *hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    feed(&mut hash, &layout.rows.to_le_bytes());
    feed(&mut hash, &layout.cols.to_le_bytes());
    for key in &layout.keys {
        feed(&mut hash, &key.row.to_le_bytes());
        feed(&mut hash, &key.col.to_le_bytes());
        feed(&mut hash, &key.x.to_le_bytes());
        feed(&mut hash, &key.y.to_le_bytes());
        feed(&mut hash, &key.w.to_le_bytes());
        feed(&mut hash, &key.h.to_le_bytes());
        feed(&mut hash, key.label.as_bytes());
        feed(&mut hash, &[0xff]);
    }
    for encoder in &layout.encoders {
        feed(&mut hash, &encoder.encoder_idx.to_le_bytes());
        feed(&mut hash, &encoder.direction.to_le_bytes());
        feed(&mut hash, &encoder.x.to_le_bytes());
        feed(&mut hash, &encoder.y.to_le_bytes());
        feed(&mut hash, encoder.label.as_bytes());
        feed(&mut hash, &[0xfe]);
    }
    for option in &layout.layout_options {
        feed(&mut hash, option.label.as_bytes());
        feed(&mut hash, &[0xfd]);
        for choice in &option.choices {
            feed(&mut hash, choice.as_bytes());
            feed(&mut hash, &[0xfc]);
        }
    }
    format!("fnv1a64:{hash:016x}")
}

#[cfg(test)]
#[path = "entlayout_macropad_tests.rs"]
mod macropad_tests;

#[cfg(test)]
#[path = "entlayout_module_tests.rs"]
mod module_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn phenom_app(color: u8) -> EntropyApp {
        let catalog: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../tests/fixtures/display-menu-catalog.json"))
                .unwrap();
        let definition = &catalog
            .iter()
            .find(|entry| entry["name"] == "QMK/keyboards/ergohaven/phenom/keymaps/v1/vial.json")
            .unwrap()["definition"];
        let mut app = EntropyApp::new_inert_for_test();
        app.layout = Some(KeyboardLayout::from_vial_json(definition).unwrap());
        app.layer_count = 4;
        let key_count = app.layout.as_ref().unwrap().keys.len();
        app.layout.as_mut().unwrap().layers = vec![vec![0u16.into(); key_count]; 4];
        app.supported_qmk_settings = (300..=303).collect();
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        recorder.respond_by(move |request| {
            (request[..2] == [0xfe, 0x0a]).then(|| {
                let mut response = [0; 32];
                response[1] = color;
                Ok(response)
            })
        });
        app.layer_led_settings = EntropyApp::read_layer_led_settings(
            definition,
            &app.supported_qmk_settings,
            app.layer_count,
            &hid,
        );
        app
    }

    #[test]
    fn entlayout_phenom_layer_colors_survive_export_and_import() {
        let source = phenom_app(16);
        assert_eq!(source.layer_led_settings.layer_colors.len(), 4);
        assert!(source
            .layer_led_settings
            .layer_colors
            .iter()
            .all(|s| s.value == 16));
        let json = serde_json::to_string(&source.entlayout_snapshot().unwrap()).unwrap();
        let bundle: EntLayoutFile = serde_json::from_str(&json).unwrap();
        assert_eq!(
            bundle.data.portable_settings.len(),
            4,
            "Phenom layer colors must be included in the exported layout"
        );
        let mut target = phenom_app(0);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        target.hid_device = Some(hid);
        let (failures, _, report) = target.apply_entlayout(&bundle).unwrap();
        assert!(failures.is_empty());
        assert_eq!(report.imported, 4);
        assert_eq!(target.layer_led_settings, source.layer_led_settings);
        let writes = recorder
            .requests()
            .into_iter()
            .filter(|r| r[..2] == [0xfe, 0x0b])
            .map(|r| (u16::from_le_bytes([r[2], r[3]]), r[4]))
            .collect::<Vec<_>>();
        assert_eq!(writes, vec![(300, 16), (301, 16), (302, 16), (303, 16)]);
    }

    #[test]
    fn entlayout_layer_colors_skip_unloaded_unsupported_and_invalid_values() {
        let mut app = phenom_app(16);
        app.layer_led_settings.layer_colors[0].loaded = false;
        app.supported_qmk_settings.retain(|qsid| *qsid != 301);
        app.layer_led_settings.layer_colors[2].value = 255;
        let settings = app.entlayout_portable_settings_snapshot();
        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].qsid, 303);
    }

    #[test]
    fn entlayout_layer_colors_reject_mismatched_contracts_and_duplicates() {
        let source = phenom_app(16)
            .entlayout_portable_settings_snapshot()
            .remove(0);
        let target = phenom_app(0).entlayout_portable_settings_snapshot();
        for (field, value) in [
            ("id", serde_json::json!("layer_led_color:1")),
            ("id", serde_json::json!("layer_led_color:256")),
            ("width", serde_json::json!("u16")),
            ("value", serde_json::json!(25)),
            ("variants", serde_json::json!(["Off", "Blue"])),
            ("module", serde_json::json!([])),
        ] {
            let mut json = serde_json::to_value(&source).unwrap();
            json[field] = value;
            let malformed = serde_json::from_value(json).unwrap();
            let plan = plan_entlayout_portable_settings_import(&[malformed], &target);
            assert!(plan.compatible.is_empty(), "{field}");
            assert_eq!(plan.report.incompatible, 1, "{field}");
        }
        let plan = plan_entlayout_portable_settings_import(&[source.clone(), source], &target);
        assert!(plan.compatible.is_empty());
        assert_eq!(plan.report.incompatible, 2);
    }

    #[test]
    fn entlayout_layer_colors_old_files_and_unsupported_targets_do_not_write() {
        let source = phenom_app(16);
        let mut bundle = source.entlayout_snapshot().unwrap();
        let mut target = phenom_app(0);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        target.hid_device = Some(hid);
        target.supported_qmk_settings.clear();
        let (_, _, report) = target.apply_entlayout(&bundle).unwrap();
        assert_eq!(report.unsupported, 4);
        bundle.data.portable_settings.clear();
        for version in 1..=7 {
            bundle.version = version;
            let (_, _, report) = target.apply_entlayout(&bundle).unwrap();
            assert_eq!(report.total(), 0);
        }
        assert!(target
            .layer_led_settings
            .layer_colors
            .iter()
            .all(|s| s.value == 0));
        assert!(!recorder.requests().iter().any(|r| r[..2] == [0xfe, 0x0b]));
    }

    #[test]
    fn entlayout_layer_colors_failed_usb_readback_keeps_local_colors() {
        let bundle = phenom_app(16).entlayout_snapshot().unwrap();
        let mut target = phenom_app(0);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        recorder.respond_by(|request| (request[..2] == [0xfe, 0x0a]).then(|| Ok([0; 32])));
        target.hid_device = Some(hid);
        let (failures, _, report) = target.apply_entlayout(&bundle).unwrap();
        assert_eq!(report.failed, 4);
        assert!(!failures.is_empty());
        assert!(target
            .layer_led_settings
            .layer_colors
            .iter()
            .all(|s| s.value == 0));
    }

    #[test]
    fn entlayout_layer_colors_restore_linked_registers() {
        let mut source = phenom_app(16);
        source.supported_qmk_settings.push(400);
        source.layer_led_settings.layer_colors[0]
            .linked_qsids
            .push(400);
        let bundle = source.entlayout_snapshot().unwrap();
        let mut target = phenom_app(0);
        target.supported_qmk_settings.push(400);
        target.layer_led_settings.layer_colors[0]
            .linked_qsids
            .push(400);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        target.hid_device = Some(hid);
        let (failures, _, report) = target.apply_entlayout(&bundle).unwrap();
        assert!(failures.is_empty());
        assert_eq!(report.imported, 5);
        assert_eq!(target.layer_led_settings, source.layer_led_settings);
        assert!(recorder
            .requests()
            .iter()
            .any(|r| r[..5] == [0xfe, 0x0b, 144, 1, 16]));
    }

    #[test]
    fn entlayout_layer_colors_failed_reads_do_not_export_fallback_zeroes() {
        for definition in [
            serde_json::json!({}),
            serde_json::json!({
                "settings": [{"name": "LEDs settings", "fields": [
                    {"type": "select", "title": "Layer 0 color", "qsid": 300}
                ]}]
            }),
        ] {
            let mut app = phenom_app(16);
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            recorder.respond_by(|request| {
                (request[..2] == [0xfe, 0x0a]).then(|| Err("read failed".to_owned()))
            });
            app.layer_led_settings = EntropyApp::read_layer_led_settings(
                &definition,
                &app.supported_qmk_settings,
                app.layer_count,
                &hid,
            );
            assert!(app.layer_led_settings.supported);
            assert!(app.entlayout_portable_settings_snapshot().is_empty());
        }
    }

    #[test]
    fn entlayout_layer_colors_partial_linked_failure_can_be_retried() {
        let mut source = phenom_app(16);
        source.supported_qmk_settings.push(400);
        source.layer_led_settings.layer_colors[0]
            .linked_qsids
            .push(400);
        let bundle = source.entlayout_snapshot().unwrap();
        let mut target = phenom_app(0);
        target.supported_qmk_settings.push(400);
        target.layer_led_settings.layer_colors[0]
            .linked_qsids
            .push(400);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        recorder.respond_by(|request| {
            (request[..4] == [0xfe, 0x0b, 144, 1])
                .then(|| Err("linked register SET failed".to_owned()))
        });
        target.hid_device = Some(hid);
        let (_, _, report) = target.apply_entlayout(&bundle).unwrap();
        assert_eq!(report.failed, 1);
        assert_eq!(target.layer_led_settings.layer_colors[0].value, 0);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        target.hid_device = Some(hid);
        let (failures, _, report) = target.apply_entlayout(&bundle).unwrap();
        assert!(failures.is_empty());
        assert_eq!(report.failed, 0);
        assert!(recorder
            .requests()
            .iter()
            .any(|r| r[..5] == [0xfe, 0x0b, 144, 1, 16]));
    }

    #[test]
    fn entlayout_layer_colors_reconnect_repairs_linked_registers() {
        let mut source = phenom_app(16);
        source.supported_qmk_settings.push(400);
        source.layer_led_settings.layer_colors[0]
            .linked_qsids
            .push(400);
        let bundle = source.entlayout_snapshot().unwrap();
        let mut target = phenom_app(0);
        target.supported_qmk_settings.push(400);
        let definition = serde_json::json!({
            "settings": [{"name": "LEDs settings", "fields": [
                {"type": "select", "title": "Layer 0 color", "qsid": 300},
                {"type": "select", "title": "Layer 0 color", "qsid": 400}
            ]}]
        });
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        recorder.respond_by(|request| {
            (request[..2] == [0xfe, 0x0a]).then(|| {
                let mut response = [0; 32];
                // Reconnect after only the primary register was written successfully.
                response[1] = if request[2..4] == [44, 1] { 16 } else { 0 };
                Ok(response)
            })
        });
        target.layer_led_settings = EntropyApp::read_layer_led_settings(
            &definition,
            &target.supported_qmk_settings,
            target.layer_count,
            &hid,
        );
        assert_eq!(target.layer_led_settings.layer_colors[0].value, 16);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        target.hid_device = Some(hid);
        let (failures, _, report) = target.apply_entlayout(&bundle).unwrap();
        assert!(failures.is_empty());
        assert_eq!(report.failed, 0);
        assert!(recorder
            .requests()
            .iter()
            .any(|r| r[..5] == [0xfe, 0x0b, 144, 1, 16]));
    }

    #[test]
    fn entlayout_layer_colors_incomplete_or_conflicting_linked_group_is_rejected() {
        let mut source = phenom_app(16);
        source.supported_qmk_settings.push(400);
        source.layer_led_settings.layer_colors[0]
            .linked_qsids
            .push(400);
        let settings = source.entlayout_portable_settings_snapshot();
        let mut target = source.entlayout_portable_settings_snapshot();
        for setting in &mut target {
            setting.value = 0;
        }
        for conflicting in [false, true] {
            let mut malformed = settings.clone();
            if conflicting {
                malformed.iter_mut().find(|s| s.qsid == 400).unwrap().value = 2;
            } else {
                malformed.retain(|s| s.qsid != 400);
            }
            let plan = plan_entlayout_portable_settings_import(&malformed, &target);
            assert!(plan.compatible.iter().all(|s| s.id != "layer_led_color:0"));
        }
    }

    #[test]
    fn entlayout_module_snapshot_exports_loaded_k04_both_sides() {
        let definition: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/k04-vial-b4709ab9.json")).unwrap();
        let mut app = EntropyApp::new_inert_for_test();
        app.layout = Some(KeyboardLayout::from_vial_json(&definition).unwrap());
        app.supported_qmk_settings = vec![140, 141, 143, 324];
        app.module_settings =
            EntropyApp::module_settings_from_definition(&definition, &app.supported_qmk_settings);
        app.module_settings
            .values
            .extend([(140, 1), (141, 1), (143, 3)]);
        let json = serde_json::to_value(app.entlayout_snapshot().unwrap()).unwrap();
        let settings = json["data"]["portable_settings"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let qsids = settings
            .iter()
            .map(|s| s["qsid"].as_u64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            qsids,
            vec![140, 141, 143],
            "loaded module values must survive .entlayout export; unloaded 324 must not"
        );
        let bundle: EntLayoutFile = serde_json::from_value(json).unwrap();
        assert_eq!(bundle.data.portable_settings.len(), 3);
    }

    pub(super) fn module_app() -> EntropyApp {
        let definition: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/k04-vial-b4709ab9.json")).unwrap();
        let mut app = EntropyApp::new_inert_for_test();
        app.layout = Some(KeyboardLayout::from_vial_json(&definition).unwrap());
        app.layer_count = 1;
        app.layout.as_mut().unwrap().layers =
            vec![vec![0u16.into(); app.layout.as_ref().unwrap().keys.len()]];
        app.supported_qmk_settings = vec![140, 141, 143, 324];
        app.module_settings =
            EntropyApp::module_settings_from_definition(&definition, &app.supported_qmk_settings);
        app.module_settings
            .values
            .extend([(140, 0), (141, 0), (143, 0), (324, 0)]);
        app
    }

    #[test]
    fn entlayout_module_roundtrip_restores_both_sides_and_auto_layer() {
        let mut source = module_app();
        source
            .module_settings
            .values
            .extend([(140, 1), (141, 1), (143, 3), (324, 12)]);
        let json = serde_json::to_string(&source.entlayout_snapshot().unwrap()).unwrap();
        let bundle: EntLayoutFile = serde_json::from_str(&json).unwrap();
        let mut target = module_app();
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        target.hid_device = Some(hid);
        let (_, _, report) = target.apply_entlayout(&bundle).unwrap();
        assert_eq!(report.imported, 4);
        assert_eq!(target.module_settings.values, source.module_settings.values);
        let sets = recorder
            .requests()
            .into_iter()
            .filter(|r| r[..2] == [0xfe, 0x0b])
            .map(|r| r[..6].to_vec())
            .collect::<Vec<_>>();
        assert_eq!(
            sets,
            vec![
                vec![0xfe, 0x0b, 140, 0, 1, 0],
                vec![0xfe, 0x0b, 141, 0, 1, 0],
                vec![0xfe, 0x0b, 143, 0, 3, 0],
                vec![0xfe, 0x0b, 68, 1, 12, 0]
            ]
        );
        assert_eq!(
            target.layout.as_ref().unwrap().layers,
            source.layout.as_ref().unwrap().layers
        );
    }

    #[test]
    fn entlayout_module_failed_get_is_not_exported() {
        let definition: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/k04-vial-b4709ab9.json")).unwrap();
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        recorder.respond_by(|request| {
            (request[..2] == [0xfe, 0x0a]).then(|| Err("scripted GET failure".to_owned()))
        });
        let settings = EntropyApp::read_module_settings(&definition, &[140, 141], &hid);
        assert!(
            settings.values.is_empty(),
            "failed GET must remain unloaded, not become a fabricated zero"
        );
        let mut app = module_app();
        app.module_settings = settings;
        assert!(app.entlayout_portable_settings_snapshot().is_empty());
    }

    #[test]
    fn entlayout_module_rejects_duplicate_qsid_before_any_write() {
        let mut source = module_app();
        source.module_settings.values.insert(140, 1);
        let mut bundle = source.entlayout_snapshot().unwrap();
        bundle.data.portable_settings.retain(|s| s.qsid == 140);
        bundle
            .data
            .portable_settings
            .push(bundle.data.portable_settings[0].clone());
        let target = module_app();
        let plan = plan_entlayout_portable_settings_import(
            &bundle.data.portable_settings,
            &target.entlayout_portable_settings_snapshot(),
        );
        assert!(
            plan.compatible.is_empty(),
            "ambiguous duplicate QSID must never write even the first entry"
        );
        assert_eq!(plan.report.incompatible, 2);
    }

    #[test]
    fn entlayout_module_rejects_malformed_serialized_contract() {
        let source = module_app();
        let setting = source
            .entlayout_portable_settings_snapshot()
            .into_iter()
            .find(|s| s.qsid == 140)
            .unwrap();
        for (key, value) in [
            ("width", serde_json::json!(3)),
            ("bit", serde_json::json!(16)),
            ("max", serde_json::json!(300)),
            ("kind", serde_json::json!("unknown")),
        ] {
            let mut json = serde_json::to_value(&setting).unwrap();
            json["module"][0][key] = value;
            let malformed: EntPortableSetting = serde_json::from_value(json).unwrap();
            let malformed_settings = [malformed];
            let plan =
                plan_entlayout_portable_settings_import(&malformed_settings, &[setting.clone()]);
            assert!(plan.compatible.is_empty());
            assert_eq!(plan.report.incompatible, 1);
        }
    }

    fn portable_setting(
        id: &str,
        qsid: u16,
        width: EntPortableSettingWidth,
        value: u16,
    ) -> EntPortableSetting {
        EntPortableSetting {
            id: id.to_owned(),
            qsid,
            width,
            value,
            variants: Vec::new(),
            module: None,
        }
    }

    fn cosmetic_test_layout() -> KeyboardLayout {
        let mut layout: KeyboardLayout = serde_json::from_str(
            r#"{"name":"Test","rows":1,"cols":3,"keys":[{"x":0.0,"y":0.0,"w":1.0,"h":1.0,"row":0,"col":0,"label":"0,0","rotation":0.0,"rotation_x":0.0,"rotation_y":0.0},{"x":2.0,"y":0.0,"w":1.0,"h":1.0,"row":0,"col":1,"label":"0,1","rotation":0.0,"rotation_x":0.0,"rotation_y":0.0},{"x":4.0,"y":0.0,"w":1.0,"h":1.0,"row":0,"col":2,"label":"0,2","rotation":0.0,"rotation_x":0.0,"rotation_y":0.0}],"encoders":[{"x":6.0,"y":0.0,"w":1.0,"h":1.0,"label":"encoder","encoder_idx":0,"direction":0,"rotation":0.0,"rotation_x":0.0,"rotation_y":0.0}],"layers":[],"encoder_layers":[[6]],"custom_keycodes":[]}"#,
        ).unwrap();
        layout.layers = vec![vec![4u16.into(), 5u16.into(), 6u16.into()]];
        layout
    }

    fn offset_alternative_backup() -> (EntLayoutFile, KeyboardLayout) {
        let json = serde_json::json!({
            "matrix": {"rows": 1, "cols": 2},
            "layouts": {
                "labels": ["Rotated option"],
                "keymap": [
                    ["0,0", {"rx": 4, "ry": 2, "w": 2}, "0,1\n\n\n0,0",
                        "0,0\n\n\n0,0\n\n\n\n\n\ne"],
                    [{"r": 90, "rx": 10, "ry": 6, "w": 2}, "0,1\n\n\n0,1",
                        {"h": 3}, "0,1\n\n\n0,1\n\n\n\n\n\ne"]
                ]
            }
        });
        let mut target = KeyboardLayout::from_vial_json(&json).unwrap();
        target.layers = vec![vec![4.into(), 5.into(), 6.into()]];
        target.encoder_layers = vec![vec![7, 8]];
        let mut legacy = target.clone();
        // Coordinates exported by the old parser, before option alignment.
        let key = &mut legacy.keys[2];
        (key.x, key.y, key.rotation_x, key.rotation_y) = (10.0, 6.0, 10.0, 6.0);
        let encoder = &mut legacy.encoders[1];
        (encoder.x, encoder.y, encoder.rotation_x, encoder.rotation_y) = (12.0, 6.0, 10.0, 6.0);
        let mut app = EntropyApp::new_inert_for_test();
        app.layout = Some(legacy);
        app.layout_options_value = Some(1);
        let serialized = serde_json::to_string(&app.entlayout_snapshot().unwrap()).unwrap();
        (serde_json::from_str(&serialized).unwrap(), target)
    }

    #[test]
    fn offset_alternative_backups_keep_exact_key_encoder_and_option_restore() {
        let (bundle, target) = offset_alternative_backup();
        assert_ne!(bundle.keyboard.layout_hash, entlayout_hash(&target));
        assert_eq!(bundle.data.layout_options, Some(1));
        let mut app = EntropyApp::new_inert_for_test();
        app.layout = Some(target);
        app.layout_options_value = Some(0);
        let mapping = app.entlayout_import_mapping(&bundle).unwrap();
        assert!(mapping.exact_layout);
        assert_eq!(mapping.key_mapping, vec![Some(0), Some(1), Some(2)]);
        assert_eq!(mapping.encoder_mapping, vec![Some(0), Some(1)]);
        let report = app.entlayout_import_report(
            &bundle,
            Path::new("unused-backup.entlayout"),
            &EntFirmwareFailures::default(),
            &mapping,
            &EntPortableSettingsImportReport::default(),
        );
        assert!(!report.contains(crate::i18n::tr_catalog(
            app.app_settings.language,
            "entlayout.layout_options_exact_only"
        )));
    }

    #[test]
    fn an_import_with_a_failed_write_is_not_reported_complete() {
        let (bundle, target) = offset_alternative_backup();
        let mut app = EntropyApp::new_inert_for_test();
        app.layout = Some(target);
        let mapping = app.entlayout_import_mapping(&bundle).unwrap();
        let lang = app.app_settings.language;
        let report = |failures: &EntFirmwareFailures| {
            app.entlayout_import_report(
                &bundle,
                Path::new("unused-backup.entlayout"),
                failures,
                &mapping,
                &EntPortableSettingsImportReport::default(),
            )
        };
        let macros_failed = crate::i18n::tr_catalog_format(
            lang,
            "entlayout.summary_failed",
            &[("section", crate::i18n::tr_catalog(lang, "entlayout.macros"))],
        );

        let clean = report(&EntFirmwareFailures::default());
        assert!(clean.starts_with(crate::i18n::tr_catalog(lang, "entlayout.import_complete")));
        assert!(!clean.contains(&macros_failed), "{clean}");

        let mut failures = EntFirmwareFailures::default();
        failures.push_section("entlayout.macros", "macros (refused)".to_owned());
        let failed = report(&failures);
        assert!(
            failed.starts_with(crate::i18n::tr_catalog(
                lang,
                "entlayout.import_complete_with_errors"
            )),
            "{failed}"
        );
        // The macros the file holds are not counted as imported.
        assert!(failed.contains(&macros_failed), "{failed}");
        assert!(failed.contains("macros (refused)"), "{failed}");
    }

    #[test]
    fn the_import_report_skips_only_sections_with_entries() {
        let (mut bundle, target) = offset_alternative_backup();
        let mut app = EntropyApp::new_inert_for_test();
        app.layout = Some(target);
        let mapping = app.entlayout_import_mapping(&bundle).unwrap();
        let lang = app.app_settings.language;
        let not_available = |section| {
            crate::i18n::tr_catalog_format(
                lang,
                "entlayout.not_available",
                &[("section", crate::i18n::tr_catalog(lang, section))],
            )
        };
        let report = |bundle: &EntLayoutFile| {
            app.entlayout_import_report(
                bundle,
                Path::new("unused-backup.entlayout"),
                &EntFirmwareFailures::default(),
                &mapping,
                &EntPortableSettingsImportReport::default(),
            )
        };

        // The board has none of these sections, and the file none either.
        let empty = report(&bundle);
        for section in [
            "entlayout.macros",
            "entlayout.combos",
            "entlayout.tap_dance",
            "entlayout.key_overrides",
            "entlayout.alt_repeat",
        ] {
            assert!(!empty.contains(&not_available(section)), "{empty}");
        }
        // An entry the board cannot take is still reported.
        bundle.data.alt_repeat.entries.push(EntAltRepeatEntry {
            keycode: 4,
            alt_keycode: 5,
            allowed_mods: 0,
            options: 0,
        });
        assert!(report(&bundle).contains(&not_available("entlayout.alt_repeat")));
    }

    #[test]
    fn offset_alternative_backup_compatibility_rejects_other_layouts_and_missing_geometry() {
        let (bundle, target) = offset_alternative_backup();
        let mut app = EntropyApp::new_inert_for_test();
        for change in 0..8 {
            let mut other = target.clone();
            match change {
                0 => other.keys[0].x += 1.0,
                1 => other.keys[2].col = 0,
                2 => other.keys[2].w += 0.5,
                3 => other.keys[2].rotation += 10.0,
                4 => other.encoders[1].direction = 0,
                5 => other.encoders[1].h += 0.5,
                6 => other.layout_options[0].label = "Different option".into(),
                _ => other.keys.swap(0, 1),
            }
            app.layout = Some(other);
            assert!(
                !app.entlayout_import_mapping(&bundle).unwrap().exact_layout,
                "unrelated layout change {change} was accepted as exact"
            );
        }
        app.layout = Some(target);
        let mut missing = bundle.clone();
        missing.data.source_layout = None;
        assert!(!app.entlayout_import_mapping(&missing).unwrap().exact_layout);
        let mut inconsistent = bundle;
        inconsistent.data.source_layout.as_mut().unwrap().keys[2].x += 1.0;
        assert!(
            !app.entlayout_import_mapping(&inconsistent)
                .unwrap()
                .exact_layout
        );
    }

    #[test]
    fn offset_alternative_backups_keep_index_mapping_when_only_layer_count_changes() {
        let (bundle, mut target) = offset_alternative_backup();
        target.layers.push(vec![0.into(); target.keys.len()]);
        target.encoder_layers.push(vec![0; target.encoders.len()]);
        let mut app = EntropyApp::new_inert_for_test();
        app.layout = Some(target);
        let mapping = app.entlayout_import_mapping(&bundle).unwrap();
        assert!(!mapping.exact_layout);
        assert_eq!(mapping.key_mapping, vec![Some(0), Some(1), Some(2)]);
        assert_eq!(mapping.encoder_mapping, vec![Some(0), Some(1)]);
    }

    #[test]
    fn entlayout_export_carries_cosmetic_visibility_without_changing_keymap() {
        let layout = cosmetic_test_layout();
        let mut app = EntropyApp::new_inert_for_test();
        app.current_encoder_visibility_id = "portable_cosmetic_test".to_owned();
        app.layout = Some(layout);
        app.app_settings.layout_element_visibility.insert(
            "another_keyboard".to_owned(),
            LayoutElementVisibility {
                hidden_keys: [(0, 2)].into_iter().collect(),
                hidden_encoders: Default::default(),
            },
        );
        let visible = app.entlayout_snapshot().unwrap();
        assert_eq!(
            visible.data.layout_element_visibility,
            Some(LayoutElementVisibility::default())
        );
        app.app_settings
            .layout_element_visibility
            .entry(app.current_encoder_visibility_id.clone())
            .or_default()
            .hidden_keys
            .insert((0, 1));
        app.app_settings
            .layout_element_visibility
            .get_mut(&app.current_encoder_visibility_id)
            .unwrap()
            .hidden_encoders
            .insert(0);
        let bundle = app.entlayout_snapshot().unwrap();
        let json = serde_json::to_string(&bundle).unwrap();
        let restored: EntLayoutFile = serde_json::from_str(&json).unwrap();
        let saved = restored.data.layout_element_visibility.unwrap();
        assert!(saved.hidden_keys.contains(&(0, 1)));
        assert!(!saved.hidden_keys.contains(&(0, 2)));
        assert!(saved.hidden_encoders.contains(&0));
        assert_eq!(restored.data.keymap[0], vec![4, 5, 6]);
        assert_eq!(restored.data.encoder_keymap[0], vec![6]);
    }

    #[test]
    fn exact_cosmetic_import_replaces_only_known_target_positions() {
        let layout = cosmetic_test_layout();
        let source = LayoutElementVisibility {
            hidden_keys: [(0, 1), (7, 7)].into_iter().collect(),
            hidden_encoders: [0, 8].into_iter().collect(),
        };
        let current = LayoutElementVisibility {
            hidden_keys: [(0, 0)].into_iter().collect(),
            hidden_encoders: [9].into_iter().collect(),
        };
        let mapping = EntLayoutImportMapping {
            exact_layout: true,
            key_mapping: vec![Some(0), Some(1), Some(2)],
            encoder_mapping: vec![Some(0)],
        };
        let result = map_entlayout_element_visibility(&source, None, &mapping, &layout, &current);
        assert_eq!(result.hidden_keys, [(0, 1)].into_iter().collect());
        assert_eq!(result.hidden_encoders, [0].into_iter().collect());
        assert_eq!(
            map_entlayout_element_visibility(
                &LayoutElementVisibility::default(),
                None,
                &mapping,
                &layout,
                &current
            ),
            LayoutElementVisibility::default()
        );
    }

    #[test]
    fn universal_cosmetic_import_remaps_groups_and_preserves_unmapped_positions() {
        let source_layout = cosmetic_test_layout();
        let source = entlayout_source_layout(&source_layout);
        let mut target = source_layout.clone();
        target.keys[0].row = 1;
        target.keys[0].col = 0;
        target.keys[1].row = 1;
        target.keys[1].col = 1;
        target.keys[2].row = 1;
        target.keys[2].col = 2;
        target.encoders[0].encoder_idx = 5;
        let current = LayoutElementVisibility {
            hidden_keys: [(1, 0), (1, 2)].into_iter().collect(),
            hidden_encoders: [5, 9].into_iter().collect(),
        };
        let saved = LayoutElementVisibility {
            hidden_keys: [(0, 0)].into_iter().collect(),
            hidden_encoders: [0].into_iter().collect(),
        };
        let mapping = EntLayoutImportMapping {
            exact_layout: false,
            key_mapping: vec![Some(1), Some(0), None],
            encoder_mapping: vec![Some(0)],
        };
        let result =
            map_entlayout_element_visibility(&saved, Some(&source), &mapping, &target, &current);
        assert_eq!(result.hidden_keys, [(1, 1), (1, 2)].into_iter().collect());
        assert_eq!(result.hidden_encoders, [5, 9].into_iter().collect());
        assert_eq!(
            map_entlayout_element_visibility(&saved, None, &mapping, &target, &current),
            current
        );
    }

    #[test]
    fn universal_encoder_visibility_maps_entire_group_or_preserves_existing_choice() {
        let mut source_layout = cosmetic_test_layout();
        let mut clockwise = source_layout.encoders[0].clone();
        clockwise.direction = 1;
        source_layout.encoders.push(clockwise);
        let source = entlayout_source_layout(&source_layout);
        let mut target = source_layout.clone();
        for encoder in &mut target.encoders {
            encoder.encoder_idx = 5;
        }
        let saved = LayoutElementVisibility {
            hidden_keys: Default::default(),
            hidden_encoders: [0].into_iter().collect(),
        };
        let current = LayoutElementVisibility::default();
        let mut mapping = EntLayoutImportMapping {
            exact_layout: false,
            key_mapping: vec![Some(0), Some(1), Some(2)],
            encoder_mapping: vec![Some(0), Some(1)],
        };
        let mapped =
            map_entlayout_element_visibility(&saved, Some(&source), &mapping, &target, &current);
        assert_eq!(mapped.hidden_encoders, [5].into_iter().collect());

        mapping.encoder_mapping[1] = None;
        let partial =
            map_entlayout_element_visibility(&saved, Some(&source), &mapping, &target, &current);
        assert!(partial.hidden_encoders.is_empty());
        mapping.encoder_mapping[1] = Some(0);
        let duplicate =
            map_entlayout_element_visibility(&saved, Some(&source), &mapping, &target, &current);
        assert!(duplicate.hidden_encoders.is_empty());
        mapping.encoder_mapping[1] = Some(1);
        target.encoders[1].encoder_idx = 6;
        let split =
            map_entlayout_element_visibility(&saved, Some(&source), &mapping, &target, &current);
        assert!(split.hidden_encoders.is_empty());
    }

    #[test]
    fn legacy_entlayout_data_defaults_portable_settings() {
        let data: EntLayoutData = serde_json::from_value(serde_json::json!({
            "keymap": [[0]],
            "encoder_keymap": [[]],
            "encoder_visibility": [],
            "layout_options": null,
            "layer_names": ["0"],
            "text_expander": {
                "enabled": false,
                "app_blacklist": "",
                "rule_files": [],
                "primary_rules": [],
                "extra_files": []
            },
            "macros": {
                "texts": [],
                "names": [],
                "descriptions": []
            },
            "combos": {
                "entries": [],
                "names": [],
                "term": null
            },
            "tap_dance": {
                "entries": [],
                "names": []
            },
            "key_overrides": {
                "entries": [],
                "names": []
            },
            "alt_repeat": {
                "entries": [],
                "names": []
            }
        }))
        .expect("legacy layout data");

        assert!(data.portable_settings.is_empty());
        assert!(data.layout_element_visibility.is_none());
    }

    #[test]
    fn portable_setting_registry_has_unique_qsids_and_excludes_existing_combo_term() {
        let qsids = ENT_PORTABLE_QSIDS
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(qsids.len(), ENT_PORTABLE_QSIDS.len());
        assert!(!qsids.contains(&2));
        assert_eq!(
            ent_portable_setting_spec(9).unwrap().width,
            EntPortableSettingWidth::U8
        );
        assert_eq!(ent_portable_setting_spec(21).unwrap().max, 0x03ff);
    }

    #[test]
    fn portable_setting_import_plan_skips_unsupported_incompatible_and_duplicate_values() {
        let settings = vec![
            portable_setting("tapping_term", 7, EntPortableSettingWidth::U16, 200),
            portable_setting("auto_shift_flags", 3, EntPortableSettingWidth::U8, 1),
            portable_setting("quick_tap_term", 25, EntPortableSettingWidth::U16, 1001),
            portable_setting("unknown", 999, EntPortableSettingWidth::U16, 1),
            portable_setting("tapping_term", 7, EntPortableSettingWidth::U16, 250),
        ];

        let target_settings = vec![
            portable_setting("tapping_term", 7, EntPortableSettingWidth::U16, 180),
            portable_setting("quick_tap_term", 25, EntPortableSettingWidth::U16, 200),
        ];
        let plan = plan_entlayout_portable_settings_import(&settings, &target_settings);

        assert_eq!(plan.compatible.len(), 1);
        assert_eq!(plan.compatible[0].qsid, 7);
        assert_eq!(
            plan.report,
            EntPortableSettingsImportReport {
                imported: 0,
                unsupported: 1,
                incompatible: 3,
                failed: 0,
            }
        );
    }

    #[test]
    fn current_version_is_v7_and_v6_files_remain_supported() {
        assert_eq!(ENTLAYOUT_VERSION, 7);
        assert!(entlayout_version_supported(4));
        assert!(entlayout_version_supported(5));
        assert!(entlayout_version_supported(6));
        assert!(entlayout_version_supported(7));
        assert!(!entlayout_version_supported(0));
        assert!(!entlayout_version_supported(8));
    }

    #[test]
    fn portable_setting_import_plan_skips_unchanged_firmware_writes() {
        let source = portable_setting("tapping_term", 7, EntPortableSettingWidth::U16, 200);
        let target = source.clone();
        let sources = [source];
        let targets = [target];

        let plan = plan_entlayout_portable_settings_import(&sources, &targets);

        assert!(plan.compatible.is_empty());
        assert_eq!(plan.report.imported, 1);
        assert_eq!(plan.report.total(), 1);
    }

    #[test]
    fn portable_touchpad_select_requires_the_same_ordered_variants() {
        let source = EntPortableSetting {
            id: "touchpad_auto_layer".to_owned(),
            qsid: 143,
            width: EntPortableSettingWidth::U8,
            value: 1,
            variants: vec!["Layer 1".to_owned(), "Layer 2".to_owned()],
            module: None,
        };
        let mut compatible_target = source.clone();
        compatible_target.value = 0;
        let mut incompatible_target = compatible_target.clone();
        incompatible_target.variants.reverse();

        let compatible = plan_entlayout_portable_settings_import(
            std::slice::from_ref(&source),
            &[compatible_target],
        );
        assert_eq!(compatible.compatible.len(), 1);

        let incompatible = plan_entlayout_portable_settings_import(
            std::slice::from_ref(&source),
            &[incompatible_target],
        );
        assert!(incompatible.compatible.is_empty());
        assert_eq!(incompatible.report.incompatible, 1);
    }

    #[test]
    fn portable_setting_report_distinguishes_imported_and_skipped_results() {
        let mut report = EntPortableSettingsImportReport::default();
        report.record_imported();
        report.record_unsupported();
        report.record_failed();

        let summary = report.summary(crate::i18n::Language::English);

        assert!(summary.contains("1/3 imported"));
        assert!(summary.contains("1 unsupported by target"));
        assert!(summary.contains("1 failed"));
    }

    #[test]
    fn entmacro_bytecode_prefers_base64() {
        let bytes = vec![0x01, 0x05, 0xA0, 0xFF];
        let macros = EntMacroData {
            bytecode_base64: vec![BASE64_STANDARD.encode(&bytes)],
            texts: vec!["legacy".to_owned()],
            names: Vec::new(),
            descriptions: Vec::new(),
        };

        assert_eq!(entmacro_bytecode(&macros).unwrap(), vec![bytes]);
    }

    #[test]
    fn entmacro_bytecode_reads_legacy_texts() {
        let macros = EntMacroData {
            bytecode_base64: Vec::new(),
            texts: vec!["abc".to_owned()],
            names: Vec::new(),
            descriptions: Vec::new(),
        };

        assert_eq!(entmacro_bytecode(&macros).unwrap(), vec![b"abc".to_vec()]);
    }

    #[test]
    fn entmacro_bytecode_rejects_invalid_base64() {
        let macros = EntMacroData {
            bytecode_base64: vec!["not base64".to_owned()],
            texts: Vec::new(),
            names: Vec::new(),
            descriptions: Vec::new(),
        };

        assert!(entmacro_bytecode(&macros).is_err());
    }

    #[test]
    fn trailing_non_empty_strings_keeps_indexed_metadata_sparse() {
        let values = vec![
            String::new(),
            "first".to_owned(),
            String::new(),
            "last".to_owned(),
            String::new(),
        ];

        assert_eq!(
            trailing_non_empty_strings(&values),
            vec![
                String::new(),
                "first".to_owned(),
                String::new(),
                "last".to_owned(),
            ]
        );
    }

    #[test]
    fn native_key_action_round_trips_through_entlayout_encoding() {
        use rmk_types::action::{Action, KeyAction};
        use rmk_types::keycode::HidKeyCode;
        use rmk_types::modifier::ModifierCombination;

        let action = KeyAction::TapHold(
            Action::KeyWithModifier(HidKeyCode::Kc0, ModifierCombination::LSHIFT),
            Action::Modifier(ModifierCombination::LCTRL),
            Default::default(),
        );
        let encoded = encode_native_key_action(action).expect("native action should fit");

        assert_eq!(decode_native_key_action(&encoded).unwrap(), action);
    }

    #[test]
    fn native_dynamic_binding_round_trips_without_losing_legacy_compatibility() {
        let binding =
            crate::universal_symbols::binding(crate::universal_symbols::USER_SYMBOL_START);
        let encoded = binding
            .rmk_action()
            .and_then(encode_native_key_action)
            .expect("universal symbol should fit");

        assert_eq!(
            entlayout_dynamic_binding(0, Some(&encoded)).unwrap(),
            binding
        );
        assert_eq!(
            entlayout_dynamic_binding(0x0004, None).unwrap(),
            crate::keyboard::KeyBinding::Vial(0x0004)
        );

        let legacy_combo: EntComboEntry = serde_json::from_value(serde_json::json!({
            "keys": [4, 5, 0, 0],
            "output": 6
        }))
        .unwrap();
        assert!(legacy_combo.native_output.is_none());
        assert!(legacy_combo.layer.is_none());

        let layered_combo = EntComboEntry {
            keys: [4, 5, 0, 0],
            output: 6,
            native_output: None,
            layer: Some(2),
        };
        let encoded_combo = serde_json::to_value(&layered_combo).unwrap();
        assert_eq!(encoded_combo["layer"], 2);
        assert_eq!(
            serde_json::from_value::<EntComboEntry>(encoded_combo)
                .unwrap()
                .layer,
            Some(2)
        );

        let legacy_tap_dance: EntTapDanceEntry = serde_json::from_value(serde_json::json!({
            "on_tap": 4,
            "on_hold": 0,
            "on_double_tap": 0,
            "on_tap_hold": 0,
            "tapping_term": 200
        }))
        .unwrap();
        assert!(legacy_tap_dance.native_on_tap.is_none());
    }
}
