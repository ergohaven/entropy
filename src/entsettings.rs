use super::*;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const ENTSETTINGS_FORMAT: &str = "entropy.app-settings";
const ENTSETTINGS_VERSION: u16 = 1;

#[derive(Clone, Serialize, Deserialize)]
struct EntSettingsFile {
    format: String,
    version: u16,
    created_by: String,
    created_at: String,
    settings: AppSettings,
    #[serde(default)]
    text_expander_extra_files: Vec<EntSettingsTextExpanderRuleFile>,
}

#[derive(Clone, Serialize, Deserialize)]
struct EntSettingsTextExpanderRuleFile {
    file_name: String,
    rules: Vec<crate::text_expander::TextExpansionRule>,
}

#[cfg(not(target_arch = "wasm32"))]
struct EntSettingsStoragePaths {
    app_settings: PathBuf,
    primary_rules: PathBuf,
    extra_rules_dir: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl EntSettingsStoragePaths {
    fn current() -> Self {
        Self {
            app_settings: app_settings_path(),
            primary_rules: text_expander_rules_path(),
            extra_rules_dir: text_expander_rules_dir(),
        }
    }

    fn extra_rules(&self, file_name: &str) -> PathBuf {
        self.extra_rules_dir.join(file_name)
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct EntSettingsWrite {
    target: PathBuf,
    contents: Vec<u8>,
    original: Option<Vec<u8>>,
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn export_entsettings_dialog(&mut self) {
        self.spawn_file_dialog(
            crate::app::file_dialog::FileDialogAction::ExportEntsettings,
            rfd::FileDialog::new()
                .add_filter("Entropy app settings", &["entsettings"])
                .set_file_name("entropy-app-settings.entsettings"),
            true,
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn write_entsettings_export(&mut self, path: &Path) {
        let bundle = self.entsettings_snapshot();
        match write_entsettings_file(path, &bundle, self.app_settings.language) {
            Ok(()) => {
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "entsettings.exported_app_settings",
                    &[("path", &path.display().to_string())],
                )
            }
            Err(e) => {
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "entsettings.export_app_settings_failed",
                    &[("error", &e.to_string())],
                )
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn import_entsettings_dialog(&mut self) {
        self.spawn_file_dialog(
            crate::app::file_dialog::FileDialogAction::ImportEntsettings,
            rfd::FileDialog::new().add_filter("Entropy app settings", &["entsettings"]),
            false,
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn begin_entsettings_import(&mut self, path: std::path::PathBuf) {
        self.pending_entsettings_import_path = Some(path);
        self.import_progress_started_at = None;
        self.import_progress_title = crate::i18n::tr_catalog(
            self.app_settings.language,
            "entsettings.importing_app_settings",
        )
        .into();
        self.import_progress_body = crate::i18n::tr_catalog(
            self.app_settings.language,
            "entsettings.applying_app_settings",
        )
        .into();
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn import_entsettings_from_path(
        &mut self,
        ctx: &egui::Context,
        path: &Path,
    ) -> Result<String> {
        let lang = self.app_settings.language;
        let data =
            std::fs::read_to_string(path).with_context(|| entsettings_read_error(lang, path))?;
        let bundle: EntSettingsFile = serde_json::from_str(&data).with_context(|| {
            crate::i18n::tr_catalog_format(
                lang,
                "entsettings.failed_to_parse",
                &[("path", &path.display().to_string())],
            )
        })?;
        validate_entsettings_file(&bundle, lang)?;
        let backup_path = write_entsettings_auto_backup(&self.entsettings_snapshot(), lang)?;
        self.apply_entsettings(ctx, bundle)?;
        Ok(crate::i18n::tr_catalog_format(
            lang,
            "entsettings.import_complete_report",
            &[
                ("path", &path.display().to_string()),
                ("backup", &backup_path.display().to_string()),
            ],
        ))
    }

    fn entsettings_snapshot(&self) -> EntSettingsFile {
        let mut settings = self.app_settings.clone();
        settings.ui_scale = clamp_ui_scale(settings.ui_scale);
        settings.text_expander_rule_files =
            normalize_text_expander_rule_files(&settings.text_expander_rule_files);

        let text_expander_extra_files = settings
            .text_expander_rule_files
            .iter()
            .filter_map(|file_name| {
                load_text_expansion_rules_from_path(&text_expander_extra_rules_path(file_name)).map(
                    |rules| EntSettingsTextExpanderRuleFile {
                        file_name: file_name.clone(),
                        rules,
                    },
                )
            })
            .collect();

        EntSettingsFile {
            format: ENTSETTINGS_FORMAT.to_owned(),
            version: ENTSETTINGS_VERSION,
            created_by: "Entropy".to_owned(),
            created_at: chrono::Local::now().to_rfc3339(),
            settings,
            text_expander_extra_files,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn apply_entsettings(&mut self, ctx: &egui::Context, bundle: EntSettingsFile) -> Result<()> {
        self.apply_entsettings_with_paths(ctx, bundle, &EntSettingsStoragePaths::current())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn apply_entsettings_with_paths(
        &mut self,
        ctx: &egui::Context,
        bundle: EntSettingsFile,
        paths: &EntSettingsStoragePaths,
    ) -> Result<()> {
        let mut settings = bundle.settings;
        settings.ui_scale = clamp_ui_scale(settings.ui_scale);
        settings.text_expander_rule_files =
            normalize_text_expander_rule_files(&settings.text_expander_rule_files);
        let language = self.app_settings.language;

        let mut writes = Vec::with_capacity(2 + bundle.text_expander_extra_files.len());
        writes.push(prepare_entsettings_write(
            paths.app_settings.clone(),
            &settings,
            language,
            "entsettings.failed_to_serialize",
        )?);
        writes.push(prepare_entsettings_write(
            paths.primary_rules.clone(),
            &settings.text_expansion_rules,
            language,
            "entsettings.failed_to_serialize_rules",
        )?);
        for file in &bundle.text_expander_extra_files {
            if let Some(file_name) = normalize_text_expander_rules_file_name(&file.file_name) {
                writes.push(prepare_entsettings_write(
                    paths.extra_rules(&file_name),
                    &file.rules,
                    language,
                    "entsettings.failed_to_serialize_rules",
                )?);
            }
        }

        write_entsettings_files(&writes, language)?;
        self.app_settings = settings;
        crate::ui_style::set_accent(self.app_settings.accent_color.color());
        ctx.set_zoom_factor(self.app_settings.ui_scale);
        self.sticky_layout_last_size = None;
        if !self.app_settings.layer_hover_preview {
            self.hover_layer = None;
        }
        #[cfg(target_os = "windows")]
        if !self.app_settings.minimize_to_tray_on_close {
            self.tray_icon = None;
        }
        self.text_expander_rules_signature =
            text_expander_rules_signature(&self.app_settings.text_expander_rule_files);
        self.sync_text_expander_runtime();
        ctx.request_repaint();
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn prepare_entsettings_write<T: Serialize>(
    target: PathBuf,
    value: &T,
    language: crate::i18n::Language,
    serialize_error_key: &'static str,
) -> Result<EntSettingsWrite> {
    let contents = serde_json::to_vec_pretty(value)
        .context(crate::i18n::tr_catalog(language, serialize_error_key))?;
    let original = match std::fs::read(&target) {
        Ok(contents) => Some(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| entsettings_read_error(language, &target));
        }
    };
    Ok(EntSettingsWrite {
        target,
        contents,
        original,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn write_entsettings_files(
    writes: &[EntSettingsWrite],
    language: crate::i18n::Language,
) -> Result<()> {
    for (index, write) in writes.iter().enumerate() {
        if let Err(error) = std::fs::write(&write.target, &write.contents) {
            let error =
                anyhow::Error::new(error).context(entsettings_write_error(language, &write.target));
            let rollback_errors = rollback_entsettings_writes(&writes[..=index]);
            return Err(with_entsettings_rollback_errors(
                error,
                rollback_errors,
                language,
            ));
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn with_entsettings_rollback_errors(
    error: anyhow::Error,
    rollback_errors: Vec<String>,
    language: crate::i18n::Language,
) -> anyhow::Error {
    if rollback_errors.is_empty() {
        error
    } else {
        let original_error = format!("{error:#}");
        error.context(crate::i18n::tr_catalog_format(
            language,
            "entsettings.failed_to_restore",
            &[
                ("error", &original_error),
                ("errors", &rollback_errors.join("; ")),
            ],
        ))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn rollback_entsettings_writes(writes: &[EntSettingsWrite]) -> Vec<String> {
    let mut errors = Vec::new();
    for write in writes.iter().rev() {
        let result = match &write.original {
            Some(original) => std::fs::write(&write.target, original),
            None => std::fs::remove_file(&write.target),
        };
        if let Err(error) = result {
            if write.original.is_some() || error.kind() != std::io::ErrorKind::NotFound {
                errors.push(format!("restore {}: {error}", write.target.display()));
            }
        }
    }
    errors
}

#[cfg(not(target_arch = "wasm32"))]
fn entsettings_read_error(language: crate::i18n::Language, path: &Path) -> String {
    crate::i18n::tr_catalog_format(
        language,
        "entsettings.failed_to_read",
        &[("path", &path.display().to_string())],
    )
}

fn entsettings_write_error(language: crate::i18n::Language, path: &Path) -> String {
    crate::i18n::tr_catalog_format(
        language,
        "entsettings.failed_to_write",
        &[("path", &path.display().to_string())],
    )
}

fn validate_entsettings_file(
    bundle: &EntSettingsFile,
    language: crate::i18n::Language,
) -> Result<()> {
    if bundle.format != ENTSETTINGS_FORMAT {
        bail!(
            "{}",
            crate::i18n::tr_catalog_format(
                language,
                "entsettings.unsupported_format",
                &[("format", &bundle.format)]
            )
        );
    }
    if bundle.version == 0 || bundle.version > ENTSETTINGS_VERSION {
        bail!(
            "{}",
            crate::i18n::tr_catalog_format(
                language,
                "entsettings.unsupported_version",
                &[("version", &bundle.version.to_string())]
            )
        );
    }
    Ok(())
}

fn write_entsettings_auto_backup(
    bundle: &EntSettingsFile,
    language: crate::i18n::Language,
) -> Result<PathBuf> {
    let base_dir = app_settings_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base_dir.join("backups");
    std::fs::create_dir_all(&dir).with_context(|| {
        crate::i18n::tr_catalog_format(
            language,
            "entsettings.failed_to_create_backup_dir",
            &[("path", &dir.display().to_string())],
        )
    })?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("auto-backup-app-settings-{stamp}.entsettings"));
    write_entsettings_file(&path, bundle, language)?;
    Ok(path)
}

fn write_entsettings_file(
    path: &Path,
    bundle: &EntSettingsFile,
    language: crate::i18n::Language,
) -> Result<()> {
    let json = serde_json::to_string_pretty(bundle).context(crate::i18n::tr_catalog(
        language,
        "entsettings.failed_to_serialize",
    ))?;
    std::fs::write(path, json).with_context(|| entsettings_write_error(language, path))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "entropy-entsettings-{name}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn failed_extra_rule_write_leaves_live_and_persisted_settings_unchanged() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        app.app_settings.language = crate::i18n::Language::English;
        let original_ui_scale = app.app_settings.ui_scale;
        let imported_ui_scale = if original_ui_scale < 1.5 { 1.75 } else { 1.0 };
        let root = test_dir("late-write-failure");
        let app_settings = root.join("app_settings.json");
        let primary_rules = root.join("text_expansion_rules.json");
        std::fs::write(&app_settings, "old app settings").unwrap();
        std::fs::write(&primary_rules, "old primary rules").unwrap();

        let mut bundle = app.entsettings_snapshot();
        bundle.settings.ui_scale = imported_ui_scale;
        bundle.settings.language = crate::i18n::Language::Russian;
        bundle.text_expander_extra_files = vec![EntSettingsTextExpanderRuleFile {
            file_name: "extra.json".to_owned(),
            rules: vec![crate::text_expander::TextExpansionRule {
                enabled: true,
                trigger: "audit".to_owned(),
                replacement: "atomic".to_owned(),
            }],
        }];
        let paths = EntSettingsStoragePaths {
            app_settings: app_settings.clone(),
            primary_rules: primary_rules.clone(),
            extra_rules_dir: root.join("missing-extra-rules-dir"),
        };

        let result = app.apply_entsettings_with_paths(&ctx, bundle, &paths);

        assert!(result.is_err());
        let error = result.unwrap_err().to_string();
        assert!(error.starts_with("failed to write "));
        assert!(!error.contains("не удалось записать"));
        assert_eq!(app.app_settings.ui_scale, original_ui_scale);
        assert_eq!(
            std::fs::read_to_string(&app_settings).unwrap(),
            "old app settings"
        );
        assert_eq!(
            std::fs::read_to_string(&primary_rules).unwrap(),
            "old primary rules"
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_error_message_keeps_the_original_write_failure() {
        let write_error = anyhow::anyhow!("disk full").context("failed to write settings");

        let error = with_entsettings_rollback_errors(
            write_error,
            vec!["restore settings: disk full".to_owned()],
            crate::i18n::Language::English,
        );
        let message = error.to_string();

        assert!(message.contains("failed to write settings: disk full"));
        assert!(message.contains("restore settings: disk full"));
    }

    #[test]
    fn successful_import_commits_all_files_before_updating_live_settings() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let root = test_dir("successful-commit");
        let extra_rules_dir = root.join("extra-rules");
        std::fs::create_dir_all(&extra_rules_dir).unwrap();
        let paths = EntSettingsStoragePaths {
            app_settings: root.join("app_settings.json"),
            primary_rules: root.join("text_expansion_rules.json"),
            extra_rules_dir,
        };
        let primary_rules = vec![crate::text_expander::TextExpansionRule {
            enabled: true,
            trigger: "primary".to_owned(),
            replacement: "committed".to_owned(),
        }];
        let extra_rules = vec![crate::text_expander::TextExpansionRule {
            enabled: true,
            trigger: "extra".to_owned(),
            replacement: "committed".to_owned(),
        }];
        let mut bundle = app.entsettings_snapshot();
        bundle.settings.ui_scale = 1.75;
        bundle.settings.text_expansion_rules = primary_rules.clone();
        bundle.settings.text_expander_rule_files = vec!["extra.json".to_owned()];
        bundle.text_expander_extra_files = vec![EntSettingsTextExpanderRuleFile {
            file_name: "extra.json".to_owned(),
            rules: extra_rules.clone(),
        }];

        app.apply_entsettings_with_paths(&ctx, bundle, &paths)
            .unwrap();

        let persisted_settings: AppSettings =
            serde_json::from_str(&std::fs::read_to_string(&paths.app_settings).unwrap()).unwrap();
        let persisted_primary_rules: Vec<crate::text_expander::TextExpansionRule> =
            serde_json::from_str(&std::fs::read_to_string(&paths.primary_rules).unwrap()).unwrap();
        let persisted_extra_rules: Vec<crate::text_expander::TextExpansionRule> =
            serde_json::from_str(
                &std::fs::read_to_string(paths.extra_rules("extra.json")).unwrap(),
            )
            .unwrap();
        let expected_ui_scale = clamp_ui_scale(1.75);
        assert_eq!(app.app_settings.ui_scale, expected_ui_scale);
        assert_eq!(persisted_settings.ui_scale, expected_ui_scale);
        assert_eq!(persisted_primary_rules, primary_rules);
        assert_eq!(persisted_extra_rules, extra_rules);

        std::fs::remove_dir_all(root).unwrap();
    }
}
