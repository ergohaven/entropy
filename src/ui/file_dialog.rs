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

/// A new dialog may start only when none is in flight — the single-slot
/// invariant that stops overlapping portal round-trips from racing each other.
#[cfg(not(target_arch = "wasm32"))]
fn can_start_file_dialog(dialog_in_flight: bool) -> bool {
    !dialog_in_flight
}

/// The decision a single `poll_file_dialog` step makes for one `try_recv`
/// outcome. Split out from the method so the whole lifecycle — success, cancel,
/// stale device, and a lost worker — is testable without a live dialog thread or
/// an egui context.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, PartialEq, Eq)]
pub(super) enum FileDialogPoll {
    /// No result yet; keep waiting.
    Pending,
    /// The user closed the dialog without choosing a file.
    Cancelled,
    /// A path was chosen, but the active device changed meanwhile; discard it.
    StaleDevice,
    /// A path was chosen and is safe to dispatch.
    Dispatch(std::path::PathBuf),
    /// The worker vanished (e.g. panicked) without ever sending a result.
    WorkerLost,
}

/// Classify one channel poll into the next lifecycle step. `Pending` is the only
/// non-terminal outcome; every other result frees the dialog slot.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn classify_file_dialog_poll(
    recv: Result<Option<std::path::PathBuf>, std::sync::mpsc::TryRecvError>,
    action: FileDialogAction,
    opened_generation: u64,
    current_generation: u64,
) -> FileDialogPoll {
    match recv {
        Ok(Some(path)) => {
            if dialog_result_stale(action, opened_generation, current_generation) {
                FileDialogPoll::StaleDevice
            } else {
                FileDialogPoll::Dispatch(path)
            }
        }
        Ok(None) => FileDialogPoll::Cancelled,
        Err(std::sync::mpsc::TryRecvError::Empty) => FileDialogPoll::Pending,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => FileDialogPoll::WorkerLost,
    }
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

    use std::path::PathBuf;
    use std::sync::mpsc::TryRecvError;

    #[test]
    fn only_one_dialog_runs_at_a_time() {
        // No dialog in flight → a new one may start; one already running → not.
        assert!(can_start_file_dialog(false));
        assert!(!can_start_file_dialog(true));
    }

    #[test]
    fn poll_pending_keeps_waiting_without_freeing_the_slot() {
        let poll = classify_file_dialog_poll(
            Err(TryRecvError::Empty),
            FileDialogAction::ImportEntlayout,
            5,
            5,
        );
        assert_eq!(poll, FileDialogPoll::Pending);
    }

    #[test]
    fn poll_success_dispatches_the_chosen_path() {
        let path = PathBuf::from("/tmp/layout.entlayout");
        let poll = classify_file_dialog_poll(
            Ok(Some(path.clone())),
            FileDialogAction::ImportEntlayout,
            5,
            5,
        );
        assert_eq!(poll, FileDialogPoll::Dispatch(path));
    }

    #[test]
    fn poll_cancel_frees_the_slot_without_dispatching() {
        let poll = classify_file_dialog_poll(Ok(None), FileDialogAction::ExportLayoutImage, 5, 5);
        assert_eq!(poll, FileDialogPoll::Cancelled);
    }

    #[test]
    fn poll_stale_device_discards_a_device_scoped_result() {
        // Device changed (5 → 6) while a device-scoped picker was open.
        let poll = classify_file_dialog_poll(
            Ok(Some(PathBuf::from("/tmp/b.entlayout"))),
            FileDialogAction::ExportEntlayout,
            5,
            6,
        );
        assert_eq!(poll, FileDialogPoll::StaleDevice);
    }

    #[test]
    fn poll_device_change_still_dispatches_app_settings() {
        // App-settings actions are device-independent, so a generation change
        // does not make the result stale.
        let path = PathBuf::from("/tmp/settings.entsettings");
        let poll = classify_file_dialog_poll(
            Ok(Some(path.clone())),
            FileDialogAction::ImportEntsettings,
            5,
            6,
        );
        assert_eq!(poll, FileDialogPoll::Dispatch(path));
    }

    #[test]
    fn poll_worker_disconnect_is_surfaced_not_swallowed() {
        let poll = classify_file_dialog_poll(
            Err(TryRecvError::Disconnected),
            FileDialogAction::ImportEntlayout,
            5,
            5,
        );
        assert_eq!(poll, FileDialogPoll::WorkerLost);
    }

    #[test]
    fn every_terminal_poll_outcome_frees_the_slot() {
        // Only `Pending` keeps the dialog slot occupied; all other outcomes are
        // terminal so the single-slot invariant can never wedge permanently.
        let terminal = [
            classify_file_dialog_poll(Ok(None), FileDialogAction::ImportEntlayout, 1, 1),
            classify_file_dialog_poll(
                Ok(Some(PathBuf::from("/tmp/x"))),
                FileDialogAction::ImportEntlayout,
                1,
                1,
            ),
            classify_file_dialog_poll(
                Ok(Some(PathBuf::from("/tmp/x"))),
                FileDialogAction::ExportEntlayout,
                1,
                2,
            ),
            classify_file_dialog_poll(
                Err(TryRecvError::Disconnected),
                FileDialogAction::ImportEntlayout,
                1,
                1,
            ),
        ];
        for outcome in terminal {
            assert_ne!(outcome, FileDialogPoll::Pending);
        }
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
        if !can_start_file_dialog(self.pending_file_dialog.is_some()) {
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
        let recv = rx.try_recv();
        match classify_file_dialog_poll(recv, action, generation, self.connection_generation) {
            FileDialogPoll::Pending => {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
            FileDialogPoll::Cancelled => {
                self.pending_file_dialog = None;
                self.recover_input_after_dialog(ctx);
            }
            FileDialogPoll::StaleDevice => {
                // The active device changed while the picker was open — otherwise
                // we could save device B's layout under device A's filename, or
                // program B with A's data. Discard the result.
                self.pending_file_dialog = None;
                self.recover_input_after_dialog(ctx);
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "status_messages.file_dialog_device_changed",
                )
                .into();
            }
            FileDialogPoll::Dispatch(path) => {
                self.pending_file_dialog = None;
                self.recover_input_after_dialog(ctx);
                self.handle_file_dialog_result(action, path);
            }
            FileDialogPoll::WorkerLost => {
                // Worker vanished (e.g. the dialog thread panicked) without ever
                // sending a result. Surface it instead of swallowing it, then run
                // the same input recovery so a lost dialog can't wedge the UI.
                log::error!("file dialog worker disconnected before returning a result");
                self.pending_file_dialog = None;
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "status_messages.file_dialog_worker_lost",
                )
                .into();
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
