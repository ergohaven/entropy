use super::*;

/// Serialized definition contract only; runtime values stay in ModuleSettingsState.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct EntModuleField {
    group: String,
    side: String,
    title: String,
    kind: String,
    bit: u8,
    width: u8,
    min: u16,
    max: u16,
    variants: Vec<String>,
}

pub(super) fn module_setting_valid(setting: &EntPortableSetting) -> bool {
    let Some(fields) = &setting.module else {
        return false;
    };
    let width = match setting.width {
        EntPortableSettingWidth::U8 => 1,
        EntPortableSettingWidth::U16 => 2,
    };
    let limit = if width == 1 { 255 } else { u16::MAX };
    if setting.id != "module"
        || fields.is_empty()
        || !setting.variants.is_empty()
        || setting.value > limit
    {
        return false;
    }
    let mut bits = std::collections::BTreeSet::new();
    for field in fields {
        if field.group.is_empty()
            || field.title.is_empty()
            || !matches!(
                field.side.as_str(),
                "left" | "right" | "auto_layer" | "other"
            )
            || !matches!(field.width, 1 | 2)
            || field.width > width
            || field.min > field.max
            || field.max > limit
        {
            return false;
        }
        match field.kind.as_str() {
            "boolean" => {
                // Preserve the raw packed word, including firmware-reserved bits.
                // Every described bit must be distinct and have the same width.
                if field.bit >= width * 8 || !field.variants.is_empty() || !bits.insert(field.bit) {
                    return false;
                }
            }
            "integer" | "select" => {
                if fields.len() != 1
                    || field.bit != 0
                    || setting.value < field.min
                    || setting.value > field.max
                {
                    return false;
                }
                if field.kind == "select" {
                    if field.variants.is_empty()
                        || field.variants.iter().any(|s| s.is_empty())
                        || field.max as usize >= field.variants.len()
                        || setting.value as usize >= field.variants.len()
                    {
                        return false;
                    }
                    let unique = field
                        .variants
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>();
                    if unique.len() != field.variants.len() {
                        return false;
                    }
                } else if !field.variants.is_empty() {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

pub(super) fn module_setting_import_value(
    source: &EntPortableSetting,
    target: &EntPortableSetting,
) -> u16 {
    if let Some(value) = module_setting_migration_value(source, target) {
        return value;
    }
    let Some(fields) = &source.module else {
        return source.value;
    };
    if !fields.iter().all(|field| field.kind == "boolean") {
        return source.value;
    }
    let mask = fields
        .iter()
        .fold(0u16, |mask, field| mask | (1u16 << field.bit));
    (source.value & mask) | (target.value & !mask)
}

pub(super) fn module_setting_migration_value(
    source: &EntPortableSetting,
    target: &EntPortableSetting,
) -> Option<u16> {
    if !module_setting_valid(source)
        || !module_setting_valid(target)
        || source.width != target.width
    {
        return None;
    }
    let [field] = source.module.as_deref()? else {
        return None;
    };
    let [target_field] = target.module.as_deref()? else {
        return None;
    };
    // These QMK registers moved because RMK uses 149/150 for module selection.
    let relocated = match (source.qsid, field.side.as_str(), field.title.as_str()) {
        (149, "left", "Invert scroll horizontal") => 327,
        (150, "right", "Invert scroll horizontal") => 328,
        (151, "left", "Invert text horizontal") => 329,
        (152, "right", "Invert text horizontal") => 330,
        (327, "left", "Touch gestures") => 151,
        (328, "right", "Touch gestures") => 152,
        (327, "left", "Invert scroll horizontal") => 149,
        (328, "right", "Invert scroll horizontal") => 150,
        (329, "left", "Invert text horizontal") => 151,
        (330, "right", "Invert text horizontal") => 152,
        (151, "left", "Touch gestures") => 327,
        (152, "right", "Touch gestures") => 328,
        _ => source.qsid,
    };
    if source.qsid != target.qsid {
        return (relocated == target.qsid && field.kind == "boolean" && field == target_field)
            .then(|| {
                let mask = 1u16 << field.bit;
                (source.value & mask) | (target.value & !mask)
            });
    }

    let old_timeouts = [
        "250 ms", "500 ms", "750 ms", "1000 ms", "1250 ms", "1500 ms",
    ];
    let new_timeouts = [
        "50 ms", "100 ms", "150 ms", "200 ms", "250 ms", "300 ms", "350 ms", "400 ms", "450 ms",
        "500 ms", "750 ms", "1000 ms", "1250 ms", "1500 ms",
    ];
    let manual = ["None", "Encoder", "Trackball", "Touchpad"];
    let auto = ["None", "Encoder", "Trackball", "Touchpad", "Auto"];
    let (before, after): (&[&str], &[&str]) =
        match (source.qsid, field.side.as_str(), field.title.as_str()) {
            (324, "auto_layer", "Auto layer timeout") => (&old_timeouts, &new_timeouts),
            (149, "left", "Module") | (150, "right", "Module") => (&manual, &auto),
            _ => return None,
        };
    let is_variants = |field: &EntModuleField, variants: &[&str]| {
        field.kind == "select"
            && field.min == 0
            && usize::from(field.max) + 1 == variants.len()
            && field
                .variants
                .iter()
                .map(String::as_str)
                .eq(variants.iter().copied())
    };
    let extended_timeouts = [
        "50 ms", "100 ms", "150 ms", "200 ms", "250 ms", "300 ms", "350 ms", "400 ms", "450 ms",
        "500 ms", "750 ms", "1000 ms", "1250 ms", "1500 ms", "1750 ms", "2000 ms", "2250 ms",
        "2500 ms", "2750 ms", "3000 ms",
    ];
    let qube_extended_timeouts = [
        "250 ms", "500 ms", "750 ms", "1000 ms", "1250 ms", "1500 ms", "1750 ms", "2000 ms",
        "2250 ms", "2500 ms", "2750 ms", "3000 ms",
    ];
    let timeout_extension = source.qsid == 324
        && source.width == EntPortableSettingWidth::U8
        && field.width == 1
        && target_field.width == 1
        && [
            old_timeouts.as_slice(),
            new_timeouts.as_slice(),
            extended_timeouts.as_slice(),
            qube_extended_timeouts.as_slice(),
        ]
        .iter()
        .any(|variants| is_variants(field, variants))
        && [
            old_timeouts.as_slice(),
            new_timeouts.as_slice(),
            extended_timeouts.as_slice(),
            qube_extended_timeouts.as_slice(),
        ]
        .iter()
        .any(|variants| is_variants(target_field, variants));
    if !(timeout_extension
        || (is_variants(field, before) && is_variants(target_field, after))
        || (is_variants(field, after) && is_variants(target_field, before)))
    {
        return None;
    }
    let mut normalized = field.clone();
    normalized.max = target_field.max;
    normalized.variants.clone_from(&target_field.variants);
    if normalized != *target_field {
        return None;
    }
    target_field
        .variants
        .iter()
        .position(|label| label == &field.variants[usize::from(source.value)])
        .map(|index| index as u16)
}

impl EntropyApp {
    pub(super) fn entlayout_module_settings_snapshot(&self) -> Vec<EntPortableSetting> {
        if !self.module_settings.supported {
            return Vec::new();
        }
        let mut contracts = std::collections::BTreeMap::<u16, Vec<EntModuleField>>::new();
        for group in &self.module_settings.groups {
            for field in &group.fields {
                let side = match group.kind {
                    ModuleSettingsGroupKind::Left => "left",
                    ModuleSettingsGroupKind::Right => "right",
                    ModuleSettingsGroupKind::AutoLayer => "auto_layer",
                    ModuleSettingsGroupKind::Other => "other",
                };
                let kind = match field.kind {
                    ModuleSettingKind::Boolean => "boolean",
                    ModuleSettingKind::Integer => "integer",
                    ModuleSettingKind::Select => "select",
                };
                contracts
                    .entry(field.qsid)
                    .or_default()
                    .push(EntModuleField {
                        group: group.title.clone(),
                        side: side.to_owned(),
                        title: field.title.clone(),
                        kind: kind.to_owned(),
                        bit: field.bit,
                        width: field.width,
                        min: field.min,
                        max: field.max,
                        variants: field.variants.clone(),
                    });
            }
        }
        contracts
            .into_iter()
            .filter_map(|(qsid, fields)| {
                if !self.supported_qmk_settings.contains(&qsid) {
                    return None;
                }
                let value = *self.module_settings.values.get(&qsid)?;
                let width = match fields.iter().map(|field| field.width).max()? {
                    1 => EntPortableSettingWidth::U8,
                    2 => EntPortableSettingWidth::U16,
                    _ => return None,
                };
                let setting = EntPortableSetting {
                    id: "module".to_owned(),
                    qsid,
                    width,
                    value,
                    variants: Vec::new(),
                    module: Some(fields),
                };
                module_setting_valid(&setting).then_some(setting)
            })
            .collect()
    }
}
