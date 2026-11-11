//! Entropy patch: on Linux the root window can go away while the app keeps
//! running, as a window hidden to a tray does in Qt and GTK.
//!
//! winit cannot hide a window on Wayland (`set_visible` does nothing there), so
//! `ViewportCommand::Visible(false)` on the root drops its window, surface and
//! egui-winit state instead, and `Visible(true)` creates them again. This is the
//! teardown Android already gets on `suspended` (`GlutinWindowContext::on_suspend`),
//! applied to the root alone and on demand. The GL context, the painter with its
//! textures and the egui context stay, and the app keeps getting `App::logic`
//! passes while the root has no window. X11 takes the same path, so a window
//! hidden to the tray leaves the task bar there too.

use std::sync::Mutex;

use egui::ViewportCommand;

static ROOT_ACTIVATION_TOKEN: Mutex<Option<String>> = Mutex::new(None);

/// Hands eframe the XDG activation token to create the root window with the
/// next time it shows again, such as the one a tray icon got with its click.
/// Without one, the compositor may show the window without focus.
pub fn set_root_activation_token(token: String) {
    if let Ok(mut slot) = ROOT_ACTIVATION_TOKEN.lock() {
        *slot = Some(token);
    }
}

pub(crate) fn take_root_activation_token() -> Option<String> {
    ROOT_ACTIVATION_TOKEN.lock().ok()?.take()
}

/// Takes the visibility commands out of `commands`; the last one wins.
pub(crate) fn take_visibility(commands: &mut Vec<ViewportCommand>) -> Option<bool> {
    let mut visible = None;
    commands.retain(|command| match command {
        ViewportCommand::Visible(value) => {
            visible = Some(*value);
            false
        }
        _ => true,
    });
    visible
}

/// Whether `commands` leave the window hidden.
pub(crate) fn hides(commands: &[ViewportCommand]) -> bool {
    commands
        .iter()
        .rev()
        .find_map(|command| match command {
            ViewportCommand::Visible(visible) => Some(!visible),
            _ => None,
        })
        .unwrap_or(false)
}
