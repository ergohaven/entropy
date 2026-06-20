use super::*;

/// Sanitize a device name into a filesystem-safe slug.
pub(super) fn device_id_slug(device_name: &str) -> String {
    device_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

pub(super) fn layer_names_path(device_name: &str) -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("entropy");
    std::fs::create_dir_all(&dir).ok();
    let slug = device_id_slug(device_name);
    dir.join(format!("layer_names_{}.json", slug))
}

pub(super) fn single_instance_signal_path() -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("entropy");
    std::fs::create_dir_all(&dir).ok();
    dir.join("single_instance_signal")
}

pub(super) fn read_single_instance_signal() -> String {
    std::fs::read_to_string(single_instance_signal_path())
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub(super) fn app_settings_path() -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("entropy");
    std::fs::create_dir_all(&dir).ok();
    dir.join("app_settings.json")
}

const TEXT_EXPANDER_MAIN_RULES_FILE: &str = "text_expansion_rules.json";

pub(super) fn text_expander_config_dir() -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("entropy");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub(super) fn text_expander_rules_dir() -> std::path::PathBuf {
    let dir = text_expander_config_dir().join("text_expander_rules");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub(super) fn legacy_text_expander_rules_path() -> std::path::PathBuf {
    text_expander_config_dir().join(TEXT_EXPANDER_MAIN_RULES_FILE)
}

pub(super) fn text_expander_rules_path() -> std::path::PathBuf {
    text_expander_rules_dir().join(TEXT_EXPANDER_MAIN_RULES_FILE)
}

pub(super) fn migrate_legacy_text_expander_rules_file() {
    let legacy = legacy_text_expander_rules_path();
    let current = text_expander_rules_path();
    if !current.exists() && legacy.exists() {
        if let Err(e) = std::fs::copy(&legacy, &current) {
            log::warn!("migrate_legacy_text_expander_rules_file failed: {e}");
        }
    }
}

pub(super) fn path_modified(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
}

pub(super) fn normalize_text_expander_rules_file_name(raw: &str) -> Option<String> {
    let name = std::path::Path::new(raw)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)?;
    if name.is_empty()
        || name.eq_ignore_ascii_case(TEXT_EXPANDER_MAIN_RULES_FILE)
        || !name.to_ascii_lowercase().ends_with(".json")
    {
        return None;
    }
    Some(name.to_owned())
}

pub(super) fn normalize_text_expander_rule_files(files: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for file in files {
        let Some(name) = normalize_text_expander_rules_file_name(file) else {
            continue;
        };
        if !normalized
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&name))
        {
            normalized.push(name);
        }
    }
    normalized
}

pub(super) fn text_expander_extra_rules_path(file_name: &str) -> std::path::PathBuf {
    text_expander_rules_dir().join(file_name)
}

pub(super) fn text_expander_available_extra_rule_files(selected_files: &[String]) -> Vec<String> {
    let selected = normalize_text_expander_rule_files(selected_files);
    let mut files = std::fs::read_dir(text_expander_rules_dir())
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let name = path.file_name()?.to_str()?;
            normalize_text_expander_rules_file_name(name)
        })
        .filter(|name| {
            !selected
                .iter()
                .any(|selected_name| selected_name.eq_ignore_ascii_case(name))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|name| name.to_ascii_lowercase());
    files
}

pub(super) fn text_expander_rules_signature(
    extra_files: &[String],
) -> Vec<(String, Option<std::time::SystemTime>)> {
    let normalized_extra_files = normalize_text_expander_rule_files(extra_files);
    let mut files = Vec::with_capacity(normalized_extra_files.len() + 1);
    let primary = text_expander_rules_path();
    files.push((
        primary.to_string_lossy().to_string(),
        path_modified(&primary),
    ));
    for file in normalized_extra_files {
        let path = text_expander_extra_rules_path(&file);
        files.push((file, path_modified(&path)));
    }
    files
}

pub(super) fn display_preset_restore_path(device_name: &str) -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("entropy");
    std::fs::create_dir_all(&dir).ok();
    dir.join(format!(
        "display_preset_restore_{}.txt",
        device_id_slug(device_name)
    ))
}

pub(super) fn parse_text_expansion_rules_json(
    data: &str,
) -> Option<Vec<crate::text_expander::TextExpansionRule>> {
    if let Ok(rules) = serde_json::from_str::<Vec<crate::text_expander::TextExpansionRule>>(data) {
        return Some(rules);
    }

    let value = serde_json::from_str::<serde_json::Value>(data).ok()?;
    for key in ["rules", "text_expansion_rules"] {
        if let Some(rules_value) = value.get(key) {
            if let Ok(rules) = serde_json::from_value::<Vec<crate::text_expander::TextExpansionRule>>(
                rules_value.clone(),
            ) {
                return Some(rules);
            }
        }
    }
    None
}

