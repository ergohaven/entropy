use super::portable_settings::{
    ent_portable_setting_contracts_match, ent_portable_setting_is_known,
    ent_portable_setting_is_valid, ent_portable_setting_spec, write_entlayout_portable_setting,
    EntPortableSetting, EntPortableSettingWidth, ENT_PORTABLE_QSIDS,
};
use super::*;
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const ENTLAYOUT_FORMAT: &str = "entropy.layout";
const ENTLAYOUT_VERSION: u16 = 7;

fn entlayout_version_supported(version: u16) -> bool {
    (1..=ENTLAYOUT_VERSION).contains(&version)
}

#[derive(Clone, Serialize, Deserialize)]
struct EntLayoutFile {
    format: String,
    version: u16,
    created_by: String,
    created_at: String,
    keyboard: EntLayoutKeyboard,
    data: EntLayoutData,
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

struct EntPortableSettingsImportPlan<'a> {
    compatible: Vec<&'a EntPortableSetting>,
    report: EntPortableSettingsImportReport,
}

fn plan_entlayout_portable_settings_import<'a>(
    settings: &'a [EntPortableSetting],
    target_settings: &[EntPortableSetting],
) -> EntPortableSettingsImportPlan<'a> {
    let mut compatible = Vec::new();
    let mut report = EntPortableSettingsImportReport::default();
    let mut seen = std::collections::BTreeSet::new();

    for setting in settings {
        if !ent_portable_setting_is_known(setting.qsid) {
            report.record_incompatible();
            continue;
        }
        if !seen.insert(setting.qsid) || !ent_portable_setting_is_valid(setting) {
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
        if setting.value == target.value {
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

#[derive(Clone, Serialize, Deserialize)]
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
        let Some(bundle) = self.entlayout_snapshot() else {
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "entlayout.connect_keyboard_before_exporting_layout",
            )
            .into();
            return;
        };
        match write_entlayout_file(path, &bundle, self.app_settings.language) {
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
        let bundle: EntLayoutFile = serde_json::from_str(&data).context(
            crate::i18n::tr_catalog(lang, "entlayout.invalid_entlayout_json"),
        )?;
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

    fn entlayout_snapshot(&self) -> Option<EntLayoutFile> {
        let layout = self.layout.as_ref()?;
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
            version: ENTLAYOUT_VERSION,
            created_by: "Entropy".to_owned(),
            created_at: chrono::Local::now().to_rfc3339(),
            keyboard,
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
                layout_options: self.layout_options_value,
                layer_names: self.layer_names.clone(),
                text_expander: self.text_expander_entlayout_snapshot(),
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
        let dir = dirs::config_dir()
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
        if !entlayout_version_supported(bundle.version) {
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
        Ok(())
    }

    fn entlayout_import_mapping(&self, bundle: &EntLayoutFile) -> Result<EntLayoutImportMapping> {
        let layout = self.layout.as_ref().context(crate::i18n::tr_catalog(
            self.app_settings.language,
            "entlayout.connect_target_keyboard",
        ))?;
        let exact_layout = bundle.keyboard.layout_hash == entlayout_hash(layout)
            && bundle.keyboard.layers == layout.layers.len()
            && bundle_source_key_count(bundle) == layout.keys.len()
            && bundle_source_encoder_count(bundle) == layout.encoders.len();

        if exact_layout {
            return Ok(EntLayoutImportMapping {
                exact_layout: true,
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
        Vec<String>,
        EntLayoutImportMapping,
        EntPortableSettingsImportReport,
    )> {
        let mapping = self.entlayout_import_mapping(bundle)?;
        let (mut firmware_failures, portable_settings) =
            self.apply_entlayout_firmware_state(bundle, &mapping)?;
        self.apply_entlayout_local_state(bundle, &mapping)?;
        self.refresh_layer_picker_content_flags();

        // Persist the imported layer names into firmware now that self.layer_names
        // holds the final target values. The shared writer continues past a
        // failing layer and reports which ones failed, so stale firmware slots
        // aren't left to win over the local names on the next reconnect.
        #[cfg(not(target_arch = "wasm32"))]
        {
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

    fn entlayout_dynamic_import_report(&self, bundle: &EntLayoutFile) -> String {
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
        [
            macro_stats.summary(lang, "entlayout.macros"),
            combo_stats.summary(lang, "entlayout.combos"),
            tap_dance_stats.summary(lang, "entlayout.tap_dance"),
            key_override_stats.summary(lang, "entlayout.key_overrides"),
            alt_repeat_stats.summary(lang, "entlayout.alt_repeat"),
        ]
        .join("; ")
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn entlayout_import_report(
        &self,
        bundle: &EntLayoutFile,
        backup_path: &Path,
        firmware_failures: &[String],
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
            tr("entlayout.text_expander"),
        ];
        let mut skipped = Vec::new();

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
            if entmacro_count(&bundle.data.macros) > 0 && self.keycode_picker.macro_count > 0 {
                imported.push(tr("entlayout.macros"));
            } else {
                skipped.push(crate::i18n::tr_catalog_format(
                    lang,
                    "entlayout.not_available",
                    &[("section", crate::i18n::tr_catalog(lang, "entlayout.macros"))],
                ));
            }

            if !bundle.data.combos.entries.is_empty() && !self.combo_entries.is_empty() {
                imported.push(tr("entlayout.combos"));
            } else {
                skipped.push(crate::i18n::tr_catalog_format(
                    lang,
                    "entlayout.not_available",
                    &[("section", crate::i18n::tr_catalog(lang, "entlayout.combos"))],
                ));
            }

            if !bundle.data.tap_dance.entries.is_empty()
                && !self.keycode_picker.tap_dance_entries.is_empty()
            {
                imported.push(tr("entlayout.tap_dance"));
            } else {
                skipped.push(crate::i18n::tr_catalog_format(
                    lang,
                    "entlayout.not_available",
                    &[(
                        "section",
                        crate::i18n::tr_catalog(lang, "entlayout.tap_dance"),
                    )],
                ));
            }

            if !bundle.data.key_overrides.entries.is_empty()
                && !self.key_override_entries.is_empty()
            {
                imported.push(tr("entlayout.key_overrides"));
            } else {
                skipped.push(crate::i18n::tr_catalog_format(
                    lang,
                    "entlayout.not_available",
                    &[(
                        "section",
                        crate::i18n::tr_catalog(lang, "entlayout.key_overrides"),
                    )],
                ));
            }

            if !bundle.data.alt_repeat.entries.is_empty() && !self.alt_repeat_entries.is_empty() {
                imported.push(tr("entlayout.alt_repeat"));
            } else {
                skipped.push(crate::i18n::tr_catalog_format(
                    lang,
                    "entlayout.not_available",
                    &[(
                        "section",
                        crate::i18n::tr_catalog(lang, "entlayout.alt_repeat"),
                    )],
                ));
            }
        }

        let none = crate::i18n::tr_catalog(lang, "entlayout.none");
        let skipped = if skipped.is_empty() {
            none.to_owned()
        } else {
            skipped.join(", ")
        };
        let firmware_failures = if firmware_failures.is_empty() {
            none.to_owned()
        } else {
            firmware_failures.join(", ")
        };
        let mode = if mapping.exact_layout {
            crate::i18n::tr_catalog(lang, "entlayout.exact")
        } else {
            crate::i18n::tr_catalog(lang, "entlayout.universal")
        };
        let dynamic_report = self.entlayout_dynamic_import_report(bundle);
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
            complete = crate::i18n::tr_catalog(lang, "entlayout.import_complete"),
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
    ) -> Result<(Vec<String>, EntPortableSettingsImportReport)> {
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
        let mut failures = Vec::new();
        let target_portable_settings = self.entlayout_portable_settings_snapshot();
        let EntPortableSettingsImportPlan {
            compatible: portable_settings,
            report: mut portable_settings_report,
        } = plan_entlayout_portable_settings_import(
            &bundle.data.portable_settings,
            &target_portable_settings,
        );
        let mut verified_portable_settings = Vec::new();

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
            let result = write_entlayout_portable_setting(hid, setting);
            match result {
                Ok(()) => {
                    portable_settings_report.record_imported();
                    verified_portable_settings.push(setting.clone());
                }
                Err(error) => {
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
                let buf = crate::hid::HidDevice::encode_macros(&macro_texts, size);
                hid.set_macro_buffer(&buf)?;
                Ok(())
            })() {
                failures.push(format!("macros ({err})"));
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
            failures.push(format!("combos ({err})"));
        }
        if let Some(term) = bundle.data.combos.term {
            if self.combo_term.is_some() && self.combo_term != Some(term) {
                if let Err(err) = hid.set_qmk_setting_u16(2, term) {
                    failures.push(format!("combo term ({err})"));
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
            failures.push(format!("tap dance ({err})"));
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
            failures.push(format!("key overrides ({err})"));
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
            failures.push(format!("alt repeat ({err})"));
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

        self.app_settings.text_expander_enabled = bundle.data.text_expander.enabled;
        self.app_settings.text_expander_app_blacklist =
            bundle.data.text_expander.app_blacklist.clone();
        self.app_settings.text_expander_rule_files =
            normalize_text_expander_rule_files(&bundle.data.text_expander.rule_files);
        self.app_settings.text_expansion_rules = bundle.data.text_expander.primary_rules.clone();
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
        save_macro_metadata(
            &self.keycode_picker.macro_names,
            &self.keycode_picker.macro_descriptions,
            &self.current_device_name,
        );
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
        save_combo_names(&self.combo_names, &self.current_device_name);
        save_combo_colors(&self.combo_colors, &self.current_device_name);
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
        save_tap_dance_names(
            &self.keycode_picker.tap_dance_names,
            &self.current_device_name,
        );
        self.keycode_picker.tap_dance_dirty = false;

        self.key_override_names = normalized_strings(
            &bundle.data.key_overrides.names,
            self.key_override_entries.len(),
        );
        save_key_override_names(&self.key_override_names, &self.current_device_name);

        self.alt_repeat_names =
            normalized_strings(&bundle.data.alt_repeat.names, self.alt_repeat_entries.len());
        save_alt_repeat_names(&self.alt_repeat_names, &self.current_device_name);

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
    bundle: &EntLayoutFile,
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
mod tests {
    use super::*;

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
        }
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
