# Entropy eframe patch

This is the published `eframe` 0.34.3 crate with one native Glow integration patch.

Secondary Wayland viewports use a non-blocking swap interval. Mutter can stop
presenting a minimized or fully covered child surface without reporting that state
to the Wayland client. A blocking child `swap_buffers` call would otherwise stall
the shared OpenGL context and the root Entropy window.

The root viewport remains synchronized, and X11, Windows, macOS, and web behavior
are unchanged. Remove the patch after upstream resolves the underlying multi-window
Wayland blocking behavior tracked in `emilk/egui` issues 5145 and 5836.
