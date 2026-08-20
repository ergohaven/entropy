# Entropy eframe patch

This is the published `eframe` 0.34.3 crate with one native Glow integration patch.

All native Wayland viewports use a non-blocking swap interval. Mutter can stop
presenting any minimized or fully covered surface without reporting that state to
the Wayland client. A blocking `swap_buffers` call on either the root or a child
surface would otherwise stall the shared OpenGL context and every other viewport.

When VSync is requested, the Glow renderer preserves its cadence in software using
the monitor refresh rate, with a 60 Hz fallback. X11, Windows, macOS, and web
behavior are unchanged. Remove the patch after upstream resolves the underlying
multi-window Wayland blocking behavior tracked in `emilk/egui` issues 5145 and
5836.
