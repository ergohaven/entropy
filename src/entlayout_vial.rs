//! Import the version 1 JSON backup written by Vial GUI for protocol 6.
use super::*;
use crate::keycode_picker::{encode_macro_actions, MacroAction};
use anyhow::ensure;
use std::collections::BTreeMap;

const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Deserialize)]
struct VialBackup {
    version: u16,
    uid: u64,
    vial_protocol: u32,
    via_protocol: u16,
    layout: Vec<Vec<Vec<VialKeycode>>>,
    encoder_layout: Vec<Vec<[VialKeycode; 2]>>,
    layout_options: i64,
    #[serde(default, rename = "macro")]
    macros: Vec<Vec<Vec<serde_json::Value>>>,
    #[serde(default)]
    combo: Vec<[VialKeycode; 5]>,
    #[serde(default)]
    tap_dance: Vec<(VialKeycode, VialKeycode, VialKeycode, VialKeycode, u16)>,
    #[serde(default)]
    key_override: Vec<VialOverride>,
    #[serde(default)]
    alt_repeat_key: Vec<VialAltRepeat>,
    #[serde(default)]
    settings: BTreeMap<u16, u32>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum VialKeycode {
    Number(i32),
    Name(String),
}

impl VialKeycode {
    fn missing(&self) -> bool {
        matches!(self, Self::Number(-1))
    }

    fn resolve(&self) -> Result<u16> {
        match self {
            Self::Number(value) => {
                u16::try_from(*value).with_context(|| format!("Invalid Vial keycode {value}"))
            }
            Self::Name(name) => {
                resolve_name(name).with_context(|| format!("Unsupported Vial keycode {name:?}"))
            }
        }
    }
}

fn resolve_name(name: &str) -> Option<u16> {
    if let Some(hex) = name.strip_prefix("0x") {
        return u16::from_str_radix(hex, 16).ok();
    }
    // Some exact names contain parentheses, including the one-shot modifiers.
    if let Some(code) = vial_name_override(name) {
        return Some(code);
    }
    // Vial serializes generated slots and masked actions with their QMK names.
    if let Some(index) = name.strip_prefix('M').and_then(|n| n.parse::<u8>().ok()) {
        return Some(0x7700 + u16::from(index));
    }
    if let Some(index) = name.strip_prefix("USER").and_then(|n| n.parse::<u8>().ok()) {
        return (index < 64).then(|| 0x7e00 + u16::from(index));
    }
    if let Some((outer, inner)) = name.split_once('(') {
        let inner = inner.strip_suffix(')')?;
        let range = match outer {
            "TD" => Some((0x5700, 255)),
            "TO" => Some((0x5200, 31)),
            "MO" => Some((0x5220, 31)),
            "DF" => Some((0x5240, 31)),
            "TG" => Some((0x5260, 31)),
            "OSL" => Some((0x5280, 31)),
            "TT" => Some((0x52c0, 31)),
            "PDF" => Some((0x52e0, 31)),
            _ => None,
        };
        if let Some((base, max)) = range {
            let index = inner.parse::<u16>().ok()?;
            return (index <= max).then(|| base + index);
        }
        // Vial serializes at most one masked prefix and a basic keycode.
        if inner.contains('(') {
            return None;
        }
        let mask = if let Some(layer) = outer.strip_prefix("LT") {
            let layer = layer.parse::<u16>().ok()?;
            (layer < 16).then(|| 0x4000 | (layer << 8))?
        } else {
            vial_name_override(&format!("{outer}(kc)"))?
        };
        let key = resolve_name(inner)?;
        return (key <= 0xff).then(|| mask | key);
    }
    crate::keycode::KEYCODES
        .iter()
        .find(|key| key.name == name)
        .map(|key| key.value)
}

fn vial_name_override(name: &str) -> Option<u16> {
    include_str!("vial_v6_keycodes.tsv")
        .lines()
        .find_map(|line| {
            let (id, hex) = line.split_once('\t')?;
            (id == name).then(|| u16::from_str_radix(hex, 16).expect("Vial keycode table"))
        })
}

#[derive(Deserialize)]
struct VialOverride {
    trigger: VialKeycode,
    replacement: VialKeycode,
    layers: u16,
    trigger_mods: u8,
    negative_mod_mask: u8,
    suppressed_mods: u8,
    options: u8,
}

#[derive(Deserialize)]
struct VialAltRepeat {
    keycode: VialKeycode,
    alt_keycode: VialKeycode,
    allowed_mods: u8,
    options: u8,
}

fn encode_macro(rows: &[Vec<serde_json::Value>], ext_supported: bool) -> Result<Vec<u8>> {
    let mut actions = Vec::new();
    for row in rows {
        let (tag, args) = row.split_first().context("Missing Vial macro action")?;
        match (tag.as_str(), args) {
            (Some("text"), [value]) => {
                let text = value.as_str().context("Invalid Vial macro text")?;
                ensure!(
                    !text.as_bytes().iter().any(|&byte| byte < 2),
                    "Vial macro text contains a reserved byte"
                );
                actions.push(MacroAction::Text(text.to_owned()));
            }
            (Some("delay"), [value]) => {
                let ms = value
                    .as_u64()
                    .filter(|ms| *ms < 255 * 255)
                    .context("Vial macro delay is outside the supported range")?;
                actions.push(MacroAction::Delay(ms as u16));
            }
            (Some(tag @ ("tap" | "down" | "up")), keys) => {
                for key in keys {
                    let key: VialKeycode = VialKeycode::deserialize(key)?;
                    let key = key.resolve()?;
                    ensure!(
                        key != 0 && key <= 0xff00 && (key <= 0xff || ext_supported),
                        "Unsupported Vial macro keycode {key:#06x}"
                    );
                    actions.push(match tag {
                        "tap" => MacroAction::Tap(key),
                        "down" => MacroAction::Down(key),
                        _ => MacroAction::Up(key),
                    });
                }
            }
            _ => bail!("Unsupported or malformed Vial macro action {tag}"),
        }
    }
    Ok(encode_macro_actions(&actions))
}

fn check_target_count(section: &str, entries: usize, capacity: usize) -> Result<()> {
    ensure!(
        entries <= capacity,
        "Vial backup has {entries} {section} slots, but this keyboard has {capacity}"
    );
    Ok(())
}

impl VialBackup {
    fn validate_format(&self) -> Result<()> {
        ensure!(
            self.version == 1,
            "Unsupported Vial backup version {}",
            self.version
        );
        ensure!(
            self.vial_protocol == 6,
            "Unsupported Vial backup protocol {}",
            self.vial_protocol
        );
        Ok(())
    }