pub(super) fn load_text_expansion_rules() -> Option<Vec<crate::text_expander::TextExpansionRule>> {
    load_text_expansion_rules_from_path(&text_expander_rules_path())
}

pub(super) fn load_text_expansion_rules_from_path(
    path: &std::path::Path,
) -> Option<Vec<crate::text_expander::TextExpansionRule>> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|data| parse_text_expansion_rules_json(&data))
}

pub(super) fn load_extra_text_expansion_rules(
    files: &[String],
) -> Vec<crate::text_expander::TextExpansionRule> {
    normalize_text_expander_rule_files(files)
        .iter()
        .filter_map(|file| {
            load_text_expansion_rules_from_path(&text_expander_extra_rules_path(file))
        })
        .flatten()
        .collect()
}

pub(super) fn save_text_expansion_rules(rules: &[crate::text_expander::TextExpansionRule]) {
    match serde_json::to_string_pretty(rules) {
        Ok(json) => {
            if let Err(e) = std::fs::write(text_expander_rules_path(), json) {
                log::warn!("save_text_expansion_rules failed: {e}");
            }
        }
        Err(e) => log::warn!("save_text_expansion_rules serialize failed: {e}"),
    }
}

pub(super) fn ensure_text_expander_rules_file(rules: &[crate::text_expander::TextExpansionRule]) {
    let path = text_expander_rules_path();
    if !path.exists() {
        save_text_expansion_rules(rules);
    }
}

pub(super) fn open_path_in_system_editor(path: &std::path::Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        return std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
    }
    #[cfg(target_os = "macos")]
    {
        return std::process::Command::new("open")
            .arg(path)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
    }
    #[cfg(target_os = "linux")]
    {
        return std::process::Command::new("xdg-open")
            .arg(path)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = path;
        false
    }
}

pub(super) fn load_app_settings() -> AppSettings {
    migrate_legacy_text_expander_rules_file();
    let path = app_settings_path();
    let settings_data = std::fs::read_to_string(&path).ok();
    let mut settings = settings_data
        .as_deref()
        .and_then(|data| serde_json::from_str::<AppSettings>(data).ok())
        .unwrap_or_default();

    let embedded_rules = settings.text_expansion_rules.clone();
    match load_text_expansion_rules() {
        Some(rules) if !rules.is_empty() => settings.text_expansion_rules = rules,
        Some(_) if !embedded_rules.is_empty() => {
            settings.text_expansion_rules = embedded_rules;
            save_text_expansion_rules(&settings.text_expansion_rules);
        }
        Some(rules) => settings.text_expansion_rules = rules,
        None => save_text_expansion_rules(&settings.text_expansion_rules),
    }
    settings.text_expander_rule_files =
        normalize_text_expander_rule_files(&settings.text_expander_rule_files);

    let has_close_behavior = settings_data
        .as_deref()
        .is_some_and(|data| data.contains("close_to_tray_behavior"));
    if !has_close_behavior && settings.minimize_to_tray_on_close {
        settings.close_to_tray_behavior = CloseToTrayBehavior::Tray;
    }
    settings.minimize_to_tray_on_close =
        matches!(settings.close_to_tray_behavior, CloseToTrayBehavior::Tray);

    let has_layout_image_export = settings_data
        .as_deref()
        .is_some_and(|data| data.contains("layout_image_export"));
    if !has_layout_image_export {
        settings.layout_image_export.key_legend_layout = settings.key_legend_layout;
    }

    settings
}

pub(super) fn save_app_settings(settings: &AppSettings) {
    match serde_json::to_string_pretty(settings) {
        Ok(json) => {
            if let Err(e) = std::fs::write(app_settings_path(), json) {
                log::warn!("save_app_settings failed: {e}");
            }
        }
        Err(e) => log::warn!("save_app_settings serialize failed: {e}"),
    }
}

pub(super) fn normalize_text_expander_app_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_matches(['\'', '"']);
    let name = trimmed
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(trimmed)
        .trim()
        .to_ascii_lowercase();
    (!name.is_empty()).then_some(name)
}

pub(super) fn parse_text_expander_blacklist(raw: &str) -> Vec<String> {
    let mut entries = Vec::new();
    for entry in raw.split([',', ';', '\n']) {
        let Some(name) = normalize_text_expander_app_name(entry) else {
            continue;
        };
        if !entries.iter().any(|existing| existing == &name) {
            entries.push(name);
        }
    }
    entries
}

pub(super) fn format_text_expander_blacklist(entries: &[String]) -> String {
    entries.join(", ")
}

