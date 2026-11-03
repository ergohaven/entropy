// Static reset-focus view of screen_keymap.c, not live matrix telemetry.
// Left matrix columns 4,3,2 map QWE; right columns 0,1,2 map YUI.
const LEFT_LEGENDS: [[&str; 3]; 3] = [["Q", "W", "E"], ["A", "S", "D"], ["Z", "X", "C"]];
const RIGHT_LEGENDS: [[&str; 3]; 3] = [["Y", "U", "I"], ["H", "J", "K"], ["N", "M", ","]];
const LEFT_SECONDARY: [[&str; 3]; 3] = [["Й", "Ц", "У"], ["Ф", "Ы", "В"], ["Я", "Ч", "С"]];
const RIGHT_SECONDARY: [[&str; 3]; 3] = [["Н", "Г", "Ш"], ["Р", "О", "Л"], ["Т", "Ь", "Б"]];

fn sm62_keymap_texture(ctx: &egui::Context, name: &'static str, bytes: &[u8]) -> egui::TextureHandle {
    let id = egui::Id::unique(name);
    if let Some(texture) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) { return texture; }
    let image = image::load_from_memory(bytes).expect("embedded firmware bitmap").into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let texture = ctx.load_texture(name, egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()), egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    texture
}

fn paint_sm62_keymap_glyph(ui: &egui::Ui, screen: egui::Rect, x: f32, y: f32, text: &str, size: usize, color: Color32) {
    #[derive(serde::Deserialize)]
    struct Font { size: usize, glyphs: Vec<[i32; 7]> }
    static FONTS: std::sync::OnceLock<Vec<Font>> = std::sync::OnceLock::new();
    let fonts = FONTS.get_or_init(|| serde_json::from_str(include_str!("../../assets/sm62/keymap-glyphs.json")).expect("firmware glyph metrics"));
    let font = fonts.iter().find(|f| f.size == size).unwrap();
    let texture = if size == 28 {
        sm62_keymap_texture(ui.ctx(), "sm62-zen-28", include_bytes!("../../assets/sm62/keymap-zen-28.png"))
    } else {
        sm62_keymap_texture(ui.ctx(), "sm62-zen-20", include_bytes!("../../assets/sm62/keymap-zen-20.png"))
    };
    let mut x = x;
    for c in text.chars() {
        if let Some(&[_, slot, w, h, dx, dy, advance]) = font.glyphs.iter().find(|g| g[0] == c as i32) {
            let uv = egui::Rect::from_min_size(egui::pos2((slot % 16 * 32) as f32 / 512.0, (slot / 16 * 40) as f32 / 440.0), egui::vec2(w as f32 / 512.0, h as f32 / 440.0));
            ui.painter().image(texture.id(), screen_rect(screen, x + dx as f32, y + dy as f32, w as f32, h as f32), uv, color);
            x += advance as f32;
        }
    }
}

fn paint_sm62_keymap(ui: &egui::Ui, screen: egui::Rect, cyrillic: bool, pressed: bool, side: usize) {
    let scale = screen.width() / 240.0;
    let bg = Color32::from_rgb(7, 7, 5);
    let fg = Color32::from_rgb(131, 127, 118);
    let accent = Color32::from_rgb(242, 219, 149);
    let (latin, secondary) = if side == 0 { (&LEFT_LEGENDS, &LEFT_SECONDARY) } else { (&RIGHT_LEGENDS, &RIGHT_SECONDARY) };
    for row in 0..3 {
        for col in 0..3 {
            let x = 10.0 + col as f32 * 75.0;
            let y = 30.0 + row as f32 * 75.0;
            let active = pressed && row == 1 && col == 1;
            ui.painter().rect_filled(screen_rect(screen, x, y, 69.0, 69.0), 7.0 * scale, if active { accent } else { bg });
            let color = if active { bg } else { fg };
            paint_sm62_keymap_glyph(ui, screen, x + 9.0, y + 4.0, latin[row][col], 28, color);
            if cyrillic { paint_sm62_keymap_glyph(ui, screen, x + 38.0, y + 40.0, secondary[row][col], 20, color); }
            // Firmware shifted label: width25, right aligned at x39, Zen20.
            // '<' advance is 12px in the exported font.
            if side == 1 && row == 2 && col == 2 {
                paint_sm62_keymap_glyph(ui, screen, x + 52.0, y + 5.0, "<", 20, color);
            }
        }
    }
    // Open outer edges: only internal segmented separators are visible.
    for boundary in 1..3 {
        for row in 0..3 {
            ui.painter().rect_filled(screen_rect(screen, 6.0 + boundary as f32 * 75.0,
                [30.0, 103.0, 178.0][row], 2.0, [73.0, 75.0, 72.0][row]), scale, fg);
        }
        for col in 0..3 {
            ui.painter().rect_filled(screen_rect(screen, [10.0, 83.0, 158.0][col],
                [26.0, 101.0, 176.0, 251.0][boundary], [73.0, 75.0, 73.0][col], 2.0), scale, fg);
        }
    }
    let junction = sm62_keymap_texture(ui.ctx(), "sm62-grid-junction", include_bytes!("../../assets/sm62/keymap-grid-junction.png"));
    for y in [92.0, 167.0] { for x in [72.0, 147.0] {
        ui.painter().image(junction.id(), screen_rect(screen, x, y, 20.0, 20.0), egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), fg);
    } }
}
