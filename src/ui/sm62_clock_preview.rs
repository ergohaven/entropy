#[cfg(test)]
mod clock_player_parity_tests {
    use super::*;

    /// Disconnected native 1:1 fixture; fixed clock samples exist only here.
    /// Production still samples chrono::Local and never reads fixture env vars.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires X11 and ENTROPY_CLOCK_RENDER_OUTPUT"]
    fn sm62_clock_render_1to1_screenshot() {
        use winit::platform::x11::EventLoopBuilderExtX11;
        struct Fixture {
            frames: usize,
            path: std::path::PathBuf,
            live: bool,
            date: bool,
            language: Option<String>,
        }
        impl eframe::App for Fixture {
            fn persist_egui_memory(&self) -> bool { false }
            fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
                let ctx = ui.ctx().clone();
                ctx.set_pixels_per_point(1.0);
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0,280.0));
                    // Identical production background and owning bitmap painters,
                    // without fabricated production time/date or HID activity.
                    ui.painter().rect_filled(screen, display_corner_radius(screen.width()), Color32::from_rgb(7,7,5));
                    paint_sm62_layer_header(ui, screen);
                    if self.live {
                        paint_sm62_clock_time(ui, screen, "12:34");
                        if self.date { paint_sm62_date(ui, screen, "14.10.2026"); }
                    } else {
                        paint_sm62_clock_fallback(ui, screen);
                    }
                    paint_sm62_language(ui, screen, self.language.as_deref());
                });
                self.frames += 1;
                if self.frames == 5 {
                    let (path, close) = (self.path.clone(), ctx.clone());
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |image| {
                        assert_eq!(image.size, [240,280]);
                        let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                        image::save_buffer(&path, &rgba, 240, 280, image::ColorType::Rgba8).unwrap();
                        close.send_viewport_cmd(egui::ViewportCommand::Close);
                    })));
                }
                assert!(self.frames < 120, "screenshot did not arrive");
                ctx.request_repaint();
            }
        }
        let path = std::path::PathBuf::from(std::env::var("ENTROPY_CLOCK_RENDER_OUTPUT").unwrap());
        let scene = std::env::var("ENTROPY_CLOCK_RENDER_SCENE").unwrap_or_else(|_| "fallback".into());
        assert!(matches!(scene.as_str(), "fallback" | "live" | "live-nodate"));
        let language = std::env::var("ENTROPY_CLOCK_RENDER_LANGUAGE").unwrap_or_else(|_| "EN".into());
        assert!(matches!(language.as_str(), "EN" | "RU" | "hidden"));
        let output = path.clone();
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([240.0,280.0]).with_resizable(false),
            renderer: eframe::Renderer::Glow,
            persist_window: false,
            event_loop_builder: Some(Box::new(|b| { b.with_any_thread(true); })),
            ..Default::default()
        };
        eframe::run_native("SM62 clock parity fixture (no HID)", options, Box::new(move |_| {
            Ok(Box::new(Fixture { frames: 0, path, live: scene != "fallback", date: scene == "live",
                language: (language != "hidden").then_some(language) }))
        })).unwrap();
        assert!(output.is_file());
    }

    #[test]
    fn sm62_clock_player_shared_pixels_do_not_jump() {
        for scale in [0.5, 0.825, 1.0] {
            for side in [0, 1] {
                for language in [None, Some("EN"), Some("RU")] {
                    for (stars, framed) in [(false,false), (false,true), (true,false), (true,true)] {
                        let ctx = egui::Context::default();
                        let mut fonts = egui::FontDefinitions::default();
                        fonts.families.insert(egui::FontFamily::Name("display_preview".into()),
                            fonts.families[&egui::FontFamily::Proportional].clone());
                        ctx.set_fonts(fonts);
                        let screen = egui::Rect::from_min_size(egui::pos2(11.0, 13.0),
                            egui::vec2(240.0, 280.0) * scale);
                        // Same context as the real mode switch: texture IDs stay stable.
                        let render = |mode, live| {
                            let output = crate::app::discard_texture_deltas(ctx.run_ui(egui::RawInput::default(), |ui| {
                                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                                    if framed {
                                        ui.set_width(240.0 * scale + 14.0);
                                        draw_sm62_preview(ui, &[mode,0,20,1,1], 0, true,
                                            280.0 * scale + 14.0, side, language, stars, false, live);
                                    } else {
                                        paint_sm62_screen(ui, screen, &[mode,0,20,1,1], 0,
                                            true, side, language, stars, false, live);
                                    }
                                });
                            }));
                            let screen = if framed {
                                output.shapes.iter().find_map(|s| match &s.shape {
                                    egui::Shape::Rect(r) if r.stroke.color == Color32::from_gray(75) => Some(r.rect.shrink(7.0)),
                                    _ => None,
                                }).expect("real preview frame")
                            } else { screen };
                            assert!((screen.width() - 240.0 * scale).abs() < 0.001);
                            let top = screen_rect(screen,0.0,0.0,240.0,80.0);
                            let bottom = screen_rect(screen,0.0,240.0,240.0,40.0);
                            output.shapes.iter().filter(|s| {
                                let bounds = s.shape.visual_bounding_rect();
                                bounds.intersects(top) || bounds.intersects(bottom)
                                    || matches!(s.shape, egui::Shape::Circle(_))
                            }).cloned().collect::<Vec<_>>()
                        };
                        let player = render(1, false);
                        for live in [false, true] {
                            assert_eq!(render(0, live), player,
                                "shared header/accent/language/frame/stars: scale={scale}, side={side}, language={language:?}, stars={stars}, live={live}, framed={framed}");
                        }
                    }
                }
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct Sm62ClockFont { size: usize, glyphs: Vec<[i32; 7]> }
fn sm62_clock_font(size: usize) -> &'static Sm62ClockFont {
    static FONTS: std::sync::OnceLock<Vec<Sm62ClockFont>> = std::sync::OnceLock::new();
    FONTS.get_or_init(|| serde_json::from_str(include_str!("../../assets/sm62/clock-glyphs.json")).expect("clock glyph metrics"))
        .iter().find(|f| f.size == size).unwrap()
}
// LVGL rounds each 1/16px advance, then aligns the complete string in its label.
fn sm62_clock_text_width(text: &str, size: usize) -> f32 {
    text.chars().filter_map(|c| sm62_clock_font(size).glyphs.iter().find(|g| g[0] == c as i32)).map(|g| g[6] as f32).sum()
}
fn paint_sm62_clock_label(ui: &egui::Ui, screen: egui::Rect, text: &str, size: usize, x: f32, y: f32, width: f32, align: u8, color: Color32) {
    let font = sm62_clock_font(size);
    let texture = if size == 48 {
        sm62_keymap_texture(ui.ctx(), "sm62-unbounded-48", include_bytes!("../../assets/sm62/clock-unbounded-48.png"))
    } else {
        sm62_keymap_texture(ui.ctx(), "sm62-unbounded-16", include_bytes!("../../assets/sm62/clock-unbounded-16.png"))
    };
    let advance = sm62_clock_text_width(text,size);
    let mut x = x + if align == 2 { width - advance } else if align == 1 { ((width - advance) / 2.0).floor() } else { 0.0 };
    for c in text.chars() {
        if let Some(&[_,slot,w,h,dx,dy,advance]) = font.glyphs.iter().find(|g| g[0] == c as i32) {
            let uv = egui::Rect::from_min_size(egui::pos2((slot % 8 * 64) as f32 / 512.0, (slot / 8 * 64) as f32 / 128.0),egui::vec2(w as f32 / 512.0,h as f32 / 128.0));
            ui.painter().image(texture.id(),screen_rect(screen,x + dx as f32,y + dy as f32,w as f32,h as f32),uv,color);
            x += advance as f32;
        }
    }
}
fn paint_sm62_clock_time(ui: &egui::Ui, screen: egui::Rect, time: &str) {
    let (hours,minutes) = time.split_once(':').unwrap_or(("SM","62"));
    let cream = Color32::from_rgb(255,248,231);
    let gold = Color32::from_rgb(242,219,149);
    paint_sm62_clock_label(ui,screen,hours,48,7.0,124.0,98.0,2,cream);
    paint_sm62_clock_label(ui,screen,":",48,110.0,124.0,20.0,1,gold);
    paint_sm62_clock_label(ui,screen,minutes,48,132.0,124.0,84.0,0,cream);
}
// Mirror the accepted firmware's visible-bounds centering, using the original
// Unbounded48 atlas/metrics. Live clock geometry is deliberately unchanged.
fn paint_sm62_clock_fallback(ui: &egui::Ui, screen: egui::Rect) {
    let glyph = |c: char| sm62_clock_font(48).glyphs.iter().find(|g| g[0] == c as i32).unwrap();
    let left = 7.0 + 98.0 - sm62_clock_text_width("SM", 48) + glyph('S')[4] as f32;
    let right = 132.0 + glyph('6')[6] as f32 + glyph('2')[4] as f32 + glyph('2')[2] as f32;
    let shift = ((240.0 - (right - left)) / 2.0).floor() - left;
    let cream = Color32::from_rgb(255,248,231);
    let gold = Color32::from_rgb(242,219,149);
    paint_sm62_clock_label(ui,screen,"SM",48,7.0+shift,124.0,98.0,2,cream);
    paint_sm62_clock_label(ui,screen,":",48,110.0+shift,124.0,20.0,1,gold);
    paint_sm62_clock_label(ui,screen,"62",48,132.0+shift,124.0,84.0,0,cream);
}
fn paint_sm62_date(ui: &egui::Ui, screen: egui::Rect, date: &str) {
    paint_sm62_clock_label(ui,screen,date,16,50.0,170.0,140.0,1,Color32::from_rgb(193,193,192));
}
