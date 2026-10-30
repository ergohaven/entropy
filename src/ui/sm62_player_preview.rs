// Static player only: metrics and coverage come from the actual firmware C/LVGL.
#[derive(serde::Deserialize)]
struct Sm62PlayerFont {
    name: String,
    glyphs: Vec<[i32; 7]>,
    kern: Vec<[i32; 3]>,
}

fn sm62_player_fonts() -> &'static [Sm62PlayerFont] {
    static FONTS: std::sync::OnceLock<Vec<Sm62PlayerFont>> = std::sync::OnceLock::new();
    FONTS.get_or_init(|| serde_json::from_str(include_str!("../../assets/sm62/player-glyphs.json"))
        .expect("actual C/LVGL player metrics"))
}

fn paint_sm62_player_label(ui: &egui::Ui, screen: egui::Rect, rect: egui::Rect,
    text: &str, font_index: usize, align: egui::Align, color: Color32) {
    let font = &sm62_player_fonts()[font_index];
    let (name, bytes): (&'static str, &[u8]) = match font.name.as_str() {
        "regular_20" => ("sm62-player-regular-20", include_bytes!("../../assets/sm62/player-zen-regular_20.png")),
        "medium_20" => ("sm62-player-medium-20", include_bytes!("../../assets/sm62/player-zen-medium_20.png")),
        "regular_24" => ("sm62-player-regular-24", include_bytes!("../../assets/sm62/player-zen-regular_24.png")),
        "bold_24" => ("sm62-player-bold-24", include_bytes!("../../assets/sm62/player-zen-bold_24.png")),
        _ => unreachable!("embedded font"),
    };
    let texture = sm62_keymap_texture(ui.ctx(), name, bytes);
    let chars: Vec<_> = text.chars().collect();
    let glyphs: Vec<_> = chars.iter().enumerate().map(|(i, &c)| {
        let g = font.glyphs.iter().find(|g| g[0] == c as i32).expect("player ASCII glyph");
        let next = chars.get(i + 1).copied().unwrap_or('\0') as i32;
        let advance = font.kern.iter().find(|k| k[0] == c as i32 && k[1] == next).map_or(g[6], |k| k[2]);
        (g, advance)
    }).collect();
    let width: i32 = glyphs.iter().map(|(_, a)| a).sum();
    // LVGL centers with integer coordinates, and uses top-aligned font line boxes.
    let mut x = rect.left() + match align {
        egui::Align::Min => 0.0,
        egui::Align::Center => ((rect.width() as i32 - width) / 2) as f32,
        egui::Align::Max => rect.width() - width as f32,
    };
    let clip = screen_rect(screen, rect.left(), rect.top(), rect.width(), rect.height());
    let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(clip));
    for (g, advance) in glyphs {
        let &[_, slot, w, h, dx, dy, _] = g;
        if w > 0 && h > 0 {
            let uv = egui::Rect::from_min_size(
                egui::pos2((slot % 16 * 32) as f32 / 512.0, (slot / 16 * 40) as f32 / 240.0),
                egui::vec2(w as f32 / 512.0, h as f32 / 240.0));
            painter.image(texture.id(), screen_rect(screen, x + dx as f32, rect.top() + dy as f32, w as f32, h as f32), uv, color);
        }
        x += advance as f32;
    }
}

// Clock and player use the same actual C font metrics, label boxes and accent.
// Preserve the accepted player submission order (number, name, rule).
fn paint_sm62_layer_header(ui: &egui::Ui, screen: egui::Rect) {
    let label = |x, y, text, font, color| paint_sm62_player_label(ui, screen,
        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(160.0, 21.0)),
        text, font, egui::Align::Center, color);
    label(40.0, 17.0, "LAYER / 0", 0, Color32::from_rgb(193,193,192));
    label(40.0, 49.0, "MAIN", 1, Color32::from_rgb(255,248,231));
    ui.painter().rect_filled(screen_rect(screen,72.0,41.0,96.0,3.0), 0.0, Color32::from_rgb(242,219,149));
}

fn paint_sm62_language(ui: &egui::Ui, screen: egui::Rect, language: Option<&str>) {
    if let Some(language) = language {
        paint_sm62_player_label(ui, screen,
            egui::Rect::from_min_size(egui::pos2(180.0,243.0), egui::vec2(40.0,21.0)),
            language, 1, egui::Align::Max, Color32::from_rgb(255,248,231));
    }
}

fn paint_sm62_player(ui: &egui::Ui, screen: egui::Rect, language: Option<&str>) {
    let cream = Color32::from_rgb(255, 248, 231);
    let label = |x, y, w, h, text, font, align, color| paint_sm62_player_label(ui, screen,
        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h)), text, font, align, color);
    paint_sm62_layer_header(ui, screen);
    ui.painter().rect_filled(screen_rect(screen,20.0,121.0,5.0,17.0), 0.0, Color32::from_rgb(242,219,149));
    // Exact preview placeholders; never replace these with host metadata or firmware fallbacks.
    label(37.0,117.0,185.0,32.0,"SONG NAME",2,egui::Align::Min,cream);
    label(20.0,149.0,200.0,32.0,"ARTIST",3,egui::Align::Min,cream);
    ui.painter().rect_filled(screen_rect(screen,20.0,193.0,200.0,5.0),0.0,Color32::from_rgb(131,127,118));
    label(20.0,202.0,200.0,23.0,"--:-- / --:--",1,egui::Align::Max,Color32::from_rgb(146,146,145));
    paint_sm62_language(ui, screen, language);
}