    fn keymap(&self, layout: &KeyboardLayout, layer_count: usize) -> Result<Vec<Vec<u16>>> {
        ensure!(
            self.layout.len() == layer_count,
            "Vial backup layer count does not match this keyboard"
        );
        let positions: std::collections::BTreeSet<_> = layout
            .keys
            .iter()
            .map(|key| (key.row as usize, key.col as usize))
            .collect();
        self.layout
            .iter()
            .enumerate()
            .map(|(index, layer)| {
                ensure!(
                    layer.len() == layout.rows && layer.iter().all(|row| row.len() == layout.cols),
                    "Vial backup matrix dimensions differ on layer {index}"
                );
                for (row, cells) in layer.iter().enumerate() {
                    for (col, code) in cells.iter().enumerate() {
                        ensure!(
                            !code.missing() == positions.contains(&(row, col)),
                            "Vial backup matrix differs at layer {index}, row {row}, column {col}"
                        );
                    }
                }
                layout
                    .keys
                    .iter()
                    .map(|key| {
                        layer
                            .get(key.row as usize)
                            .and_then(|row| row.get(key.col as usize))
                            .context(
                                "Keyboard definition contains an out-of-range matrix position",
                            )?
                            .resolve()
                            .with_context(|| {
                                format!("Layer {index}, matrix {},{}", key.row, key.col)
                            })
                    })
                    .collect()
            })
            .collect()
    }

    fn encoders(&self, layout: &KeyboardLayout, layer_count: usize) -> Result<Vec<Vec<u16>>> {
        ensure!(
            self.encoder_layout.len() == layer_count,
            "Vial encoder layer count differs"
        );
        let count = layout
            .encoders
            .iter()
            .map(|key| usize::from(key.encoder_idx) + 1)
            .max()
            .unwrap_or(0);
        self.encoder_layout
            .iter()
            .enumerate()
            .map(|(layer, encoders)| {
                ensure!(
                    encoders.len() == count,
                    "Vial backup encoder count differs on layer {layer}"
                );
                for (index, pair) in encoders.iter().enumerate() {
                    let present = layout
                        .encoders
                        .iter()
                        .any(|key| usize::from(key.encoder_idx) == index);
                    ensure!(
                        pair.iter().all(|code| code.missing() != present),
                        "Vial encoder {index} is missing or unexpected on layer {layer}"
                    );
                }
                layout
                    .encoders
                    .iter()
                    .map(|key| {
                        encoders[usize::from(key.encoder_idx)]
                            .get(usize::from(key.direction))
                            .context("Invalid encoder direction in keyboard definition")?
                            .resolve()
                            .with_context(|| format!("Layer {layer}, encoder {}", key.encoder_idx))
                    })
                    .collect()
            })
            .collect()
    }

