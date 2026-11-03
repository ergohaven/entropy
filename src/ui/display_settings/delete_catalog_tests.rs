use super::*;
#[test]
fn full_ui_delete_catalog_approved_bitmap_is_selectable_in_both_languages() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for language in crate::i18n::Language::ALL {
        let (mut app, ctx, recorder) = app(true);
        click_label(&mut app, &ctx, "Пиктограммы");
        app.app_settings.language = language;
        let mut user = SavedPictogram {
            origin: PictogramOrigin::User,
            name: "Private user drawing".into(),
            color: [17, 81, 193],
            bitmap: vec![0; PICTOGRAM_BYTES],
        };
        user.bitmap[42] = 0x40;
        app.app_settings.saved_pictograms = vec![user];
        let saved = app.app_settings.saved_pictograms.clone();
        let name = crate::i18n::tr_catalog(language, "display_settings.pictogram_firmware_delete");
        // Real production Choose popup. Full translated caption uniquely filters
        // the native Delete key rather than a generic delete/eraser action.
        app.display_settings.pictograms.search_query = name.into();
        click_label(
            &mut app,
            &ctx,
            crate::i18n::tr_catalog(language, "display_settings.choose"),
        );
        assert!(egui::Popup::is_any_open(&ctx));
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        let tiles: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Rect(r)
                    if (r.rect.width() - 48.0).abs() < 0.1
                        && (r.rect.height() - 48.0).abs() < 0.1 =>
                {
                    Some(r.rect.center())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            tiles.len(),
            2,
            "actual Choose popup must contain None + exact native Delete: {language:?}"
        );
        let entry = pictogram_catalog(language)
            .iter()
            .find(|i| i.name == name)
            .expect("distinct localized Delete entry");
        assert_eq!(
            &entry.bitmap,
            include_bytes!("../../../tools/artwork/backspace-delete/delete-right.bin")
        );
        assert_eq!(
            entry.group,
            pictogram_group("display_settings.pictogram_firmware_backspace"),
            "Delete must stay beside Backspace in the existing group"
        );
        recorder.respond_with(test_pictogram_upload_responses(true, 0));
        click(&mut app, &ctx, tiles[1]);
        assert!(app.vial_hid_task_active());
        finish(&mut app, &ctx);
        assert_eq!(
            app.display_settings
                .pictograms
                .library
                .bitmap(PictogramKind::Macro, 0),
            Some(entry.bitmap.as_slice())
        );
        assert_eq!(
            app.display_settings.pictograms.source_levels,
            pictogram_bitmap_levels(&entry.bitmap)
        );
        assert_eq!(
            app.app_settings.saved_pictograms, saved,
            "catalog choice must not mutate the user library"
        );
        assert_eq!(
            recorder.requests().len(),
            9,
            "one production upload transaction"
        );
    }
}

#[test]
fn all_macropad_default_pictograms_survive_localized_catalog_deduplication() {
    let defaults: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../tools/artwork/catalog-baseline-61cefc8/macropad-defaults.json"
    ))
    .unwrap();
    assert_eq!(defaults.len(), 41);
    for language in crate::i18n::Language::ALL {
        let catalog = pictogram_catalog(language);
        for row in &defaults {
            let expected: Vec<u8> = row["bitmap"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| b.as_u64().unwrap() as u8)
                .collect();
            assert!(
                catalog.iter().any(|i| i.bitmap.as_slice() == expected),
                "firmware default {} is absent from actual {language:?} chooser",
                row["selector"]
            );
        }
        let backspace = catalog
            .iter()
            .position(|i| {
                BUILTIN_PICTOGRAM_KEYS[i.index] == "display_settings.pictogram_firmware_backspace"
            })
            .unwrap();
        let delete = catalog
            .iter()
            .position(|i| {
                BUILTIN_PICTOGRAM_KEYS[i.index] == "display_settings.pictogram_firmware_delete"
            })
            .unwrap();
        assert_eq!(
            delete,
            backspace + 1,
            "native Delete must be immediately after Backspace"
        );
        assert_eq!(catalog[delete].group, catalog[backspace].group);
    }
}