pub(super) fn compact_dropdown_popup_height(
    option_count: usize,
    option_height: f32,
    spacing_y: f32,
) -> f32 {
    let visible = option_count.max(1).min(5) as f32;
    option_height * visible + spacing_y * (visible - 1.0).max(0.0)
}

pub(super) fn load_saved_layer_names(device_name: &str) -> Option<Vec<String>> {
    let path = layer_names_path(device_name);
    let data = std::fs::read_to_string(&path).ok()?;
    let v = serde_json::from_str::<Vec<String>>(&data).ok()?;
    if v.is_empty() {
        return None;
    }
    Some(v)
}

pub(super) fn load_layer_names(device_name: &str) -> Vec<String> {
    if let Some(v) = load_saved_layer_names(device_name) {
        return v;
    }
    let mut v: Vec<String> = (0..16).map(|i| i.to_string()).collect();
    v[0] = "Main".to_string();
    v
}

pub(super) fn encoder_visibility_id(device_name: &str, keyboard_id: u64) -> String {
    format!("vial_{keyboard_id:016x}_{device_name}")
}

pub(super) fn encoder_visibility_path(device_id: &str) -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("entropy");
    std::fs::create_dir_all(&dir).ok();
    let slug = device_id_slug(device_id);
    dir.join(format!("encoder_visibility_{}.json", slug))
}

fn load_encoder_visibility_from_id(device_id: &str, count: usize) -> Option<Vec<bool>> {
    let path = encoder_visibility_path(device_id);
    let data = std::fs::read_to_string(&path).ok()?;
    let mut v = serde_json::from_str::<Vec<bool>>(&data).ok()?;
    v.truncate(count);
    while v.len() < count {
        v.push(true);
    }
    Some(v)
}

pub(super) fn load_encoder_visibility(device_id: &str, count: usize) -> Vec<bool> {
    if count == 0 {
        return vec![];
    }
    load_encoder_visibility_from_id(device_id, count).unwrap_or_else(|| vec![true; count])
}

pub(super) fn save_encoder_visibility(visibility: &[bool], device_id: &str) {
    let path = encoder_visibility_path(device_id);
    match serde_json::to_string_pretty(visibility) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("save_encoder_visibility failed: {}", e);
            }
        }
        Err(e) => log::warn!("save_encoder_visibility serialize failed: {}", e),
    }
}

pub(super) fn save_layer_names(names: &[String], device_name: &str) {
    if let Ok(data) = serde_json::to_string(names) {
        let path = layer_names_path(device_name);
        if let Err(e) = std::fs::write(&path, &data) {
            log::warn!("save_layer_names failed at {:?}: {e}", path);
        } else {
            log::info!("save_layer_names ok → {:?}", path);
        }
    }
}

pub(super) fn tap_dance_names_path(device_name: &str) -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("entropy");
    std::fs::create_dir_all(&dir).ok();
    let slug = device_id_slug(device_name);
    dir.join(format!("tap_dance_names_{}.json", slug))
}

pub(super) fn load_tap_dance_names(device_name: &str) -> Vec<String> {
    let path = tap_dance_names_path(device_name);
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(v) = serde_json::from_str::<Vec<String>>(&data) {
            return v;
        }
    }
    vec![]
}

pub(super) fn save_tap_dance_names(names: &[String], device_name: &str) {
    if let Ok(data) = serde_json::to_string(names) {
        let path = tap_dance_names_path(device_name);
        if let Err(e) = std::fs::write(&path, &data) {
            log::warn!("save_tap_dance_names failed at {:?}: {e}", path);
        } else {
            log::info!("save_tap_dance_names ok → {:?}", path);
        }
    }
}

pub(super) fn combo_names_path(device_name: &str) -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("entropy");
    std::fs::create_dir_all(&dir).ok();
    let slug = device_id_slug(device_name);
    dir.join(format!("combo_names_{}.json", slug))
}

pub(super) fn load_combo_names(device_name: &str) -> Vec<String> {
    let path = combo_names_path(device_name);
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(v) = serde_json::from_str::<Vec<String>>(&data) {
            return v;
        }
    }
    vec![]
}

pub(super) fn save_combo_names(names: &[String], device_name: &str) {
    if let Ok(data) = serde_json::to_string(names) {
        let path = combo_names_path(device_name);
        if let Err(e) = std::fs::write(&path, &data) {
            log::warn!("save_combo_names failed at {:?}: {e}", path);
        } else {
            log::info!("save_combo_names ok → {:?}", path);
        }
    }
}

pub(super) fn combo_colors_path(device_name: &str) -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("entropy");
    std::fs::create_dir_all(&dir).ok();
    let slug = device_id_slug(device_name);
    dir.join(format!("combo_colors_{}.json", slug))
}

pub(super) fn load_combo_colors(device_name: &str) -> Vec<u32> {
    let path = combo_colors_path(device_name);
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(v) = serde_json::from_str::<Vec<u32>>(&data) {
            return v;
        }
    }
    vec![]
}

