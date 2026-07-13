use super::*;

/// Which follow-up action to run once a background file dialog returns a path.
///
/// The native pickers on Linux go through the xdg-desktop-portal over D-Bus,
/// and `rfd`'s blocking API drives that to completion on the calling thread.
/// Running it on the egui/winit UI thread freezes the whole app for the
/// duration and corrupts input state afterwards (menus stop responding), so
/// every dialog runs on a worker thread and delivers its result here.
#[derive(Clone, Copy, Debug)]
pub enum FileDialogAction {
    ImportEntlayout,
    ExportEntlayout,
    ImportEntsettings,
    ExportEntsettings,
    ExportLayoutImage,
}

impl EntropyApp {
    /// Spawn a native file dialog on a worker thread. `save` picks a save
    /// dialog, otherwise an open dialog. Only one dialog runs at a time; a new
    /// request while one is in flight is ignored.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn spawn_file_dialog(
        &mut self,
        action: FileDialogAction,
        dialog: rfd::FileDialog,
        save: bool,
    ) {
        if self.pending_file_dialog.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = if save {
                dialog.save_file()
            } else {
                dialog.pick_file()
            };
            let _ = tx.send(result);
        });
        self.pending_file_dialog = Some((action, rx));
    }

    /// Poll the in-flight file dialog, if any, and dispatch its result.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_file_dialog(&mut self, ctx: &egui::Context) {
        let Some((action, rx)) = &self.pending_file_dialog else {
            return;
        };
        match rx.try_recv() {
            Ok(result) => {
                let action = *action;
                self.pending_file_dialog = None;
                if let Some(path) = result {
                    self.handle_file_dialog_result(action, path, ctx);
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.pending_file_dialog = None;
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_file_dialog_result(
        &mut self,
        action: FileDialogAction,
        path: std::path::PathBuf,
        ctx: &egui::Context,
    ) {
        match action {
            FileDialogAction::ImportEntlayout => self.begin_entlayout_import(path),
            FileDialogAction::ExportEntlayout => self.write_entlayout_export(&path),
            FileDialogAction::ImportEntsettings => self.begin_entsettings_import(path),
            FileDialogAction::ExportEntsettings => self.write_entsettings_export(&path),
            FileDialogAction::ExportLayoutImage => self.write_layout_image_export(path, ctx),
        }
    }
}
