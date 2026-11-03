# Entropy eframe patch

This is the published `eframe` 0.37.0 crate with two native Glow integration patches.

## X11 window identity

egui-winit forwards `ViewportBuilder::app_id` on Wayland but leaves X11 `WM_CLASS`
to winit's `argv[0]` fallback, so an AppImage launch name would rename the app.
`create_window_attributes` in `glow_integration.rs` sets `WM_CLASS` from the app id.

## Linux: a root viewport without a window

Entropy hides its main window to the system tray. winit 0.30 cannot hide a window
on Wayland (`Window::set_visible` does nothing there), so on Linux
`ViewportCommand::Visible(false)` on the root drops its window, GL surface and
egui-winit state, and `Visible(true)` creates them again, as Qt and GTK do for a
window hidden to a tray. This is the teardown eframe already performs for every
window when Android suspends the app (`GlutinWindowContext::on_suspend`), applied
to the root alone and on demand. The GL context, the painter with its textures and
the egui context stay. X11 takes the same path.

- `native/linux_root.rs`: the visibility commands of the root, and
  `eframe::set_root_activation_token`, which hands over the XDG activation token a
  tray icon got with its click; the root window is created again with it.
- `glow_integration.rs`: `root_commands` applies the root's visibility commands
  from a full pass and from a logic-only pass alike; `hide_root_window` drops the
  window like `on_suspend` and keeps its size, position and maximized state in the
  builder; `initialize_all_windows` skips the hidden root; a root created again
  gets the activation token and, with `accesskit`, a new AccessKit adapter.
  Without a window, `run_hidden_root` gives the app `App::logic` passes
  (`EpiIntegration::update_logic_only`), as eframe does for an invisible window:
  no `App::ui`, no painting; texture changes wait in the pending deltas for the
  next painted pass. A close request reaches the app as egui-winit would deliver
  it; other root commands wait for the window. The pass that hides the root does
  not paint it, so a root hidden at launch never shows. `Frame` follows the root
  window and lets go of a dropped one, since winit destroys a window only with its
  last `Arc`; while there is no window it has neither a window nor a handle.
  `save_and_destroy` no longer requires one.
- `run.rs`: repaint requests of the hidden root schedule its next pass by time,
  throttled like an invisible window, since there is no `WindowId` to redraw. A
  close request of the hidden root ends the event loop itself: eframe otherwise
  ends it on the next window event, and there is no window left to send one.
- `winit_integration.rs`: `WinitApp::root_hidden` and `run_hidden_root`, no-ops
  for wgpu.

Child viewports close while the root is hidden, as their `ui` callbacks come from
the root's `App::ui`. Windows, macOS, wgpu and web behavior are unchanged; all of
it is under `cfg(target_os = "linux")`.

## Dropped with 0.37

The 0.34 patch also swapped Wayland surfaces without waiting and paced VSync in
software, since a compositor that stops presenting a hidden surface would block the
shared GL context. eframe 0.37 swaps with interval 0 on Wayland itself, paces
frames by the compositor's frame callbacks, and runs `App::logic` alone when a
`RedrawRequested` is 250 ms overdue (emilk/egui#8631).
