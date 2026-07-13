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
                // A native dialog steals window focus; when it closes, egui can
                // be left holding stale pointer/interaction state (a pointer that
                // never got its release), which wedges subsequent clicks — dead
                // menus. Drop that state and close the hover dropdown that opened
                // the dialog before continuing.
                self.close_top_dropdowns(ctx);
                ctx.input_mut(|i| i.pointer = egui::PointerState::default());
                ctx.memory_mut(|m| m.stop_text_input());
                ctx.request_repaint();
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