    fn convert(&self, app: &EntropyApp) -> Result<EntLayoutFile> {
        self.validate_format()?;
        let layout = app
            .layout
            .as_ref()
            .context("Connect the original Vial keyboard first")?;
        let info = app
            .device_about_info
            .as_ref()
            .context("Keyboard protocols are unconfirmed")?;
        ensure!(
            app.firmware == FirmwareProtocol::Vial
                && !app.current_device_is_likely_rmk()
                && app.application_layout_device_key().is_none(),
            "Vial backups require a Vial-QMK device with a standard layout"
        );
        ensure!(
            self.uid != 0 && self.uid != u64::MAX && app.current_keyboard_id == Some(self.uid),
            "Vial backup keyboard UID {:016X} does not match the connected keyboard",
            self.uid
        );
        ensure!(
            (self.vial_protocol, self.via_protocol) == (info.vial_protocol, info.via_protocol),
            "Vial backup and keyboard protocol versions differ"
        );
        check_target_count("macro", self.macros.len(), app.keycode_picker.macro_count)?;
        check_target_count("Combo", self.combo.len(), app.combo_entries.len())?;
        check_target_count(
            "Tap Dance",
            self.tap_dance.len(),
            app.keycode_picker.tap_dance_entries.len(),
        )?;
        check_target_count(
            "Key Override",
            self.key_override.len(),
            app.key_override_entries.len(),
        )?;
        check_target_count(
            "Alt Repeat",
            self.alt_repeat_key.len(),
            app.alt_repeat_entries.len(),
        )?;
        ensure!(
            self.macros.is_empty() || self.macros.len() == app.keycode_picker.macro_count,
            "Vial backup macro slot count must match the keyboard's to preserve the other macros"
        );

        let keymap = self.keymap(layout, app.layer_count)?;
        let encoder_keymap = self.encoders(layout, app.layer_count)?;
        let bytecode_base64 = self
            .macros
            .iter()
            .enumerate()
            .map(|(slot, rows)| {
                encode_macro(rows, info.supports_macro_ext_keycodes)
                    .with_context(|| format!("Vial macro {slot}"))
                    .map(|bytes| BASE64_STANDARD.encode(bytes))
            })
            .collect::<Result<_>>()?;
        let combos = self
            .combo
            .iter()
            .map(|[a, b, c, d, output]| {
                Ok(EntComboEntry {
                    keys: [a.resolve()?, b.resolve()?, c.resolve()?, d.resolve()?],
                    output: output.resolve()?,
                    native_output: None,
                    rynk_output: None,
                    layer: None,
                })
            })
            .collect::<Result<_>>()?;
        let tap_dance = self
            .tap_dance
            .iter()
            .map(|(a, b, c, d, term)| {
                Ok(EntTapDanceEntry {
                    on_tap: a.resolve()?,
                    native_on_tap: None,
                    rynk_on_tap: None,
                    on_hold: b.resolve()?,
                    native_on_hold: None,
                    rynk_on_hold: None,
                    on_double_tap: c.resolve()?,
                    native_on_double_tap: None,
                    rynk_on_double_tap: None,
                    on_tap_hold: d.resolve()?,
                    native_on_tap_hold: None,
                    rynk_on_tap_hold: None,
                    tapping_term: *term,
                })
            })
            .collect::<Result<_>>()?;
        let key_overrides = self
            .key_override
            .iter()
            .map(|e| {
                Ok(EntKeyOverrideEntry {
                    trigger: e.trigger.resolve()?,
                    replacement: e.replacement.resolve()?,
                    layers: e.layers,
                    trigger_mods: e.trigger_mods,
                    negative_mod_mask: e.negative_mod_mask,
                    suppressed_mods: e.suppressed_mods,
                    options: e.options,
                })
            })
            .collect::<Result<_>>()?;
        let alt_repeat = self
            .alt_repeat_key
            .iter()
            .map(|e| {
                Ok(EntAltRepeatEntry {
                    keycode: e.keycode.resolve()?,
                    alt_keycode: e.alt_keycode.resolve()?,
                    allowed_mods: e.allowed_mods,
                    options: e.options,
                })
            })
            .collect::<Result<_>>()?;
        let layout_options = match self.layout_options {
            -1 => None,
            value => Some(u32::try_from(value).context("Invalid Vial layout options")?),
        };
        ensure!(
            layout_options.is_none()
                || app.layout_options_value.is_some()
                || layout_options == Some(0),
            "The keyboard does not have confirmed layout options"
        );
        let combo_term = self
            .settings
            .get(&2)
            .copied()
            .map(u16::try_from)
            .transpose()
            .context("Vial Combo term does not fit the firmware setting")?;
        let portable_settings = self
            .settings
            .iter()
            .filter(|&(&id, _)| id != 2)
            .map(|(&qsid, &value)| {
                let spec = ent_portable_setting_spec(qsid)
                    .filter(|s| qsid < 100 && value <= u32::from(s.max))
                    .with_context(|| {
                        format!("Unsupported QMK setting {qsid} or value {value} in Vial backup")
                    })?;
                Ok(EntPortableSetting {
                    id: spec.id.into(),
                    qsid,
                    width: spec.width,
                    value: value as u16,
                    module: None,
                    variants: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let bundle = EntLayoutFile {
            format: ENTLAYOUT_FORMAT.into(),
            version: ENTLAYOUT_VERSION,
            created_by: "Vial".into(),
            created_at: String::new(),
            keyboard: EntLayoutKeyboard {
                name: app.current_device_name_if_known(layout),
                firmware: FirmwareProtocol::Vial,
                keyboard_id: Some(self.uid),
                layout_hash: entlayout_hash(layout),
                layers: app.layer_count,
                keys: layout.keys.len(),
                encoders: layout.encoder_count(),
                rows: layout.rows,
                cols: layout.cols,
            },
            data: EntLayoutData {
                rgb: None,
                source_layout: Some(entlayout_source_layout(layout)),
                keymap,
                native_keymap: Vec::new(),
                rynk_keymap: Vec::new(),
                encoder_keymap,
                encoder_visibility: Vec::new(),
                layout_element_visibility: None,
                layout_options: layout_options.filter(|_| app.layout_options_value.is_some()),
                layer_names: Vec::new(),
                text_expander: EntTextExpanderData::default(),
                macros: EntMacroData {
                    bytecode_base64,
                    texts: Vec::new(),
                    names: Vec::new(),
                    descriptions: Vec::new(),
                },
                combos: EntComboData {
                    entries: combos,
                    names: Vec::new(),
                    colors: Vec::new(),
                    term: combo_term,
                },
                tap_dance: EntTapDanceData {
                    entries: tap_dance,
                    names: Vec::new(),
                },
                key_overrides: EntKeyOverrideData {
                    entries: key_overrides,
                    names: Vec::new(),
                },
                alt_repeat: EntAltRepeatData {
                    entries: alt_repeat,
                    names: Vec::new(),
                },
                portable_settings,
            },
            application_layouts: None,
            pictograms: None,
            application_categories: Vec::new(),
        };
        self.validate_bindings(app, layout, &bundle)?;
        Ok(bundle)
    }

    fn validate_bindings(
        &self,
        app: &EntropyApp,
        layout: &KeyboardLayout,
        bundle: &EntLayoutFile,
    ) -> Result<()> {
        let limits = app.entlayout_keycode_import_limits();
        let check = |code, section| {
            ensure!(
                matches!(check_entlayout_keycode(code, bundle, layout, limits), Ok(mapped) if mapped == code),
                "Unrestorable Vial keycode {code:#06x} in {section}"
            );
            Ok(())
        };
        for &code in bundle.data.keymap.iter().flatten() {
            check(code, "keymap")?;
        }
        for &code in bundle.data.encoder_keymap.iter().flatten() {
            check(code, "encoders")?;
        }
        for entry in &bundle.data.combos.entries {
            for code in entry.keys.into_iter().chain([entry.output]) {
                check(code, "Combo")?;
            }
        }
        for entry in &bundle.data.tap_dance.entries {
            for code in [
                entry.on_tap,
                entry.on_hold,
                entry.on_double_tap,
                entry.on_tap_hold,
            ] {
                check(code, "Tap Dance")?;
            }
        }
        for entry in &bundle.data.key_overrides.entries {
            check(entry.trigger, "Key Override")?;
            check(entry.replacement, "Key Override")?;
        }
        for entry in &bundle.data.alt_repeat.entries {
            check(entry.keycode, "Alt Repeat")?;
            check(entry.alt_keycode, "Alt Repeat")?;
        }
        Ok(())
    }
}

/// Vial GUI qmk_settings.json describes these wider slots. Entropy's portable
/// contract accepts narrower values for some devices, so read the whole wire
/// value when deciding whether its backup will be reversible.
fn read_vial_setting(
    vial: &crate::hid::HidDevice,
    qsid: u16,
    width: EntPortableSettingWidth,
) -> Result<u32> {
    match (qsid, width) {
        (21, _) => vial.get_qmk_setting_u32(qsid),
        (9..=17, _) | (_, EntPortableSettingWidth::U16) => {
            vial.get_qmk_setting_u16(qsid).map(u32::from)
        }
        _ => vial.get_qmk_setting_u8(qsid).map(u32::from),
    }
}

impl EntropyApp {
    /// The older editors can retain a failed edit in their cache. Confirm the
    /// entries that can be edited synchronously or normalized during loading.
    fn confirm_vial_import_dynamic_state(&self, vial: &crate::hid::HidDevice) -> Result<()> {
        for (index, entry) in self.key_override_entries.iter().enumerate() {
            let cached = (
                entry.trigger,
                entry.replacement,
                entry.layers,
                entry.trigger_mods,
                entry.negative_mod_mask,
                entry.suppressed_mods,
                entry.options.bits(),
            );
            ensure!(
                vial.get_key_override(index as u8)? == cached,
                "Key Override slot {} differs from the keyboard. Entropy cannot save a verified recovery backup.",
                index + 1
            );
        }
        for (index, entry) in self.alt_repeat_entries.iter().enumerate() {
            let cached = (
                entry.keycode,
                entry.alt_keycode,
                entry.allowed_mods,
                entry.options.bits(),
            );
            ensure!(
                vial.get_alt_repeat_key(index as u8)? == cached,
                "Alt Repeat slot {} differs from the keyboard. Entropy cannot save a verified recovery backup.",
                index + 1
            );
        }
        Ok(())
    }

    pub(super) fn import_vial_backup(&mut self, path: &Path) -> Result<String> {
        use std::io::Read as _;
        let file = std::fs::File::open(path).with_context(|| format!("Read {}", path.display()))?;
        let mut data = Vec::new();
        file.take(MAX_FILE_BYTES + 1).read_to_end(&mut data)?;
        ensure!(
            data.len() as u64 <= MAX_FILE_BYTES,
            "Vial backup exceeds the file size limit"
        );
        let source: VialBackup =
            serde_json::from_slice(&data).context("Invalid Vial backup JSON")?;
        let bundle = source.convert(self)?;
        self.validate_entlayout_file(&bundle)?;
        ensure!(
            self.vial_unlocked == Some(true),
            "Unlock the keyboard before importing the Vial backup"
        );
        ensure!(!self.hid_write_lifecycle_busy(), "The keyboard is busy");
        ensure!(
            !(self.keycode_picker.macros_dirty
                || self.combo_dirty
                || self.combo_term_dirty
                || self.keycode_picker.tap_dance_dirty
                || self.key_override_dirty
                || !self.pending_tap_hold_numeric_writes.is_empty()
                || (!self.auto_shift_timeout_text.is_empty()
                    && self.auto_shift_timeout_text.parse::<u16>().ok()
                        != self.auto_shift_timeout)),
            "Save or discard unsaved keyboard edits before importing a Vial backup"
        );
        self.confirm_entlayout_macros()?;
        self.confirm_uncertain_layer_names()?;
        self.require_complete_entlayout_lighting()?;
        let mapping = self.entlayout_import_mapping(&bundle)?;
        ensure!(mapping.exact_layout, "Connected keyboard layout changed");

        let hid = self
            .hid_device
            .as_ref()
            .context("No active keyboard connection")?;
        preflight_vial_entlayout(hid, &bundle)?;
        let vial = hid.require_vial()?;
        ensure!(
            vial.get_keyboard_id()? == (source.vial_protocol, source.uid)
                && vial.get_protocol_version()? == source.via_protocol,
            "Connected keyboard UID or protocol changed"
        );
        let macros = entmacro_bytecode(&bundle.data.macros)?;
        if !macros.is_empty() {
            let size = hid.get_macro_buffer_size()?;
            hid.encode_macros(&macros, size)?;
        }
        for layer in 0..self.layer_count {
            ensure!(
                self.deferred_device_load.layer_status(layer).ready(),
                "Wait for all keyboard layers to load before importing"
            );
        }
        for section in [
            DeferredLoadSection::Macros,
            DeferredLoadSection::Combos,
            DeferredLoadSection::TapDance,
            DeferredLoadSection::KeyOverrides,
            DeferredLoadSection::AltRepeat,
            DeferredLoadSection::BehaviorSettings,
        ] {
            ensure!(
                !self.deferred_device_load.section_supported(section)
                    || self.deferred_device_load.section_status(section).ready(),
                "Wait for keyboard settings to load before importing ({section:?})"
            );
        }
        ensure!(
            bundle.data.combos.term.is_none() || self.combo_term.is_some(),
            "The keyboard does not have a confirmed Combo term"
        );
        let target = self.entlayout_portable_settings_snapshot();
        let settings =
            plan_entlayout_portable_settings_import(&bundle.data.portable_settings, &target, &[]);
        ensure!(
            settings.report.unsupported == 0 && settings.report.incompatible == 0,
            "Vial backup contains QMK settings this keyboard cannot restore"
        );
        for setting in &bundle.data.portable_settings {
            let current = read_vial_setting(vial, setting.qsid, setting.width)?;
            let previous = target
                .iter()
                .find(|previous| previous.qsid == setting.qsid)
                .context("Missing pre-import QMK setting")?;
            ensure!(
                current == u32::from(previous.value),
                "QMK setting {} on the keyboard differs from the backup value. Entropy cannot save a verified recovery backup.",
                setting.qsid
            );
        }
        if bundle.data.layout_options.is_some() {
            ensure!(
                Some(vial.get_layout_options()?) == self.layout_options_value,
                "Layout options changed on the keyboard. Reconnect before retrying."
            );
        }
        if bundle.data.combos.term.is_some() {
            ensure!(
                Some(vial.get_qmk_setting_u16(2)?) == self.combo_term,
                "Combo term changed on the keyboard. Reconnect before retrying."
            );
        }
        self.confirm_vial_import_dynamic_state(vial)?;
        let current = self
            .entlayout_snapshot()
            .context("Pre-import keyboard backup is incomplete")?;
        self.validate_entlayout_file(&current)?;
        let backup = self.publish_entlayout_import_backup(&current)?;
        let result = (|| -> Result<()> {
            let (failures, settings) =
                self.apply_entlayout_firmware_state_for_import(&bundle, &mapping, true)?;
            ensure!(failures.is_empty(), "{}", failures.lines.join("\n"));
            ensure!(
                settings.imported == bundle.data.portable_settings.len(),
                "Some QMK settings could not be restored"
            );
            let vial = self
                .hid_device
                .as_ref()
                .context("Keyboard disconnected")?
                .require_vial()?;
            for setting in &bundle.data.portable_settings {
                let current = read_vial_setting(vial, setting.qsid, setting.width)?;
                ensure!(
                    current == u32::from(setting.value),
                    "QMK setting {} readback mismatch",
                    setting.qsid
                );
            }
            self.apply_entlayout_local_state_for_import(
                &bundle,
                &mapping,
                !macros.is_empty(),
                true,
            )?;
            self.deferred_device_load.confirm_import_publication();
            self.refresh_layer_picker_content_flags();
            Ok(())
        })();
        result.with_context(|| format!("Vial import did not finish. Reconnect before retrying. Restore the pre-import backup at {}", backup.path.display()))?;
        let lang = self.app_settings.language;
        Ok(format!(
            "{}\n\n{}:\n{}",
            crate::i18n::tr_catalog(lang, "entlayout.import_complete"),
            crate::i18n::tr_catalog(lang, "entlayout.auto_backup"),
            backup.path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn vial_file() -> serde_json::Value {
        json!({
            "version": 1, "uid": 42, "vial_protocol": 6, "via_protocol": 9,
            "layout": [[["KC_A", -1, "LCTL(KC_Z)"]]],
            "encoder_layout": [[["KC_VOLU", "KC_VOLD"]]],
            "layout_options": -1
        })
    }

    fn connected_app() -> EntropyApp {
        let mut app = EntropyApp::new_inert_for_test();
        let mut layout = KeyboardLayout::from_vial_json(&json!({
            "name": "Vial test",
            "matrix": {"rows": 1, "cols": 3},
            "layouts": {"keymap": [["0,2", "0,0", "0,0\n\n\n\n\n\n\n\n\ne", "0,1\n\n\n\n\n\n\n\n\ne"]]}
        })).unwrap();
        layout.layers = vec![vec![0.into(); layout.keys.len()]];
        layout.encoder_layers = vec![vec![0; layout.encoders.len()]];
        app.layout = Some(layout);
        app.firmware = FirmwareProtocol::Vial;
        app.current_keyboard_id = Some(42);
        app.device_about_info = Some(DeviceAboutInfo {
            keyboard_id: 42,
            vial_protocol: 6,
            via_protocol: 9,
            supports_macro_ext_keycodes: true,
            ..Default::default()
        });
        app.layer_count = 1;
        app.vial_unlocked = Some(true);
        app.keycode_picker.macro_count = 0;
        app
    }

    #[test]
    fn entlayout_vial_backup_reads_protocol_names_and_matrix_in_target_order() {
        let source: VialBackup = serde_json::from_value(vial_file()).unwrap();
        let app = connected_app();
        let bundle = source.convert(&app).unwrap();
        assert_eq!(bundle.data.keymap, vec![vec![0x011d, 4]]);
        assert_eq!(bundle.data.encoder_keymap, vec![vec![0x00a9, 0x00aa]]);
        assert_eq!(resolve_name("PDF(31)"), Some(0x52ff));
        assert_eq!(resolve_name("LT15(KC_A)"), Some(0x4f04));
        assert_eq!(resolve_name("TD(255)"), Some(0x57ff));
        assert_eq!(resolve_name("USER04"), Some(0x7e04));
        assert_eq!(resolve_name("0x7eee"), Some(0x7eee));
        assert_eq!(resolve_name("MO(32)"), None);
        assert_eq!(resolve_name("OSM(MOD_LCTL|MOD_LSFT)"), Some(0x52a3));
    }

    #[test]
    fn entlayout_vial_backup_converts_dynamic_entries_and_macro_wire_encoding() {
        let mut source = vial_file();
        source["macro"] = json!([[
            ["text", "Hi"],
            ["down", "KC_LCTRL"],
            ["tap", "KC_A", "LCTL(KC_B)"],
            ["up", "KC_LCTRL"],
            ["delay", 255]
        ]]);
        source["combo"] = json!([["KC_A", "KC_B", 0, 0, "M0"]]);
        source["tap_dance"] = json!([["KC_A", "KC_B", "KC_C", "KC_D", 230]]);
        source["key_override"] = json!([{"trigger": "KC_A", "replacement": "KC_B",
            "layers": 65535, "trigger_mods": 2, "negative_mod_mask": 1, "suppressed_mods": 2, "options": 3}]);
        source["alt_repeat_key"] =
            json!([{"keycode": "KC_A", "alt_keycode": "KC_B", "allowed_mods": 2, "options": 0}]);
        source["settings"] = json!({"2": 42, "7": 201});
        let source: VialBackup = serde_json::from_value(source).unwrap();
        let mut app = connected_app();
        app.keycode_picker.macro_count = 1;
        app.combo_entries.push(ComboEntry::default());
        app.keycode_picker
            .tap_dance_entries
            .push(crate::keycode_picker::TapDanceEntry::default());
        app.key_override_entries.push(KeyOverrideEntry::default());
        app.alt_repeat_entries.push(AltRepeatKeyEntry::default());
        let bundle = source.convert(&app).unwrap();
        assert_eq!(
            entmacro_bytecode(&bundle.data.macros).unwrap()[0],
            [b'H', b'i', 1, 2, 0xe0, 1, 1, 4, 1, 5, 5, 1, 1, 3, 0xe0, 1, 4, 1, 2,]
        );
        assert_eq!(bundle.data.combos.entries[0].output, 0x7700);
        assert_eq!(bundle.data.combos.term, Some(42));
        assert_eq!(bundle.data.tap_dance.entries[0].on_tap_hold, 7);
        assert_eq!(bundle.data.tap_dance.entries[0].tapping_term, 230);
        assert_eq!(bundle.data.key_overrides.entries[0].options, 3);
        assert_eq!(bundle.data.alt_repeat.entries[0].allowed_mods, 2);
        assert_eq!(bundle.data.portable_settings[0].qsid, 7);
        assert_eq!(bundle.data.portable_settings[0].value, 201);
    }

    #[test]
    fn entlayout_vial_backup_refuses_mismatch_and_unrestorable_actions_before_hid() {
        let storage = crate::app::app_storage::ScopedTempConfig::new();
        let path = storage.path().join("test.VIL");
        let mut app = connected_app();
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid.into());
        for (key, value, message) in [
            ("version", json!(3), "backup version"),
            ("uid", json!(43), "does not match"),
            ("vial_protocol", json!(5), "backup protocol"),
            ("layout_options", json!(-2), "Invalid Vial layout options"),
            ("layout", json!([[["KC_A", "KC_B", -1]]]), "matrix differs"),
            (
                "layout",
                json!([[["KC_A", -1, "NO_SUCH_KEY"]]]),
                "Unsupported Vial keycode",
            ),
            (
                "layout",
                json!([[["KC_A", -1, "M3"]]]),
                "Unrestorable Vial keycode",
            ),
            ("combo", json!([[4, 5, 0, 0, 6]]), "Combo slots"),
        ] {
            let mut source = vial_file();
            source[key] = value;
            std::fs::write(&path, serde_json::to_vec(&source).unwrap()).unwrap();
            let error = app.import_entlayout_from_path(&path).unwrap_err();
            assert!(format!("{error:#}").contains(message), "{error:#}");
        }
        assert!(recorder.requests().is_empty());
    }

    #[test]
    fn entlayout_vial_import_publishes_backup_and_preserves_host_settings() {
        verify_vial_import(false, false);
    }

    #[test]
    fn entlayout_vial_import_exposes_partial_write_and_does_not_publish_local_keymap() {
        verify_vial_import(true, false);
    }

    #[test]
    fn entlayout_vial_import_requires_reconnect_after_lost_options_readback() {
        verify_vial_import(false, true);
    }

    fn verify_vial_import(encoder_mismatch: bool, lost_options_readback: bool) {
        let storage = crate::app::app_storage::ScopedTempConfig::new();
        let mut app = connected_app();
        app.current_device_name = "Vial test".into();
        app.layer_names = vec!["My layer".into()];
        app.encoder_visibility = vec![false, true];
        app.app_settings.text_expander_enabled = true;
        app.layout_options_value = Some(4);
        let mut source = vial_file();
        source["layout_options"] = json!(8);
        let path = storage.path().join("test.vil");
        std::fs::write(&path, serde_json::to_vec(&source).unwrap()).unwrap();
        let backup_dir = storage.path().join("entropy").join("backups");
        let observed_dir = backup_dir.clone();
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let mut keymap = [0u16; 3];
        let mut encoders = [0u16; 2];
        let mut options = 4u32;
        recorder.respond_by(move |request| {
            let mut reply = [0; 32];
            match request[0] {
                0xfe if request[1] == 0 => {
                    reply[..4].copy_from_slice(&6u32.to_le_bytes());
                    reply[4..12].copy_from_slice(&42u64.to_le_bytes());
                }
                0x01 => reply[1..3].copy_from_slice(&9u16.to_be_bytes()),
                0x02 if request[1] == 2 => {
                    if lost_options_readback && options == 8 {
                        return Some(Err("Lost layout options readback".into()));
                    }
                    reply[2..6].copy_from_slice(&options.to_be_bytes());
                }
                0x03 if request[1] == 2 => {
                    options = u32::from_be_bytes(request[2..6].try_into().unwrap());
                }
                0x05 => {
                    assert!(observed_dir.read_dir().unwrap().any(|entry| {
                        entry
                            .unwrap()
                            .path()
                            .extension()
                            .is_some_and(|ext| ext == "entlayout")
                    }));
                    keymap[request[3] as usize] = u16::from_be_bytes([request[4], request[5]]);
                }
                0x04 => reply[4..6].copy_from_slice(&keymap[request[3] as usize].to_be_bytes()),
                0xfe if request[1] == 0x04 => {
                    if !encoder_mismatch {
                        encoders[request[4] as usize] =
                            u16::from_be_bytes([request[5], request[6]]);
                    }
                }
                0xfe if request[1] == 0x03 => {
                    reply[..2].copy_from_slice(&encoders[0].to_be_bytes());
                    reply[2..4].copy_from_slice(&encoders[1].to_be_bytes());
                }
                _ => return Some(Err(format!("Unexpected HID request {request:02x?}"))),
            }
            Some(Ok(reply))
        });
        app.hid_device = Some(hid.into());
        let result = app.import_entlayout_from_path(&path);
        let mut backups = backup_dir.read_dir().unwrap();
        let backup_path = backups.next().unwrap().unwrap().path();
        assert!(backups.next().is_none());
        let saved: EntLayoutFile =
            serde_json::from_slice(&std::fs::read(&backup_path).unwrap()).unwrap();
        assert_eq!(saved.data.keymap, vec![vec![0, 0]]);
        assert_eq!(saved.data.layout_options, Some(4));
        assert_eq!(app.layout_options_import_unconfirmed, lost_options_readback);
        if lost_options_readback {
            let error = format!("{:#}", result.unwrap_err());
            assert!(error.contains("Lost layout options readback"), "{error}");
            assert!(error.contains(backup_path.to_str().unwrap()), "{error}");
            assert!(app.entlayout_snapshot().is_none());
            let requests = recorder.requests().len();
            let error = app.import_entlayout_from_path(&backup_path).unwrap_err();
            assert!(error.to_string().contains("Reconnect"));
            assert_eq!(recorder.requests().len(), requests);
            return;
        }
        // Options were independently verified even when the encoder failed.
        assert_eq!(app.layout_options_value, Some(8));
        assert_eq!(saved.data.layer_names, ["My layer"]);
        assert!(saved.data.text_expander.enabled);
        assert_eq!(app.layer_names, ["My layer"]);
        assert_eq!(app.encoder_visibility, [false, true]);
        assert!(app.app_settings.text_expander_enabled);
        let layout = app.layout.as_ref().unwrap();
        if encoder_mismatch {
            let error = format!("{:#}", result.unwrap_err());
            assert!(error.contains("Encoder readback mismatch"), "{error}");
            assert!(error.contains(backup_path.to_str().unwrap()), "{error}");
            assert_eq!(layout.get_keycode(0, 0), 0);
            assert!(app.deferred_device_load.import_layer_unconfirmed(0));
        } else {
            let result = result.unwrap();
            assert!(result.contains(backup_path.to_str().unwrap()));
            assert_eq!(layout.get_keycode(0, 0), 0x011d);
            assert_eq!(layout.get_encoder_keycode(0, 0), 0xa9);
            assert!(!app.deferred_device_load.has_unconfirmed_import_data());
        }
    }

    #[test]
    fn entlayout_vial_import_blocks_pending_tapping_term_write() {
        let storage = crate::app::app_storage::ScopedTempConfig::new();
        let path = storage.path().join("test.vil");
        std::fs::write(&path, serde_json::to_vec(&vial_file()).unwrap()).unwrap();
        let mut app = connected_app();
        app.pending_tap_hold_numeric_writes.insert(7, 250);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid.into());
        let error = app.import_entlayout_from_path(&path).unwrap_err();
        assert!(error.to_string().contains("Save or discard"));
        assert_eq!(app.pending_tap_hold_numeric_writes.get(&7), Some(&250));
        assert!(recorder.requests().is_empty());
    }

    #[test]
    fn entlayout_vial_import_checks_physical_alt_repeat_before_backup() {
        let storage = crate::app::app_storage::ScopedTempConfig::new();
        let path = storage.path().join("test.vil");
        std::fs::write(&path, serde_json::to_vec(&vial_file()).unwrap()).unwrap();
        let mut app = connected_app();
        app.alt_repeat_entries.push(AltRepeatKeyEntry {
            keycode: 4,
            ..Default::default()
        });
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        recorder.respond_by(|request| {
            let mut reply = [0; 32];
            match request[..3] {
                [0xfe, 0, _] => {
                    reply[..4].copy_from_slice(&6u32.to_le_bytes());
                    reply[4..12].copy_from_slice(&42u64.to_le_bytes());
                }
                [0x01, _, _] => reply[1..3].copy_from_slice(&9u16.to_be_bytes()),
                [0xfe, 0x0d, 0x07] => {}
                _ => panic!("Unexpected HID request {request:02x?}"),
            }
            Some(Ok(reply))
        });
        app.hid_device = Some(hid.into());
        let error = app.import_entlayout_from_path(&path).unwrap_err();
        assert!(format!("{error:#}").contains("Alt Repeat slot 1 differs"));
        assert!(!storage.path().join("entropy").join("backups").exists());
    }

    #[test]
    fn entlayout_vial_backup_rejects_macro_data_that_cannot_round_trip() {
        for action in [
            json!(["delay", 65025]),
            json!(["text", "\u{0000}"]),
            json!(["tap", "KC_NO"]),
            json!(["tap", "LCTL(KC_A)"]),
            json!(["other", 123]),
        ] {
            assert!(encode_macro(&[serde_json::from_value(action).unwrap()], false).is_err());
        }
    }
}