#[test]
fn full_ui_every_macropad_default_mask_can_be_chosen_without_losing_user_art() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../tools/artwork/catalog-baseline-61cefc8/macropad-defaults.json"
    ))
    .unwrap();
    let mut seen = std::collections::HashSet::new();
    let masks: Vec<Vec<u8>> = rows
        .iter()
        .filter_map(|row| {
            let bitmap: Vec<u8> = row["bitmap"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| b.as_u64().unwrap() as u8)
                .collect();
            seen.insert(bitmap.clone()).then_some(bitmap)
        })
        .collect();
    assert_eq!(masks.len(), 39);
    for language in crate::i18n::Language::ALL {
        let (mut app, ctx, recorder) = app(true);
        click_label(&mut app, &ctx, "Пиктограммы");
        app.app_settings.language = language;
        let saved = SavedPictogram {
            origin: PictogramOrigin::User,
            name: "Independent drawing".into(),
            color: [10, 80, 240],
            bitmap: vec![0x20; PICTOGRAM_BYTES],
        };
        app.app_settings.saved_pictograms = vec![saved.clone()];
        for bitmap in &masks {
            let entry = pictogram_catalog(language)
                .iter()
                .find(|i| i.bitmap.as_slice() == bitmap)
                .expect("source-backed firmware mask must be selectable");
            app.display_settings.pictograms.search_query = entry.name.clone();
            click_label(
                &mut app,
                &ctx,
                crate::i18n::tr_catalog(language, "display_settings.choose"),
            );
            // Let the real popup's size/position animation settle before
            // deriving the pointer coordinates (multiple search matches can
            // change its height and anchor).
            for _ in 0..12 {
                frame(&mut app, &ctx, vec![]);
            }
            let output = frame(&mut app, &ctx, vec![]);
            assert!(egui::Popup::is_any_open(&ctx));
            let search = entry.name.to_lowercase();
            let filtered: Vec<_> = pictogram_catalog(language)
                .iter()
                .filter(|i| i.name.to_lowercase().contains(&search))
                .collect();
            let target = filtered
                .iter()
                .position(|i| i.index == entry.index)
                .unwrap()
                + 1;
            let tile_specs = |output: &egui::FullOutput| -> Vec<(egui::Pos2, egui::Rect)> {
                output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Rect(r)
                            if (r.rect.width() - 48.0).abs() < 0.1
                                && (r.rect.height() - 48.0).abs() < 0.1 =>
                        {
                            Some((r.rect.center(), s.clip_rect))
                        }
                        _ => None,
                    })
                    .collect()
            };
            let mut specs = tile_specs(&output);
            assert_eq!(
                specs.len(),
                filtered.len() + 1,
                "actual chooser tiles for {}",
                entry.name
            );
            // Search may match icons in multiple existing groups. Scroll the
            // actual popup instead of clicking a clipped, merely emitted shape.
            for _ in 0..3 {
                let (pos, clip) = specs[target];
                if clip.contains(pos) {
                    break;
                }
                let pointer = egui::pos2(pos.x, clip.center().y);
                frame(
                    &mut app,
                    &ctx,
                    vec![
                        egui::Event::PointerMoved(pointer),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            phase: egui::TouchPhase::Move,
                            delta: egui::vec2(0.0, clip.center().y - pos.y),
                            modifiers: egui::Modifiers::NONE,
                            source: egui::MouseWheelSource::Unknown,
                        },
                    ],
                );
                for _ in 0..16 {
                    frame(&mut app, &ctx, vec![]);
                }
                specs = tile_specs(&frame(&mut app, &ctx, vec![]));
            }
            let (target_pos, clip) = specs[target];
            assert!(
                clip.contains(target_pos),
                "{} tile must be visible after real scrolling",
                entry.name
            );
            recorder.respond_with(test_pictogram_upload_responses(true, 0));
            click(&mut app, &ctx, target_pos);
            assert!(
                app.vial_hid_task_active(),
                "actual chooser did not submit {} ({language:?}), target {target}, status={}",
                entry.name,
                app.status_msg
            );
            finish(&mut app, &ctx);
            assert_eq!(
                app.display_settings
                    .pictograms
                    .library
                    .bitmap(PictogramKind::Macro, 0),
                Some(bitmap.as_slice()),
                "selected {}",
                entry.name
            );
            assert_eq!(app.app_settings.saved_pictograms, vec![saved.clone()]);
        }
        assert_eq!(
            recorder.requests().len(),
            masks.len() * 9,
            "one upload per actual chooser selection"
        );
    }
}

