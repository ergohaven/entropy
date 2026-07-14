use super::*;

/// Which follow-up action to run once a background file dialog returns a path.
///
/// The native pickers on Linux go through the xdg-desktop-portal over D-Bus,
/// and `rfd`'s blocking API drives that to completion on the calling thread.
/// Running it on the egui/winit UI thread freezes the whole app for the
/// duration and corrupts input state afterwards (menus stop responding), so
/// every dialog runs on a worker thread and delivers its result here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileDialogAction {
    ImportEntlayout,
    ExportEntlayout,
    ImportEntsettings,
    ExportEntsettings,
    ExportLayoutImage,
}

impl FileDialogAction {
    /// Whether the follow-up reads or writes the currently connected device.
    /// These must be rejected if the active device changed while the picker was
    /// open. App-settings import/export are device-independent.
    fn is_device_scoped(self) -> bool {
        match self {
            FileDialogAction::ImportEntlayout
            | FileDialogAction::ExportEntlayout
            | FileDialogAction::ExportLayoutImage => true,
            FileDialogAction::ImportEntsettings | FileDialogAction::ExportEntsettings => false,
        }
    }
}

/// A device-scoped operation captured at `opened_generation` is stale once the
/// active device changes to `current_generation`. Used both at dialog
/// completion and again just before the deferred `.entlayout` firmware write,
/// since the device can also change during that later gap.
pub(super) fn device_generation_stale(opened_generation: u64, current_generation: u64) -> bool {
    opened_generation != current_generation
}

/// Whether a completed dialog result must be discarded because the active device
/// changed while the picker was open. App-settings actions are never rejected.
fn dialog_result_stale(
    action: FileDialogAction,
    opened_generation: u64,
    current_generation: u64,
) -> bool {
    action.is_device_scoped() && device_generation_stale(opened_generation, current_generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_scoped_actions_are_classified_correctly() {
        assert!(FileDialogAction::ImportEntlayout.is_device_scoped());
        assert!(FileDialogAction::ExportEntlayout.is_device_scoped());
        assert!(FileDialogAction::ExportLayoutImage.is_device_scoped());
        assert!(!FileDialogAction::ImportEntsettings.is_device_scoped());
        assert!(!FileDialogAction::ExportEntsettings.is_device_scoped());
    }

    #[test]
    fn device_scoped_result_is_stale_only_when_generation_changed() {
        // Same generation: apply.
        assert!(!dialog_result_stale(
            FileDialogAction::ImportEntlayout,
            7,
            7
        ));
        assert!(!dialog_result_stale(
            FileDialogAction::ExportEntlayout,
            7,
            7
        ));
        // Device changed while picker open: reject.
        assert!(dialog_result_stale(FileDialogAction::ImportEntlayout, 7, 8));
        assert!(dialog_result_stale(
            FileDialogAction::ExportLayoutImage,
            7,
            8
        ));
    }

    #[test]
    fn app_settings_result_never_stale() {
        // App settings are device-independent, so a generation change is fine.
        assert!(!dialog_result_stale(
            FileDialogAction::ImportEntsettings,
            1,
            99
        ));
        assert!(!dialog_result_stale(
            FileDialogAction::ExportEntsettings,
            1,
            99
        ));
    }

    #[test]
    fn deferred_import_generation_check() {
        // The deferred .entlayout write in handle_pending_imports applies the
        // same rule: proceed only if the device is unchanged since the pick.
        assert!(!device_generation_stale(3, 3));
        assert!(device_generation_stale(3, 4));
    }
}

/// Owns the main window's raw handles so a file dialog can be parented to it.
/// Only ever used on the UI thread (where the window is alive) to hand the
/// handles to `rfd`'s `set_parent`, which copies them into the (Send) dialog.
#[cfg(not(target_arch = "wasm32"))]
struct ParentWindow {
    window: raw_window_handle::RawWindowHandle,
    display: raw_window_handle::RawDisplayHandle,
}

#[cfg(not(target_arch = "wasm32"))]
impl raw_window_handle::HasWindowHandle for ParentWindow {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        // Safe: the main window outlives this transient borrow used only to
        // build the parent identifier for the dialog.
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.window) })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl raw_window_handle::HasDisplayHandle for ParentWindow {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(self.display) })
    }
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn cache_parent_window_handles(&mut self, frame: &eframe::Frame) {
        use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
        if let Ok(handle) = frame.window_handle() {
            self.parent_window_handle = Some(handle.as_raw());
        }
        if let Ok(handle) = frame.display_handle() {
            self.parent_display_handle = Some(handle.as_raw());
        }
    }

    /// Spawn a native file dialog on a worker thread. `save` picks a save
    /// dialog, otherwise an open dialog. Only one dialog runs at a time; a new
    /// request while one is in flight is ignored.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn spawn_file_dialog(
        &mut self,
        action: FileDialogAction,
        mut dialog: rfd::FileDialog,
        save: bool,
    ) {
        if self.pending_file_dialog.is_some() {
            return;
        }
        // Parent the picker to the main window so it opens in front instead of
        // behind it. set_parent copies the raw handles into the Send dialog, so
        // the dialog can still run on the worker thread.
        if let (Some(window), Some(display)) =
            (self.parent_window_handle, self.parent_display_handle)
        {
            dialog = dialog.set_parent(&ParentWindow { window, display });
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
        self.pending_file_dialog = Some((action, self.connection_generation, rx));
    }

    /// Restore input state after a native dialog closes. The dialog steals
    /// window focus, and on return egui can be left holding a pointer that never
    /// got its release, which wedges subsequent clicks — dead menus. Drop that
    /// state and close the hover dropdown that launched the dialog.
    #[cfg(not(target_arch = "wasm32"))]
    fn recover_input_after_dialog(&mut self, ctx: &egui::Context) {
        self.close_top_dropdowns(ctx);
        ctx.input_mut(|i| i.pointer = egui::PointerState::default());
        ctx.memory_mut(|m| m.stop_text_input());
        ctx.request_repaint();
    }

    /// Poll the in-flight file dialog, if any, and dispatch its result.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_file_dialog(&mut self, ctx: &egui::Context) {
        let Some((action, generation, rx)) = &self.pending_file_dialog else {
            return;
        };
        let action = *action;
        let generation = *generation;
        match rx.try_recv() {
            Ok(result) => {
                self.pending_file_dialog = None;
                self.recover_input_after_dialog(ctx);
                let Some(path) = result else {
                    return;
                };
                // Reject a device-scoped result if the active device changed
                // while the picker was open — otherwise we could save device B's
                // layout under device A's filename, or program B with A's data.
                if dialog_result_stale(action, generation, self.connection_generation) {
                    self.status_msg =
                        "Device changed while the file dialog was open — please try again."
                            .to_owned();
                    return;
                }
                self.handle_file_dialog_result(action, path);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // Worker vanished (e.g. panicked). Clear state and run the same
                // input recovery so a lost dialog can't leave the UI wedged.
                self.pending_file_dialog = None;
                self.recover_input_after_dialog(ctx);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_file_dialog_result(&mut self, action: FileDialogAction, path: std::path::PathBuf) {
        match action {
            FileDialogAction::ImportEntlayout => self.begin_entlayout_import(path),
            FileDialogAction::ExportEntlayout => self.write_entlayout_export(&path),
            FileDialogAction::ImportEntsettings => self.begin_entsettings_import(path),
            FileDialogAction::ExportEntsettings => self.write_entsettings_export(&path),
            FileDialogAction::ExportLayoutImage => self.write_layout_image_export(path),
        }
    }
}
