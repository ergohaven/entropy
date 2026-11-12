//! Single-profile portability. Import copies content into an explicit existing
//! target; it never changes app matching, selection, or global resources.
use super::*;
use crate::application_layouts::{
    ApplicationLayout, APPLICATION_LAYOUT_LAYER_COUNT, APPLICATION_LAYOUT_UNSET_KEYCODE,
    DEFAULT_APPLICATION_LAYOUT_ID,
};
use anyhow::{bail, Context, Result};
use std::path::Path;

fn validate_portable_application_layout(source: &ApplicationLayout) -> Result<()> {
    if source.layers.len() != APPLICATION_LAYOUT_LAYER_COUNT
        || source.layer_names.len() != APPLICATION_LAYOUT_LAYER_COUNT
        || source.encoder_stacks.len() != APPLICATION_LAYOUT_LAYER_COUNT
        || source
            .encoder_stacks
            .iter()
            .any(|stack| !matches!(stack.len(), 0 | 2..=4))
    {
        bail!("Invalid application layout dimensions or encoder stacks");
    }
    // Dynamic resources use global/device IDs. Only the fixed Prev/Next codes
    // verified in display_preview.rs::preview_key_icon are exempt; adjacent
    // custom codes remain rejected, not an entire QMK custom range.
    if source
        .layers
        .iter()
        .flatten()
        .copied()
        .chain(
            source
                .encoder_stacks
                .iter()
                .flatten()
                .flat_map(|action| [action.counter_clockwise, action.clockwise]),
        )
        .any(|code| {
            code >= 0x2000 && !matches!(code, 0x7E05 | 0x7E06 | APPLICATION_LAYOUT_UNSET_KEYCODE)
        })
    {
        bail!("Application layout uses device-specific actions (macros, Tap Dance, layers or custom keycodes). Safe resource remapping is not supported; nothing was written. Use the full app-settings backup for shared resources.");
    }
    Ok(())
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn application_layout_file_dialog(&mut self, id: String, export: bool) {
        self.commit_pending_application_layout_edits();
        let action = if export {
            crate::app::file_dialog::FileDialogAction::ExportApplicationLayout(id.clone())
        } else {
            crate::app::file_dialog::FileDialogAction::ImportApplicationLayout(id.clone())
        };
        let dialog = rfd::FileDialog::new()
            .add_filter("Entropy layout", &["entlayout"])
            .set_file_name(format!("{}.entlayout", device_id_slug(&id)));
        self.spawn_file_dialog(action, dialog, export);
    }

    pub(super) fn write_application_layout_export(&self, path: &Path, id: &str) -> Result<()> {
        let profile = self
            .application_layout_settings()
            .and_then(|settings| settings.layouts.get(id))
            .context("Application layout no longer exists")?
            .clone();
        if id == DEFAULT_APPLICATION_LAYOUT_ID {
            bail!("Use Export layout for Default");
        }
        validate_portable_application_layout(&profile)?;
        super::entlayout::write_application_entlayout(path, profile, self.app_settings.language)
    }

    pub(super) fn import_application_layout_from_path(
        &mut self,
        path: &Path,
        id: &str,
    ) -> Result<()> {
        let source = super::entlayout::read_application_entlayout(path)
            .context("Invalid application layout file (expected .entlayout)")?;
        if id == DEFAULT_APPLICATION_LAYOUT_ID {
            bail!("Use Import layout for Default; application imports require an explicit application target");
        }
        validate_portable_application_layout(&source)?;
        let key = self
            .application_layout_device_key()
            .context("Select a device with application layouts")?;
        // The namespace is produced by Device::application_layout_storage_key,
        // including while editing a retained offline device. The source profile
        // (or an already-corrupted target profile) cannot choose target geometry.
        let sm62 = self.selected_device.and_then(|index| self.device_manager.devices().get(index)).map_or_else(
            || key.ends_with("-sm62") || key.rsplit_once("-sm62-owner-").is_some_and(|(model, owner)| {
                !model.is_empty() && owner.len() == 40 && owner.bytes().all(|byte| byte.is_ascii_hexdigit())
            }),
            |device| device.is_ergohaven_sm62(),
        );
        let controls = if sm62 { 62 } else { crate::application_layouts::APPLICATION_LAYOUT_CONTROL_COUNT };
        if source.layers.iter().any(|layer| layer.len() != controls)
            || (sm62 && source.encoder_stacks.iter().any(|stack| !stack.is_empty()))
        {
            bail!("Application layout geometry does not match the target device; nothing was written");
        }
        let mut settings = self.app_settings.clone();
        let target = settings
            .application_layouts
            .get_mut(&key)
            .and_then(|settings| settings.layouts.get_mut(id))
            .context("Application target no longer exists")?;
        target.layers = source.layers;
        target.layer_names = source.layer_names;
        target.encoder_stacks = source.encoder_stacks;
        target.bump_revision();
        // Persist before publishing in memory; never touch the shared pictogram
        // library (save_app_settings intentionally writes that separate file).
        let destination = app_settings_path();
        let parent = destination.parent().context("Invalid settings path")?;
        std::fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        use std::io::Write;
        temporary.write_all(&serde_json::to_vec_pretty(&settings)?)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&destination)
            .context("Failed to save application layout")?;
        self.app_settings = settings;
        self.refresh_layer_picker_content_flags();
        Ok(())
    }
}