pub(super) fn save_combo_colors(colors: &[u32], device_name: &str) {
    if let Ok(data) = serde_json::to_string(colors) {
        let path = combo_colors_path(device_name);
        if let Err(e) = std::fs::write(&path, &data) {
            log::warn!("save_combo_colors failed at {:?}: {e}", path);
        } else {
            log::info!("save_combo_colors ok → {:?}", path);
        }
    }
}

pub(super) fn combo_display_name(combo_names: &[String], idx: usize) -> String {
    match combo_names.get(idx) {
        Some(name) if !name.trim().is_empty() => name.clone(),
        _ => format!("C{}", idx),
    }
}

pub(super) fn key_override_names_path(device_name: &str) -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("entropy");
    std::fs::create_dir_all(&dir).ok();
    let slug = device_id_slug(device_name);
    dir.join(format!("key_override_names_{}.json", slug))
}

pub(super) fn load_key_override_names(device_name: &str) -> Vec<String> {
    let path = key_override_names_path(device_name);
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(v) = serde_json::from_str::<Vec<String>>(&data) {
            return v;
        }
    }
    vec![]
}

pub(super) fn save_key_override_names(names: &[String], device_name: &str) {
    if let Ok(data) = serde_json::to_string(names) {
        let path = key_override_names_path(device_name);
        if let Err(e) = std::fs::write(&path, &data) {
            log::warn!("save_key_override_names failed at {:?}: {e}", path);
        } else {
            log::info!("save_key_override_names ok → {:?}", path);
        }
    }
}

pub(super) fn key_override_display_name(key_override_names: &[String], idx: usize) -> String {
    match key_override_names.get(idx) {
        Some(name) if !name.trim().is_empty() => name.clone(),
        _ => format!("KO{}", idx),
    }
}

pub(super) fn alt_repeat_names_path(device_name: &str) -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("entropy");
    std::fs::create_dir_all(&dir).ok();
    let slug = device_id_slug(device_name);
    dir.join(format!("alt_repeat_names_{}.json", slug))
}

pub(super) fn load_alt_repeat_names(device_name: &str) -> Vec<String> {
    let path = alt_repeat_names_path(device_name);
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(v) = serde_json::from_str::<Vec<String>>(&data) {
            return v;
        }
    }
    vec![]
}

pub(super) fn save_alt_repeat_names(names: &[String], device_name: &str) {
    if let Ok(data) = serde_json::to_string(names) {
        let path = alt_repeat_names_path(device_name);
        if let Err(e) = std::fs::write(&path, &data) {
            log::warn!("save_alt_repeat_names failed at {:?}: {e}", path);
        } else {
            log::info!("save_alt_repeat_names ok → {:?}", path);
        }
    }
}

pub(super) fn macro_custom_name(macro_names: &[String], idx: usize) -> Option<String> {
    macro_names
        .get(idx)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

pub(super) fn macro_display_name(macro_names: &[String], idx: usize) -> String {
    macro_custom_name(macro_names, idx).unwrap_or_else(|| format!("M{}", idx))
}

pub(super) fn tap_dance_custom_name(tap_dance_names: &[String], idx: usize) -> Option<String> {
    tap_dance_names
        .get(idx)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

pub(super) fn tap_dance_display_name(tap_dance_names: &[String], idx: usize) -> String {
    tap_dance_custom_name(tap_dance_names, idx).unwrap_or_else(|| format!("TD{}", idx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_expander_rules_json_parses_plain_array() {
        let data = r#"[
            {"enabled": true, "trigger": ":hello", "replacement": "Привет"}
        ]"#;

        let rules = parse_text_expansion_rules_json(data).unwrap();

        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].trigger, ":hello");
        assert_eq!(rules[0].replacement, "Привет");
    }

    #[test]
    fn text_expander_rules_json_parses_wrapped_rules_key() {
        let data = r#"{
            "rules": [
                {"enabled": true, "trigger": ":sig", "replacement": "Best\\n$|$Regards"}
            ]
        }"#;

        let rules = parse_text_expansion_rules_json(data).unwrap();

        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].trigger, ":sig");
        assert_eq!(rules[0].replacement, "Best\\n$|$Regards");
    }

    #[test]
    fn text_expander_rules_json_parses_legacy_settings_key() {
        let data = r#"{
            "text_expansion_rules": [
                {"enabled": false, "trigger": ":off", "replacement": "Off"}
            ]
        }"#;

        let rules = parse_text_expansion_rules_json(data).unwrap();

        assert_eq!(rules.len(), 1);
        assert!(!rules[0].enabled);
        assert_eq!(rules[0].trigger, ":off");
    }
}