#[cfg(test)]
mod player_bitmap_tests {
    use super::*;

    #[test]
    fn sm62_player_bitmap_metrics_geometry_and_scaling() {
        for scale in [1.0, 0.5, 0.825] {
            let ctx = egui::Context::default();
            let screen = egui::Rect::from_min_size(egui::pos2(11.0,13.0),egui::vec2(240.0,280.0)*scale);
            let output = crate::app::discard_texture_deltas(ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    paint_sm62_screen(ui,screen,&[1,0,20,1,1],0,false,0,Some("EN"),false,false,false);
                });
            }));
            let meshes: Vec<_> = output.shapes.iter().filter_map(|s| match &s.shape {
                egui::Shape::Mesh(m) => Some((s.clip_rect,m)), _ => None }).collect();
            assert_eq!(meshes.len(),38);
            // Independently exported actual C first-glyph positions. Bold artist is
            // left-aligned at x20, not centered; title begins at x37 + bearing1.
            for (i,x,y,w,h,cx,cy,cw,ch,color) in [
                (0,75.0,20.0,10.0,14.0,40.0,17.0,160.0,21.0,Color32::from_rgb(193,193,192)),
                (7,97.0,53.0,13.0,14.0,40.0,49.0,160.0,21.0,Color32::from_rgb(255,248,231)),
                (11,38.0,121.0,13.0,17.0,37.0,117.0,185.0,32.0,Color32::from_rgb(255,248,231)),
                (19,20.0,152.0,16.0,17.0,20.0,149.0,200.0,32.0,Color32::from_rgb(255,248,231)),
                (25,116.0,214.0,7.0,2.0,20.0,202.0,200.0,23.0,Color32::from_rgb(146,146,145)),
                (36,195.0,247.0,10.0,14.0,180.0,243.0,40.0,21.0,Color32::from_rgb(255,248,231)),
            ] {
                let (clip,mesh) = meshes[i];
                let expected = screen_rect(screen,x,y,w,h);
                assert_eq!(mesh.calc_bounds(), expected);
                assert_eq!(clip,screen_rect(screen,cx,cy,cw,ch));
                assert!(mesh.vertices.iter().all(|v|v.color == color));
            }
            assert_ne!(meshes[11].1.texture_id,meshes[19].1.texture_id,"regular title and bold artist require distinct true font atlases");
            assert_ne!(meshes[0].1.texture_id,meshes[7].1.texture_id,"regular layer and medium MAIN are different firmware fonts");
        }
    }

    /// Disconnected 1:1 real Glow render for comparison with actual C/LVGL.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires X11 and ENTROPY_PLAYER_RENDER_OUTPUT"]
    fn sm62_player_render_1to1_screenshot() {
        use winit::platform::x11::EventLoopBuilderExtX11;
        struct Fixture { frames: usize, path: std::path::PathBuf }
        impl eframe::App for Fixture {
            fn persist_egui_memory(&self) -> bool { false }
            fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
                let ctx = ui.ctx().clone();
                ctx.set_pixels_per_point(1.0);
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                    paint_sm62_screen(ui,egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(240.0,280.0)),
                        &[1,0,20,1,1],0,false,0,Some("EN"),false,false,false);
                });
                self.frames += 1;
                if self.frames == 5 {
                    let (path, close) = (self.path.clone(), ctx.clone());
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |image| {
                        assert_eq!(image.size,[240,280]);
                        let rgba: Vec<u8> = image.pixels.iter().flat_map(|p|p.to_array()).collect();
                        image::save_buffer(&path,&rgba,240,280,image::ColorType::Rgba8).unwrap();
                        close.send_viewport_cmd(egui::ViewportCommand::Close);
                    })));
                }
                assert!(self.frames < 120,"screenshot did not arrive");
                ctx.request_repaint();
            }
        }
        let path = std::path::PathBuf::from(std::env::var("ENTROPY_PLAYER_RENDER_OUTPUT").unwrap());
        let output = path.clone();
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([240.0,280.0]).with_resizable(false),
            renderer: eframe::Renderer::Glow,
            persist_window: false,
            event_loop_builder: Some(Box::new(|b| { b.with_any_thread(true); })),
            ..Default::default()
        };
        eframe::run_native("SM62 player parity fixture (no HID)",options,Box::new(move |_| {
            Ok(Box::new(Fixture {frames:0,path}))
        })).unwrap();
        assert!(output.is_file());
    }
}