#[test]
fn renamed_page_key_captions_preserve_exact_legacy_stock_recognition() {
    for (old, key) in [
        (
            "Страница вверх",
            "display_settings.pictogram_firmware_page_up",
        ),
        (
            "Страница вниз",
            "display_settings.pictogram_firmware_page_down",
        ),
    ] {
        let index = BUILTIN_PICTOGRAM_KEYS
            .iter()
            .position(|candidate| *candidate == key)
            .unwrap();
        let mut icon = SavedPictogram {
            origin: PictogramOrigin::Legacy,
            name: old.into(),
            color: [84, 189, 191],
            bitmap: builtin_pictogram_bitmap(index).to_vec(),
        };
        assert!(is_known_migrated_stock_pictogram(&icon));
        icon.origin = PictogramOrigin::User;
        assert!(!is_known_migrated_stock_pictogram(&icon));
        icon.origin = PictogramOrigin::Legacy;
        icon.bitmap[24] ^= 1;
        assert!(!is_known_migrated_stock_pictogram(&icon));
    }
}

#[test]
fn catalog_append_keeps_every_existing_stock_index_and_pixel_unchanged() {
    let keys: Vec<String> = serde_json::from_str(include_str!(
        "../../../tools/artwork/catalog-baseline-61cefc8/keys.json"
    ))
    .unwrap();
    let bits = include_bytes!("../../../tools/artwork/catalog-baseline-61cefc8/bitmaps.bin");
    assert_eq!(bits.len(), keys.len() * PICTOGRAM_BYTES);
    assert_eq!(
        &BUILTIN_PICTOGRAM_KEYS[..keys.len()],
        keys.iter().map(String::as_str).collect::<Vec<_>>()
    );
    for (index, expected) in bits.chunks_exact(PICTOGRAM_BYTES).enumerate() {
        assert_eq!(
            builtin_pictogram_bitmap(index).as_slice(),
            expected,
            "existing catalog index {index} changed"
        );
    }
    for language in crate::i18n::Language::ALL {
        for key in [
            "display_settings.pictogram_firmware_backspace",
            "display_settings.pictogram_builtin_action_eraser",
            "display_settings.pictogram_firmware_layer_prev_icon",
            "display_settings.pictogram_firmware_layer_next_icon",
        ] {
            assert!(
                pictogram_catalog(language)
                    .iter()
                    .any(|i| BUILTIN_PICTOGRAM_KEYS[i.index] == key),
                "{key} disappeared in {language:?}"
            );
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires X11 and ENTROPY_DELETE_CHOOSER_SCREENSHOT"]
fn full_ui_delete_catalog_native_chooser_screenshot() {
    use winit::platform::x11::EventLoopBuilderExtX11;
    struct Fixture {
        app: EntropyApp,
        frames: usize,
        path: std::path::PathBuf,
    }
    impl eframe::App for Fixture {
        fn persist_egui_memory(&self) -> bool {
            false
        }
        fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
            let ctx = ui.ctx().clone();
            eframe::App::ui(&mut self.app, ui, frame);
            self.frames += 1;
            if self.frames == 5 {
                let (path, close) = (self.path.clone(), ctx.clone());
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                    egui::ScreenshotCallback::new(move |image| {
                        let rgba: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                        image::save_buffer(
                            &path,
                            &rgba,
                            image.size[0] as u32,
                            image.size[1] as u32,
                            image::ColorType::Rgba8,
                        )
                        .unwrap();
                        close.send_viewport_cmd(egui::ViewportCommand::Close);
                    }),
                ));
            }
            assert!(self.frames < 120);
            ctx.request_repaint();
        }
    }
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let path =
        std::path::PathBuf::from(std::env::var("ENTROPY_DELETE_CHOOSER_SCREENSHOT").unwrap());
    let output = path.clone();
    eframe::run_native(
        "Entropy — actual pictogram chooser (synthetic HID)",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 1100.0])
                .with_resizable(false),
            renderer: eframe::Renderer::Glow,
            persist_window: false,
            event_loop_builder: Some(Box::new(|b| {
                b.with_any_thread(true);
            })),
            ..Default::default()
        },
        Box::new(move |cc| {
            let (mut app, _, _) = app(true);
            let ctx = &cc.egui_ctx;
            let mut fonts = egui::FontDefinitions::default();
            for name in ["display_preview", "clock_montserrat", "emoji_preview"] {
                fonts.families.insert(
                    egui::FontFamily::Name(name.into()),
                    fonts.families[&egui::FontFamily::Proportional].clone(),
                );
            }
            ctx.set_fonts(fonts.clone());
            click_label(&mut app, ctx, "Пиктограммы");
            app.display_settings.pictograms.search_query.clear();
            click_label(&mut app, ctx, "Выбрать");
            assert!(egui::Popup::is_any_open(ctx));
            let catalog = pictogram_catalog(crate::i18n::Language::Russian);
            let backspace = catalog
                .iter()
                .position(|i| {
                    BUILTIN_PICTOGRAM_KEYS[i.index]
                        == "display_settings.pictogram_firmware_backspace"
                })
                .unwrap()
                + 1;
            let delete = catalog
                .iter()
                .position(|i| {
                    BUILTIN_PICTOGRAM_KEYS[i.index] == "display_settings.pictogram_firmware_delete"
                })
                .unwrap()
                + 1;
            let specs = |out: &egui::FullOutput| -> Vec<(egui::Pos2, egui::Rect)> {
                out.shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Rect(r)
                            if (r.rect.width() - 48.0).abs() < 0.1
                                && (r.rect.height() - 48.0).abs() < 0.1 =>
                        {
                            Some((r.rect.center(), s.clip_rect))
                        }
                        _ => None,
                    })
                    .collect()
            };
            for _ in 0..16 {
                frame(&mut app, ctx, vec![]);
            }
            for _ in 0..4 {
                let tiles = specs(&frame(&mut app, ctx, vec![]));
                assert_eq!(tiles.len(), catalog.len() + 1);
                let (pos, clip) = tiles[backspace];
                let (next, _) = tiles[delete];
                if clip.contains(pos) && clip.contains(next) {
                    break;
                }
                frame(
                    &mut app,
                    ctx,
                    vec![
                        egui::Event::PointerMoved(egui::pos2(pos.x, clip.center().y)),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            phase: egui::TouchPhase::Move,
                            delta: egui::vec2(0.0, clip.center().y - (pos.y + next.y) * 0.5),
                            modifiers: egui::Modifiers::NONE,
                            source: egui::MouseWheelSource::Unknown,
                        },
                    ],
                );
                for _ in 0..24 {
                    frame(&mut app, ctx, vec![]);
                }
            }
            let tiles = specs(&frame(&mut app, ctx, vec![]));
            assert!(
                tiles[backspace].1.contains(tiles[backspace].0)
                    && tiles[delete].1.contains(tiles[delete].0),
                "native screenshot must show both adjacent keys"
            );
            // Preflight egui clicks do not go through the native GPU. Rebuild the
            // font atlas on its first native frame so its texture delta is uploaded.
            fonts.families.insert(
                egui::FontFamily::Name("native_fixture_reset".into()),
                fonts.families[&egui::FontFamily::Proportional].clone(),
            );
            ctx.set_fonts(fonts);
            Ok(Box::new(Fixture {
                app,
                frames: 0,
                path,
            }))
        }),
    )
    .unwrap();
    assert!(output.is_file());
}

#[test]
#[ignore = "requires ENTROPY_ICON_CATALOG_AUDIT_OUTPUT"]
fn export_actual_pictogram_catalog_for_firmware_audit() {
    let path = std::env::var("ENTROPY_ICON_CATALOG_AUDIT_OUTPUT").unwrap();
    let raw:Vec<_>=BUILTIN_PICTOGRAM_KEYS.iter().enumerate().map(|(index,key)|serde_json::json!({"index":index,"key":key,"bitmap":builtin_pictogram_bitmap(index).to_vec()})).collect();
    let catalog:Vec<_>=crate::i18n::Language::ALL.iter().map(|language|serde_json::json!({"language":format!("{language:?}"),"entries":pictogram_catalog(*language).iter().map(|i|serde_json::json!({"index":i.index,"key":BUILTIN_PICTOGRAM_KEYS[i.index],"name":i.name,"group":i.group,"bitmap":i.bitmap.to_vec()})).collect::<Vec<_>>()})).collect();
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&serde_json::json!({"raw":raw,"catalog":catalog})).unwrap(),
    )
    .unwrap();
}
