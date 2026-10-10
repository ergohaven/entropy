use super::*;

pub(super) fn local_layout_visibility_key(base: &str, serial: Option<&str>) -> Option<String> {
    if base.is_empty() {
        return None;
    }
    Some(
        match serial.map(str::trim).filter(|serial| !serial.is_empty()) {
            Some(serial) => format!("{base}|serial:{}", serial.to_ascii_lowercase()),
            None => base.to_owned(),
        },
    )
}

impl LayoutElementVisibility {
    fn key_visible(&self, row: u8, col: u8, automatically_visible: bool, editing: bool) -> bool {
        automatically_visible && (editing || !self.hidden_keys.contains(&(row, col)))
    }

    fn encoder_visible(&self, index: u8, automatically_visible: bool, editing: bool) -> bool {
        automatically_visible && (editing || !self.hidden_encoders.contains(&index))
    }

    fn toggle_key(&mut self, row: u8, col: u8) {
        if !self.hidden_keys.remove(&(row, col)) {
            self.hidden_keys.insert((row, col));
        }
    }

    fn toggle_encoder(&mut self, index: u8) {
        if !self.hidden_encoders.remove(&index) {
            self.hidden_encoders.insert(index);
        }
    }
}

impl EntropyApp {
    pub(super) fn layout_element_visibility_device_key(&self) -> Option<String> {
        let base = self.current_encoder_visibility_id.as_str();
        let serial = self
            .selected_device
            .and_then(|index| self.device_manager.devices().get(index))
            .map(|device| device.serial_number.as_str());
        local_layout_visibility_key(base, serial)
    }

    pub(super) fn current_layout_element_visibility(&self) -> Option<&LayoutElementVisibility> {
        let device_key = self.layout_element_visibility_device_key()?;
        self.app_settings.layout_element_visibility.get(&device_key)
    }

    pub(super) fn main_layout_key_visible(
        &self,
        layout: &KeyboardLayout,
        key: &PhysicalKey,
    ) -> bool {
        self.layout_key_visible_with_edit_mode(layout, key, self.editing_layout_visibility)
    }

    pub(super) fn exported_layout_key_visible(
        &self,
        layout: &KeyboardLayout,
        key: &PhysicalKey,
    ) -> bool {
        self.layout_key_visible_with_edit_mode(layout, key, false)
    }

    fn layout_key_visible_with_edit_mode(
        &self,
        layout: &KeyboardLayout,
        key: &PhysicalKey,
        editing: bool,
    ) -> bool {
        let automatically_visible = Self::layout_key_visible(
            &self.module_settings,
            layout,
            key,
            self.layout_options_value,
        ) && Self::module_settings_encoder_press_key_encoder_idx(
            &self.module_settings,
            layout,
            key,
        )
        .is_none_or(|encoder_idx| {
            Self::encoder_visibility_allows(layout, encoder_idx, &self.encoder_visibility)
                && self
                    .current_layout_element_visibility()
                    .is_none_or(|visibility| visibility.encoder_visible(encoder_idx, true, editing))
        });
        self.current_layout_element_visibility()
            .map_or(automatically_visible, |visibility| {
                visibility.key_visible(key.row, key.col, automatically_visible, editing)
            })
    }

    pub(super) fn main_layout_encoder_visible(
        &self,
        layout: &KeyboardLayout,
        encoder: &PhysicalEncoder,
    ) -> bool {
        self.layout_encoder_visible_with_edit_mode(layout, encoder, self.editing_layout_visibility)
    }

    pub(super) fn exported_layout_encoder_visible(
        &self,
        layout: &KeyboardLayout,
        encoder: &PhysicalEncoder,
    ) -> bool {
        self.layout_encoder_visible_with_edit_mode(layout, encoder, false)
    }

    fn layout_encoder_visible_with_edit_mode(
        &self,
        layout: &KeyboardLayout,
        encoder: &PhysicalEncoder,
        editing: bool,
    ) -> bool {
        let automatically_visible = self.automatic_layout_encoder_visible(layout, encoder);
        self.current_layout_element_visibility()
            .map_or(automatically_visible, |visibility| {
                visibility.encoder_visible(encoder.encoder_idx, automatically_visible, editing)
            })
    }

    pub(super) fn automatic_layout_encoder_visible(
        &self,
        layout: &KeyboardLayout,
        encoder: &PhysicalEncoder,
    ) -> bool {
        Self::encoder_layout_condition_visible(layout, encoder, self.layout_options_value)
            && Self::module_settings_encoder_visible(
                &self.module_settings,
                layout,
                encoder.encoder_idx,
            )
            && Self::encoder_visibility_allows(
                layout,
                encoder.encoder_idx,
                &self.encoder_visibility,
            )
    }

