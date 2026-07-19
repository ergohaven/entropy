use crate::device::{Device, DeviceManager};
use crate::firmware::FirmwareProtocol;

use crate::keyboard::{KeyboardLayout, LayoutOption, PhysicalEncoder, PhysicalKey};
use crate::keycode::{
    key_label_font_sizes, keycode_label_with_names_and_layout, keycode_tooltip, KeyLegendLayout,
};
use crate::keycode_picker::{KeycodePicker, KeycodeTab};
use egui::{Color32, FontId, RichText, Sense, Stroke, Vec2};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc;

#[path = "app_theme.rs"]
mod app_theme;
use app_theme::*;

#[path = "typing_trainer_words.rs"]
mod typing_trainer_words;

#[path = "app_state.rs"]
mod app_state;
pub use app_state::*;

#[path = "app_init.rs"]
mod app_init;

#[path = "app_storage.rs"]
mod app_storage;
use app_storage::*;
#[path = "app_update.rs"]
mod app_update;
use app_update::*;
#[path = "app_repaint.rs"]
mod app_repaint;
#[cfg(not(target_arch = "wasm32"))]
use app_repaint::*;
#[path = "entlayout.rs"]
mod entlayout;
#[path = "entsettings.rs"]
mod entsettings;

#[path = "ui/settings_units.rs"]
mod settings_units;
use settings_units::*;
#[path = "ui/settings_viewport.rs"]
mod settings_viewport;
use settings_viewport::*;
#[path = "ui/onboarding_tour_state.rs"]
mod onboarding_tour_state;
use onboarding_tour_state::*;
#[path = "ui/about_device.rs"]
mod about_device_ui;
#[path = "ui/alt_repeat_settings.rs"]
mod alt_repeat_settings_ui;
#[path = "ui/app_lifecycle.rs"]
mod app_lifecycle;
#[path = "ui/app_settings.rs"]
mod app_settings_ui;
#[path = "ui/auto_shift_settings.rs"]
mod auto_shift_settings_ui;
#[path = "ui/bluetooth_settings.rs"]
mod bluetooth_settings_ui;
#[path = "ui/combo_write.rs"]
mod combo_write;
#[cfg(not(target_arch = "wasm32"))]
use combo_write::ComboWriteTask;
#[path = "ui/combo_settings.rs"]
mod combo_settings_ui;
#[path = "ui/device_connect_apply.rs"]
mod device_connect_apply;
#[path = "ui/device_connect_task.rs"]
mod device_connect_task;
#[path = "ui/device_connection.rs"]
mod device_connection;
#[path = "ui/device_scan.rs"]
mod device_scan;
#[path = "ui/device_settings_helpers.rs"]
mod device_settings_helpers;
#[path = "ui/encoder_visibility_settings.rs"]
mod encoder_visibility_settings_ui;
#[path = "ui/grave_escape_settings.rs"]
mod grave_escape_settings_ui;
#[path = "ui/key_assignment.rs"]
mod key_assignment;
#[path = "ui/layer_operations.rs"]
mod layer_operations;
#[cfg(not(target_arch = "wasm32"))]
use layer_operations::LayerWriteTask;
use layer_operations::{LayerClipboard, LayerSnapshot};
#[path = "ui/key_override_settings.rs"]
mod key_override_settings_ui;
#[path = "ui/layer_led_settings.rs"]
mod layer_led_settings_ui;
#[path = "ui/layout_indicator.rs"]
mod layout_indicator;
#[path = "ui/layout_indicator_preview.rs"]
mod layout_indicator_preview;
#[path = "ui/layout_indicator_window.rs"]
mod layout_indicator_window;
#[path = "ui/layout_options_settings.rs"]
mod layout_options_settings_ui;
#[path = "ui/layout_shared.rs"]
mod layout_shared;
use layout_shared::*;
#[path = "ui/top_dropdown.rs"]
mod top_dropdown;
use top_dropdown::*;
#[path = "ui/layout_advanced_dropdown.rs"]
mod layout_advanced_dropdown;
#[path = "ui/layout_chrome.rs"]
mod layout_chrome;
#[path = "ui/layout_device_dropdown.rs"]
mod layout_device_dropdown;
#[path = "ui/layout_dropdowns.rs"]
mod layout_dropdowns;
#[path = "ui/layout_hints.rs"]
mod layout_hints;
#[path = "ui/layout_image_export.rs"]
mod layout_image_export;
#[path = "ui/layout_keyboard.rs"]
mod layout_keyboard;
#[path = "ui/layout_layer_switcher.rs"]
mod layout_layer_switcher;
#[path = "ui/layout_settings_dropdown.rs"]
mod layout_settings_dropdown;
#[path = "ui/layout_top_tabs.rs"]
mod layout_top_tabs;
#[path = "ui/layout_view.rs"]
mod layout_view;
#[path = "ui/live_features_settings.rs"]
mod live_features_settings_ui;
#[path = "ui/magic_settings.rs"]
mod magic_settings_ui;
#[path = "ui/matrix_tester.rs"]
mod matrix_tester;
#[path = "ui/module_settings.rs"]
mod module_settings_ui;
#[path = "ui/mouse_keys_settings.rs"]
mod mouse_keys_settings_ui;
#[path = "ui/onboarding_tour.rs"]
mod onboarding_tour;
#[path = "ui/rgb_settings.rs"]
mod rgb_settings_ui;
#[path = "ui/settings_shell.rs"]
mod settings_shell;
#[path = "ui/tap_hold_settings.rs"]
mod tap_hold_settings_ui;
#[path = "ui/text_expander_editor.rs"]
mod text_expander_editor;
#[path = "ui/text_expander_runtime.rs"]
mod text_expander_runtime;
#[path = "ui/text_expander_settings.rs"]
mod text_expander_settings_ui;
#[path = "ui/touchpad_settings.rs"]
mod touchpad_settings_ui;
#[path = "ui/typing_trainer.rs"]
mod typing_trainer_ui;
#[path = "ui/ui_scale.rs"]
mod ui_scale;
#[path = "ui/universal_symbols_setup.rs"]
mod universal_symbols_setup;
#[path = "vial/unlock.rs"]
mod vial_unlock;
#[path = "ui/window_lifecycle.rs"]
mod window_lifecycle;
