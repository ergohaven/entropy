use super::*;

fn visible(out: &egui::FullOutput) -> Vec<(String, egui::Pos2)> {
    out.shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) if s.clip_rect.contains(t.visual_bounding_rect().center()) => {
                Some((
                    t.galley.text().to_owned(),
                    t.visual_bounding_rect().center(),
                ))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn games_builtin_assignment_main_selector_and_persisted_reload() {
    use crate::application_layouts::{
        ApplicationLayoutCategory, DetectedApplication, DeviceApplicationLayouts,
    };
    for language in [
        crate::i18n::Language::English,
        crate::i18n::Language::Russian,
    ] {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings.language = language;
        let key = "games-offline-device".to_owned();
        let mut settings = DeviceApplicationLayouts::default();
        settings.provision_builtin_presets(None);
        settings.pending_builtin_auto_bind.clear();
        let custom = settings.create_category("My group").unwrap();
        let existing = settings.create_for_application(&DetectedApplication {
            executable: "existing-tool".into(),
            ..Default::default()
        });
        assert!(settings.set_layout_category_id(&existing, &custom));
        assert!(settings.rename_category("audio", "My audio"));
        assert!(settings.remove_category("video"));
        let previous = settings.clone();
        app.app_settings
            .application_layouts
            .insert(key.clone(), settings);
        app.app_settings
            .application_layouts
            .insert("other-device".into(), previous.clone());
        app.app_settings.last_application_layout_device_key = Some(key.clone());
        let label = if language == crate::i18n::Language::Russian {
            "Игры"
        } else {
            "Games"
        };
        let choices = app.application_layout_category_choices(language);
        assert!(
            choices
                .iter()
                .any(|(id, name)| id == "games" && name == label),
            "built-in Games must exist and be localized"
        );
        assert_eq!(
            ApplicationLayoutCategory::ALL
                .iter()
                .map(|c| c.id())
                .filter(|id| *id != "games")
                .collect::<Vec<_>>(),
            [
                "browsers",
                "development",
                "graphics",
                "video",
                "audio",
                "communication",
                "other"
            ]
        );
        assert!(
            !app.application_layout_editor_grouped_options(
                &app.application_layout_editor_options()
            )
            .iter()
            .any(|(id, _)| id == "games"),
            "empty groups stay hidden"
        );
        app.application_categories_open = true;
        app.application_categories_selected_id = Some("games".into());
        app.application_categories_name = label.into();
        let manager_ctx = egui::Context::default();
        let manager_frame = |app: &mut EntropyApp| {
            crate::app::discard_texture_deltas(manager_ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 650.),
                    )),
                    ..Default::default()
                },
                |ui| app.draw_application_categories_modal(ui.ctx()),
            ))
        };
        for _ in 0..3 {
            manager_frame(&mut app);
        }
        assert!(
            visible(&manager_frame(&mut app))
                .iter()
                .any(|(t, _)| t == label),
            "manager must render Games choice"
        );
        app.application_categories_open = false;
        app.open_application_picker(false);
        app.select_application_picker_choice(DetectedApplication {
            executable: "unlisted-game.exe".into(),
            display_name: "My Game".into(),
            ..Default::default()
        });
        let ctx = egui::Context::default();
        let frame = |app: &mut EntropyApp, events| {
            crate::app::discard_texture_deltas(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 650.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.draw_application_picker_v2(ui.ctx()),
            ))
        };
        let click = |app: &mut EntropyApp, pos| {
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
        for _ in 0..3 {
            frame(&mut app, vec![]);
        }
        let other = if language == crate::i18n::Language::Russian {
            "Другие"
        } else {
            "Other"
        };
        let pos = visible(&frame(&mut app, vec![]))
            .into_iter()
            .find(|(t, _)| t == other)
            .expect("picker category trigger")
            .1;
        click(&mut app, pos);
        let mut popup = frame(&mut app, vec![]);
        for _ in 0..12 {
            if visible(&popup).iter().any(|(t, _)| t == label) {
                break;
            }
            let anchor = visible(&popup).last().expect("visible popup row").1;
            popup = frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(anchor),
                    egui::Event::MouseWheel {
                        source: egui::MouseWheelSource::Unknown,
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0., -90.),
                        modifiers: egui::Modifiers::NONE,
                        phase: egui::TouchPhase::Move,
                    },
                ],
            );
            for _ in 0..8 {
                popup = frame(&mut app, vec![]);
            }
        }
        let pos = visible(&popup)
            .into_iter()
            .rev()
            .find(|(t, _)| t == label)
            .unwrap_or_else(|| panic!("Games assignment choice: {:?}", visible(&popup)))
            .1;
        click(&mut app, pos);
        assert_eq!(app.application_picker_category.id(), "games");
        let add = if language == crate::i18n::Language::Russian {
            "Добавить"
        } else {
            "Add"
        };
        let pos = visible(&frame(&mut app, vec![]))
            .into_iter()
            .find(|(t, _)| t == add)
            .unwrap()
            .1;
        click(&mut app, pos);
        assert!(!app.application_picker_open);
        let selected = app.app_settings.application_layouts[&key]
            .editor_layout_id
            .clone();
        assert_eq!(
            app.app_settings.application_layouts[&key].category_id_for_layout(
                &app.app_settings.application_layouts[&key].layouts[&selected]
            ),
            "games"
        );
        app.app_settings
            .application_layouts
            .get_mut(&key)
            .unwrap()
            .editor_layout_id = "default".into();
        app.application_layout_editor_active = true;
        let main_ctx = egui::Context::default();
        let main_frame = |app: &mut EntropyApp, events| {
            crate::app::discard_texture_deltas(main_ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 650.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.draw_layout_layer_switcher_and_hints(ui, 0., 0., 52.),
            ))
        };
        let main_click = |app: &mut EntropyApp, pos| {
            for pressed in [true, false] {
                main_frame(
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
        let pos = visible(&main_frame(&mut app, vec![]))
            .into_iter()
            .find(|(t, _)| t == "Default")
            .unwrap()
            .1;
        main_click(&mut app, pos);
        let pos = visible(&main_frame(&mut app, vec![]))
            .into_iter()
            .find(|(t, _)| t == label)
            .expect("populated Games group")
            .1;
        main_frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
        let pos = visible(&main_frame(&mut app, vec![]))
            .into_iter()
            .rev()
            .find(|(t, _)| t == "My Game")
            .expect("game submenu")
            .1;
        main_click(&mut app, pos);
        let persisted = crate::app::app_storage::load_app_settings();
        let restored = &persisted.application_layouts[&key];
        assert_eq!(restored.editor_layout_id, selected);
        assert_eq!(restored.active_layout_id, selected);
        assert_eq!(
            restored.category_id_for_layout(&restored.layouts[&selected]),
            "games"
        );
        for (id, layout) in &previous.layouts {
            assert_eq!(&restored.layouts[id], layout, "existing profile changed");
        }
        assert_eq!(restored.category_names, previous.category_names);
        assert_eq!(restored.removed_categories, previous.removed_categories);
        assert_eq!(persisted.application_layouts["other-device"], previous);
        assert_eq!(
            crate::application_layouts::application_layout_category_for_executable(
                "unlisted-game.exe"
            ),
            ApplicationLayoutCategory::Other,
            "no implicit game classification"
        );
    }
}