    pub(super) fn main_layout_key_dimmed(&self, key: &PhysicalKey) -> bool {
        self.editing_layout_visibility
            && self
                .current_layout_element_visibility()
                .is_some_and(|visibility| visibility.hidden_keys.contains(&(key.row, key.col)))
    }

    pub(super) fn main_layout_encoder_dimmed(&self, index: u8) -> bool {
        self.editing_layout_visibility
            && self
                .current_layout_element_visibility()
                .is_some_and(|visibility| visibility.hidden_encoders.contains(&index))
    }

    pub(super) fn start_layout_visibility_edit(&mut self) {
        if self.layout.is_none() || self.current_encoder_visibility_id.is_empty() {
            return;
        }
        self.main_menu_tab = MainMenuTab::Keyboard;
        self.keycode_picker.open = false;
        self.selected_key = None;
        self.selected_encoder = None;
        self.hover_layer = None;
        self.jump_back_stack.clear();
        self.editing_layer = None;
        self.editing_layer_text.clear();
        self.editing_layer_focus_requested = false;
        self.editing_layer_layout_id = None;
        self.editing_layout_visibility = true;
    }

    pub(super) fn finish_layout_visibility_edit_on_input(&mut self, ctx: &egui::Context) -> bool {
        if !self.editing_layout_visibility {
            return false;
        }
        let esc_pressed = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        let right_clicked = ctx.input(|input| input.pointer.secondary_clicked());
        if !esc_pressed && !right_clicked {
            return false;
        }
        self.editing_layout_visibility = false;
        if esc_pressed {
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        }
        true
    }

    pub(super) fn toggle_layout_key_visibility(&mut self, index: usize) {
        let Some((row, col)) = self
            .layout
            .as_ref()
            .and_then(|layout| layout.keys.get(index))
            .map(|key| (key.row, key.col))
        else {
            return;
        };
        let Some(device_id) = self.layout_element_visibility_device_key() else {
            return;
        };
        self.app_settings
            .layout_element_visibility
            .entry(device_id)
            .or_default()
            .toggle_key(row, col);
        save_app_settings(&self.app_settings);
    }

    pub(super) fn toggle_layout_encoder_visibility(&mut self, index: u8) {
        let Some(device_id) = self.layout_element_visibility_device_key() else {
            return;
        };
        self.app_settings
            .layout_element_visibility
            .entry(device_id)
            .or_default()
            .toggle_encoder(index);
        save_app_settings(&self.app_settings);
    }
}

#[cfg(test)]
mod tests {
    use super::{AppSettings, EntropyApp, KeyboardLayout, LayoutElementVisibility, MainMenuTab};

    #[test]
    fn device_visibility_key_distinguishes_same_model_serials() {
        let base = "vial_1111_model";
        assert_ne!(
            super::local_layout_visibility_key(base, Some("serial-a")),
            super::local_layout_visibility_key(base, Some("serial-b"))
        );
        assert_eq!(
            super::local_layout_visibility_key(base, Some(" SERIAL-A ")),
            super::local_layout_visibility_key(base, Some("serial-a"))
        );
        assert_eq!(
            super::local_layout_visibility_key(base, None),
            Some(base.to_owned())
        );
        assert_eq!(super::local_layout_visibility_key("", None), None);
    }

    #[test]
    fn escape_and_right_click_exit_visibility_mode_before_layer_navigation() {
        let mut app = EntropyApp::new_inert_for_test();
        app.selected_layer = 4;
        app.jump_back_stack.push(2);
        let ctx = egui::Context::default();
        let mut handled = false;
        let _ = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                app.editing_layout_visibility = true;
                handled = app.finish_layout_visibility_edit_on_input(ui.ctx());
            },
        );
        assert!(handled);
        assert!(!app.editing_layout_visibility);
        assert_eq!(app.selected_layer, 4);
        assert_eq!(app.jump_back_stack, vec![2]);

        let ctx = egui::Context::default();
        let pos = egui::pos2(15.0, 15.0);
        app.editing_layout_visibility = true;
        for pressed in [true, false] {
            let _ = ctx.run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Secondary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| handled = app.finish_layout_visibility_edit_on_input(ui.ctx()),
            );
        }
        assert!(handled);
        assert!(!app.editing_layout_visibility);
        assert_eq!(app.selected_layer, 4);
        assert_eq!(app.jump_back_stack, vec![2]);
    }

    #[test]
    fn layout_menu_action_returns_from_settings_and_enters_edit_mode() {
        let mut app = EntropyApp::new_inert_for_test();
        app.layout = Some(serde_json::from_str::<KeyboardLayout>(
            r#"{"name":"Test","rows":1,"cols":1,"keys":[],"encoders":[],"layers":[],"encoder_layers":[],"custom_keycodes":[]}"#,
        ).unwrap());
        app.current_encoder_visibility_id = "vial_1111_test".to_owned();
        app.main_menu_tab = MainMenuTab::Settings;
        app.keycode_picker.open = true;
        app.start_layout_visibility_edit();
        assert!(app.main_menu_tab == MainMenuTab::Keyboard);
        assert!(app.editing_layout_visibility);
        assert!(!app.keycode_picker.open);
        assert!(app.selected_key.is_none());
    }

    #[test]
    fn visibility_edits_are_cosmetic_and_automatic_hiding_always_wins() {
        let mut visibility = LayoutElementVisibility::default();
        visibility.toggle_key(2, 4);
        visibility.toggle_encoder(2);
        assert!(!visibility.key_visible(2, 4, true, false));
        assert!(!visibility.encoder_visible(2, true, false));
        assert!(visibility.key_visible(2, 4, true, true));
        assert!(visibility.encoder_visible(2, true, true));
        assert!(!visibility.key_visible(2, 4, false, true));
        assert!(!visibility.encoder_visible(2, false, true));
        visibility.toggle_key(2, 4);
        visibility.toggle_encoder(2);
        assert!(visibility.key_visible(2, 4, true, false));
        assert!(visibility.encoder_visible(2, true, false));
    }

    #[test]
    fn main_canvas_hides_saved_elements_only_outside_edit_mode() {
        let layout: KeyboardLayout = serde_json::from_str(
            r#"{"name":"Test","rows":1,"cols":1,"keys":[{"x":0.0,"y":0.0,"w":1.0,"h":1.0,"row":0,"col":0,"label":"0,0","rotation":0.0,"rotation_x":0.0,"rotation_y":0.0}],"encoders":[{"x":2.0,"y":0.0,"w":1.0,"h":1.0,"label":"encoder","encoder_idx":0,"direction":0,"rotation":0.0,"rotation_x":0.0,"rotation_y":0.0}],"layers":[],"encoder_layers":[[1]],"custom_keycodes":[]}"#,
        ).unwrap();
        let mut layout = layout;
        layout.layers = vec![vec![1u16.into()]];
        let mut app = EntropyApp::new_inert_for_test();
        app.current_encoder_visibility_id = "vial_1111_test".to_owned();
        let visibility = app
            .app_settings
            .layout_element_visibility
            .entry(app.current_encoder_visibility_id.clone())
            .or_default();
        visibility.toggle_key(0, 0);
        visibility.toggle_encoder(0);
        assert!(!app.main_layout_key_visible(&layout, &layout.keys[0]));
        assert!(!app.main_layout_encoder_visible(&layout, &layout.encoders[0]));
        app.editing_layout_visibility = true;
        assert!(app.main_layout_key_visible(&layout, &layout.keys[0]));
        assert!(app.main_layout_encoder_visible(&layout, &layout.encoders[0]));
        assert!(app.main_layout_key_dimmed(&layout.keys[0]));
        assert!(app.main_layout_encoder_dimmed(0));
        assert_eq!(layout.layers[0][0].vial_keycode(), 1);
    }

    #[test]
    #[ignore = "run with an isolated XDG_CONFIG_HOME; clicks persist app settings"]
    fn canvas_clicks_toggle_a_key_and_an_encoder_without_keymap_writes() {
        let mut layout: KeyboardLayout = serde_json::from_str(
            r#"{"name":"Test","rows":1,"cols":1,"keys":[{"x":0.0,"y":0.0,"w":1.0,"h":1.0,"row":0,"col":0,"label":"0,0","rotation":0.0,"rotation_x":0.0,"rotation_y":0.0}],"encoders":[{"x":2.0,"y":0.0,"w":1.0,"h":1.0,"label":"encoder","encoder_idx":0,"direction":0,"rotation":0.0,"rotation_x":0.0,"rotation_y":0.0}],"layers":[],"encoder_layers":[[2]],"custom_keycodes":[]}"#,
        ).unwrap();
        layout.layers = vec![vec![4u16.into()]];
        let mut app = EntropyApp::new_inert_for_test();
        app.current_encoder_visibility_id = "isolated_visibility_click_test".to_owned();
        app.layout = Some(layout.clone());
        app.start_layout_visibility_edit();
        let ctx = egui::Context::default();
        let frame = |app: &mut EntropyApp, events| {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.draw_layout_keyboard_canvas(
                        ui,
                        &layout,
                        &ctx,
                        egui::vec2(900.0, 700.0),
                        100.0,
                        100.0,
                        80.0,
                        4.0,
                        80.0,
                    );
                },
            );
        };
        let click = |app: &mut EntropyApp, pos| {
            frame(app, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                frame(
                    app,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
        };
        click(&mut app, egui::pos2(140.0, 140.0));
        assert!(
            app.app_settings.layout_element_visibility["isolated_visibility_click_test"]
                .hidden_keys
                .contains(&(0, 0))
        );
        click(&mut app, egui::pos2(140.0, 140.0));
        assert!(
            !app.app_settings.layout_element_visibility["isolated_visibility_click_test"]
                .hidden_keys
                .contains(&(0, 0))
        );
        click(&mut app, egui::pos2(140.0, 140.0));
        click(&mut app, egui::pos2(300.0, 125.0));
        assert!(
            app.app_settings.layout_element_visibility["isolated_visibility_click_test"]
                .hidden_encoders
                .contains(&0)
        );
        click(&mut app, egui::pos2(300.0, 125.0));
        assert!(
            !app.app_settings.layout_element_visibility["isolated_visibility_click_test"]
                .hidden_encoders
                .contains(&0)
        );
        click(&mut app, egui::pos2(300.0, 125.0));
        app.editing_layout_visibility = false;
        assert!(!app.main_layout_key_visible(&layout, &layout.keys[0]));
        assert!(!app.main_layout_encoder_visible(&layout, &layout.encoders[0]));
        assert!(!app.keycode_picker.open);
        assert_eq!(layout.layers[0][0].vial_keycode(), 4);
        assert_eq!(layout.encoder_layers[0][0], 2);

        // K:03/Imperial44 render the press key inside an encoder circle.
        // Hiding that encoder must not leave the middle press key clickable.
        let mut combined_layout = layout.clone();
        combined_layout.name = "K03".to_owned();
        combined_layout.keys[0].x = 2.0;
        app.layout = Some(combined_layout.clone());
        app.app_settings
            .layout_element_visibility
            .get_mut("isolated_visibility_click_test")
            .unwrap()
            .hidden_keys
            .clear();
        assert!(app.main_layout_key_visible(&combined_layout, &combined_layout.keys[0]));
        let ctx = egui::Context::default();
        let pos = egui::pos2(300.0, 140.0);
        for pressed in [true, false] {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| {
                    app.draw_layout_keyboard_canvas(
                        ui,
                        &combined_layout,
                        &ctx,
                        egui::vec2(900.0, 700.0),
                        100.0,
                        100.0,
                        80.0,
                        4.0,
                        80.0,
                    );
                },
            );
        }
        assert!(!app.keycode_picker.open);
    }

    #[test]
    fn visibility_choices_round_trip_without_firmware_fields() {
        let mut visibility = LayoutElementVisibility::default();
        visibility.toggle_key(1, 8);
        visibility.toggle_encoder(1);
        let restored: LayoutElementVisibility =
            serde_json::from_str(&serde_json::to_string(&visibility).unwrap()).unwrap();
        assert_eq!(restored, visibility);
        assert_eq!(
            serde_json::from_str::<LayoutElementVisibility>("{}").unwrap(),
            LayoutElementVisibility::default()
        );
    }

    #[test]
    fn saved_visibility_is_scoped_to_device_and_survives_settings_reload() {
        let mut settings = AppSettings::default();
        settings
            .layout_element_visibility
            .entry("vial_1111_keyboard-a".to_owned())
            .or_default()
            .toggle_key(2, 4);
        settings
            .layout_element_visibility
            .entry("vial_2222_keyboard-b".to_owned())
            .or_default()
            .toggle_encoder(1);
        let reloaded: AppSettings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert!(reloaded.layout_element_visibility["vial_1111_keyboard-a"]
            .hidden_keys
            .contains(&(2, 4)));
        assert!(reloaded.layout_element_visibility["vial_2222_keyboard-b"]
            .hidden_encoders
            .contains(&1));
        assert!(reloaded.layout_element_visibility["vial_1111_keyboard-a"]
            .hidden_encoders
            .is_empty());
        assert!(reloaded.layout_element_visibility["vial_2222_keyboard-b"]
            .hidden_keys
            .is_empty());
        assert!(serde_json::from_str::<AppSettings>("{}")
            .unwrap()
            .layout_element_visibility
            .is_empty());
    }
}
