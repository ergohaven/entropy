use super::*;

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "display_settings/pipeline_tests.rs"]
mod pipeline_tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "display_settings/sm62_timeout_tests.rs"]
mod sm62_timeout_tests;

const DISPLAY_BUTTON_STYLE_MAX_ID: u8 = 32;
const DISPLAY_BUTTON_STYLE_IDS: [u8; 5] = [0, 6, 4, 2, 5];

fn format_rgb_hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

fn parse_rgb_hex(value: &str) -> Option<[u8; 3]> {
    let value = value.trim().strip_prefix('#').unwrap_or(value.trim());
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some([
        u8::from_str_radix(&value[0..2], 16).ok()?,
        u8::from_str_radix(&value[2..4], 16).ok()?,
        u8::from_str_radix(&value[4..6], 16).ok()?,
    ])
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn paint_monochrome_pictogram(
    ui: &egui::Ui,
    rect: egui::Rect,
    bitmap: &[u8],
    foreground: Color32,
    background: Color32,
) {
    if background != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 0.0, background);
    }
    if bitmap.len() < PICTOGRAM_BYTES {
        return;
    }

    let pixel_width = rect.width() / PICTOGRAM_WIDTH as f32;
    let pixel_height = rect.height() / PICTOGRAM_HEIGHT as f32;
    let mut mesh = egui::epaint::Mesh::default();
    for index in 0..PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT {
        if bitmap[index / 8] & (1 << (7 - index % 8)) == 0 {
            continue;
        }
        let column = index % PICTOGRAM_WIDTH;
        let row = index / PICTOGRAM_WIDTH;
        mesh.add_colored_rect(
            egui::Rect::from_min_max(
                egui::pos2(
                    rect.left() + column as f32 * pixel_width,
                    rect.top() + row as f32 * pixel_height,
                ),
                egui::pos2(
                    rect.left() + (column + 1) as f32 * pixel_width,
                    rect.top() + (row + 1) as f32 * pixel_height,
                ),
            ),
            foreground,
        );
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

#[cfg(not(target_arch = "wasm32"))]
fn pictogram_editor_line(from: (u8, u8), to: (u8, u8)) -> Vec<(u8, u8)> {
    let (mut x0, mut y0) = (i32::from(from.0), i32::from(from.1));
    let (x1, y1) = (i32::from(to.0), i32::from(to.1));
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut error = dx + dy;
    let mut cells = Vec::new();
    loop {
        cells.push((x0 as u8, y0 as u8));
        if x0 == x1 && y0 == y1 {
            break;
        }
        let twice = error * 2;
        if twice >= dy {
            error += dy;
            x0 += sx;
        }
        if twice <= dx {
            error += dx;
            y0 += sy;
        }
    }
    cells
}

#[cfg(not(target_arch = "wasm32"))]
fn pictogram_name_error(
    name: &str,
    saved: &[SavedPictogram],
    selected_saved: Option<usize>,
    builtin_names: &[String],
) -> Option<&'static str> {
    let name = name.trim();
    let mut characters = name.chars();
    if !characters.next().is_some_and(char::is_alphabetic) {
        return Some("display_settings.pictogram_name_format_error");
    }
    if !characters
        .all(|character| character.is_alphanumeric() || character == '_' || character == ' ')
    {
        return Some("display_settings.pictogram_name_format_error");
    }
    let normalized = name.to_lowercase();
    let keeps_existing_name = selected_saved
        .and_then(|index| saved.get(index))
        .is_some_and(|icon| icon.name.trim().to_lowercase() == normalized);
    if !keeps_existing_name
        && builtin_names
            .iter()
            .any(|builtin| builtin.trim().to_lowercase() == normalized)
    {
        return Some("display_settings.pictogram_name_duplicate_error");
    }
    if saved.iter().enumerate().any(|(index, pictogram)| {
        Some(index) != selected_saved && pictogram.name.trim().to_lowercase() == normalized
    }) {
        return Some("display_settings.pictogram_name_duplicate_error");
    }
    None
}

#[cfg(not(target_arch = "wasm32"))]
fn pictogram_is_builtin_selection(
    name: &str,
    selected: Option<usize>,
    builtin_names: &[String],
) -> bool {
    selected
        .and_then(|i| builtin_names.get(i))
        .is_some_and(|builtin| name.trim() == builtin)
}

#[cfg(not(target_arch = "wasm32"))]
fn visible_pictogram_color(_dark: bool, color: [u8; 3]) -> Color32 {
    Color32::from_rgb(color[0], color[1], color[2])
}

#[cfg(not(target_arch = "wasm32"))]
fn can_delete_saved_pictogram(selected_saved: Option<usize>) -> bool {
    selected_saved.is_some()
}

#[cfg(not(target_arch = "wasm32"))]
fn startup_image_reset_enabled(supported: bool, present: bool, busy: bool) -> bool {
    supported && present && !busy
}

#[cfg(not(target_arch = "wasm32"))]
fn draw_pictogram_library_tile(
    ui: &mut egui::Ui,
    dark: bool,
    scale: f32,
    bitmap: &[u8],
    color: [u8; 3],
    background: [u8; 3],
    tooltip: &str,
    selected: bool,
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(48.0 * scale, 48.0 * scale), Sense::click());
    let border = if selected {
        Color32::from_rgb(84, 189, 191)
    } else {
        crate::ui_style::border_color(dark)
    };
    ui.painter().rect(
        rect,
        7.0 * scale,
        Color32::from_rgb(background[0], background[1], background[2]),
        Stroke::new(if selected { 2.0_f32 } else { 1.0_f32 }, border),
        egui::StrokeKind::Inside,
    );
    let bitmap_rect = egui::Rect::from_center_size(
        rect.center(),
        egui::vec2(
            PICTOGRAM_WIDTH as f32 * scale,
            PICTOGRAM_HEIGHT as f32 * scale,
        ),
    );
    paint_monochrome_pictogram(
        ui,
        bitmap_rect,
        bitmap,
        visible_pictogram_color(dark, color),
        Color32::TRANSPARENT,
    );

    response.on_hover_text(tooltip).clicked()
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct CatalogIcon {
    index: usize,
    name: String,
    bitmap: [u8; PICTOGRAM_BYTES],
    group: usize,
}

// The indices are persistent identifiers in pictograms.json. Only the view is
// deduplicated and reordered; old references and saved user artwork stay valid.
#[cfg(not(target_arch = "wasm32"))]
fn pictogram_catalog(lang: crate::i18n::Language) -> &'static [CatalogIcon] {
    use std::sync::OnceLock;
    static ENGLISH: OnceLock<Vec<CatalogIcon>> = OnceLock::new();
    static RUSSIAN: OnceLock<Vec<CatalogIcon>> = OnceLock::new();
    let cache = match lang {
        crate::i18n::Language::English => &ENGLISH,
        crate::i18n::Language::Russian => &RUSSIAN,
    };
    cache.get_or_init(|| build_pictogram_catalog(lang))
}

#[cfg(not(target_arch = "wasm32"))]
fn pictogram_builtin_names(lang: crate::i18n::Language) -> &'static [String] {
    use std::sync::OnceLock;
    static ENGLISH: OnceLock<Vec<String>> = OnceLock::new();
    static RUSSIAN: OnceLock<Vec<String>> = OnceLock::new();
    let cache = match lang {
        crate::i18n::Language::English => &ENGLISH,
        crate::i18n::Language::Russian => &RUSSIAN,
    };
    cache.get_or_init(|| {
        BUILTIN_PICTOGRAM_KEYS
            .iter()
            .map(|key| {
                #[cfg(test)]
                NAME_LIST_LOOKUPS.with(|count| count.set(count.get() + 1));
                crate::i18n::tr_catalog(lang, key).to_owned()
            })
            .collect()
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn build_pictogram_catalog(lang: crate::i18n::Language) -> Vec<CatalogIcon> {
    use std::collections::HashSet;
    let mut names = HashSet::new();
    let mut bitmaps = HashSet::new();
    let mut icons = Vec::new();
    // Prefer the firmware's actual artwork for actions represented twice by
    // older hand-drawn editor icons. Exact bitmap/name checks below cannot
    // catch these: the two calculators, for example, have different pixels and
    // different tooltips but mean the same thing.
    for index in (38..BUILTIN_PICTOGRAM_KEYS.len()).chain(0..38) {
        let key = BUILTIN_PICTOGRAM_KEYS[index];
        if is_superseded_stock_pictogram(key) {
            continue;
        }
        let name = crate::i18n::tr_catalog(lang, key).to_owned();
        let bitmap = builtin_pictogram_bitmap(index);
        if !names.insert(name.trim().to_lowercase()) || !bitmaps.insert(bitmap) {
            continue;
        }
        icons.push(CatalogIcon {
            index,
            name,
            bitmap,
            group: pictogram_group(BUILTIN_PICTOGRAM_KEYS[index]),
        });
    }
    icons.sort_by(|a, b| {
        a.group.cmp(&b.group).then_with(|| {
            pictogram_sort_key(BUILTIN_PICTOGRAM_KEYS[a.index])
                .cmp(&pictogram_sort_key(BUILTIN_PICTOGRAM_KEYS[b.index]))
        })
    });
    icons
}

#[cfg(not(target_arch = "wasm32"))]
fn is_superseded_stock_pictogram(key: &str) -> bool {
    matches!(
        key,
        "display_settings.pictogram_builtin_mail"
            | "display_settings.pictogram_builtin_home"
            | "display_settings.pictogram_builtin_screenshot"
            | "display_settings.pictogram_builtin_mouse_button_left"
            | "display_settings.pictogram_builtin_mouse_button_middle"
            | "display_settings.pictogram_builtin_mouse_button_right"
            | "display_settings.pictogram_builtin_brightness_down"
            | "display_settings.pictogram_builtin_brightness_up"
            | "display_settings.pictogram_builtin_computer"
            | "display_settings.pictogram_builtin_web_search"
            | "display_settings.pictogram_builtin_calculator"
    )
}

// Older Entropy releases could put a stock snapshot in `user`. Recognize only
// an exact legacy stock name/bitmap revision, independent of display RGB.
// Explicit authorship wins; unknown pixels/names stay visible. Legacy files
// cannot distinguish an intentional identical copy from a promoted snapshot.
// Filtering never deletes data, so this ambiguity is reversible.
#[cfg(test)]
thread_local! {
    static NAME_LIST_LOOKUPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static SAVED_FILTER_RUNS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(not(target_arch = "wasm32"))]
fn visible_saved_pictograms<'a>(
    saved: &'a [SavedPictogram],
    _catalog: &[CatalogIcon],
) -> Vec<&'a SavedPictogram> {
    #[cfg(test)]
    SAVED_FILTER_RUNS.with(|count| count.set(count.get() + 1));
    let mut seen = std::collections::HashSet::new();
    saved
        .iter()
        .filter(|icon| {
            icon.bitmap.len() == PICTOGRAM_BYTES
                && !is_known_migrated_stock_pictogram(icon)
                // Only exact repeated records collapse. Matching a name or an
                // image alone must not hide custom drawings/colors/names.
                && seen.insert((icon.name.as_str(), icon.color, icon.bitmap.as_slice()))
        })
        .collect()
}

// Order within each existing group. Keep these lists independent of the group
// assignment rules: changing the order must not move an icon to another group.
#[cfg(not(target_arch = "wasm32"))]
fn pictogram_sort_key(key: &str) -> String {
    let id = key.rsplit('.').next().unwrap_or(key);
    let id = id
        .strip_prefix("pictogram_builtin_action_")
        .or_else(|| id.strip_prefix("pictogram_firmware_"))
        .or_else(|| id.strip_prefix("pictogram_integration_"))
        .or_else(|| id.strip_prefix("pictogram_builtin_"))
        .unwrap_or(id);
    const ORDER: [&str; 22] = [
        "layer_prev_icon layer_next_icon", "", "", "", "",
        "play pause stop",
        "files file_plus file_pen file_output folder_open folder_search download mail save printer cut copy paste copy_plus",
        "pen_tool pencil paintbrush pipette lasso crop eraser square_pen rectangle_horizontal circle frame image_plus layers layers_plus group ungroup blend eye eye_off rotate_ccw rotate_cw scaling circle_arrow_up circle_arrow_down book_open",
        "terminal square_terminal code message_square_code command git_branch bug bug_off component package_plus blocks keyboard",
        "arrow_left arrow_right arrow_up arrow_down arrow_up_to_line arrow_down_to_line",
        "home end page_up page_down insert bookmark history search web_search focus zoom_out zoom_in move_left move_right move_up move_down move_horizontal move_vertical move link panel_left panel_right panel_bottom view_front view_right view_top send_to_back control_panel",
        "undo redo select_all text_cursor_input type replace scissors eraser delete",
        "brightness_down brightness_up computer settings control_panel calculator screenshot lock",
        "single_tap double_tap hold plus maximize expand corner_up_left corner_up_right scan scan_line scan_dashed square_dashed list_filter list_restart route hand box bandage line_squiggle incognito bring_to_front backspace delete",
        "previous_track next_track skip_back skip_forward",
        "step_back step_forward flag flag_triangle_right",
        "volume_down volume volume_2 mute volume_x headphones",
        "microphone mic_off audio_lines camera",
        "mouse_left mouse_middle mouse_right mouse_button_left mouse_button_middle mouse_button_right mouse_up mouse_down wheel_up wheel_down mouse_pointer mouse_pointer_2",
        "", "task_new task_edit task_done task_repeat obs_record obs_stream obs_scene obs_audio obs_visible obs_studio obs_refresh", "",
    ];
    let group = pictogram_group(key);
    let rank = ORDER[group]
        .split_whitespace()
        .position(|candidate| candidate == id)
        .unwrap_or(999);
    format!("{rank:03}_{id}_{key}")
}

#[cfg(not(target_arch = "wasm32"))]
const PICTOGRAM_GROUP_KEYS: [&str; 22] = [
    "display_settings.pictogram_group_navigation",
    "display_settings.pictogram_group_editing",
    "display_settings.pictogram_group_files",
    "display_settings.pictogram_group_design",
    "display_settings.pictogram_group_development",
    "display_settings.pictogram_group_playback",
    "display_settings.pictogram_group_playback_files",
    "display_settings.pictogram_group_playback_art",
    "display_settings.pictogram_group_playback_code",
    "display_settings.pictogram_group_playback_arrows",
    "display_settings.pictogram_group_playback_navigation",
    "display_settings.pictogram_group_playback_editing",
    "display_settings.pictogram_group_playback_device",
    "display_settings.pictogram_group_playback_actions",
    "display_settings.pictogram_group_tracks",
    "display_settings.pictogram_group_timeline",
    "display_settings.pictogram_group_sound",
    "display_settings.pictogram_group_recording",
    "display_settings.pictogram_group_mouse",
    "display_settings.pictogram_group_system",
    "display_settings.pictogram_group_integrations",
    "display_settings.pictogram_group_other",
];

#[cfg(not(target_arch = "wasm32"))]
fn pictogram_group(key: &str) -> usize {
    // Keep the two native deletion keys together in Backspace's existing group.
    // The generic eraser/delete action keeps its own established placement.
    if matches!(key, "display_settings.pictogram_firmware_backspace" | "display_settings.pictogram_firmware_delete") {
        return 13;
    }
    if key.contains("pictogram_firmware_layer_") {
        return 0;
    }
    if key.contains("pictogram_integration_") {
        return 20;
    }
    if key.contains("mouse") || key.contains("wheel") || key.contains("pointer") {
        return 18;
    }
    if ["microphone", "mic_off", "audio_lines", "camera"]
        .iter()
        .any(|part| key.contains(part))
    {
        return 17;
    }
    if ["volume", "mute", "headphones"]
        .iter()
        .any(|part| key.contains(part))
    {
        return 16;
    }
    if ["step_back", "step_forward", "flag"]
        .iter()
        .any(|part| key.contains(part))
    {
        return 15;
    }
    if ["track", "skip_back", "skip_forward"]
        .iter()
        .any(|part| key.contains(part))
    {
        return 14;
    }
    // The old `key.contains("play")` matched `display_settings` for every
    // remaining icon. Split that bucket only; the groups above keep their
    // existing membership and relative order.
    let id = key.rsplit('.').next().unwrap_or(key);
    if id.ends_with("_play") || id.ends_with("_pause") || id.ends_with("_stop") {
        return 5;
    }
    if [
        "file", "folder", "save", "download", "printer", "export", "mail", "copy", "paste", "cut",
    ]
    .iter()
    .any(|part| id.contains(part))
    {
        return 6;
    }
    if [
        "paint",
        "pencil",
        "pen",
        "brush",
        "crop",
        "lasso",
        "pipette",
        "rectangle",
        "circle",
        "frame",
        "scaling",
        "rotate",
        "layers",
        "group",
        "image",
        "blend",
        "eye",
    ]
    .iter()
    .any(|part| id.contains(part))
    {
        return 7;
    }
    if [
        "terminal",
        "code",
        "bug",
        "git",
        "command",
        "component",
        "package",
        "blocks",
        "keyboard",
    ]
    .iter()
    .any(|part| id.contains(part))
    {
        return 8;
    }
    if id.contains("arrow") {
        return 9;
    }
    if [
        "search", "move", "home", "page", "history", "bookmark", "focus", "zoom", "view", "next",
        "previous", "end", "insert", "panel", "link",
    ]
    .iter()
    .any(|part| id.contains(part))
    {
        return 10;
    }
    if [
        "undo", "redo", "delete", "replace", "select", "text", "type", "scissors", "eraser",
    ]
    .iter()
    .any(|part| id.contains(part))
    {
        return 11;
    }
    if [
        "brightness",
        "computer",
        "calculator",
        "control_panel",
        "settings",
        "lock",
        "screenshot",
    ]
    .iter()
    .any(|part| id.contains(part))
    {
        return 12;
    }
    13
}

#[cfg(not(target_arch = "wasm32"))]
fn draw_grouped_catalog(
    ui: &mut egui::Ui,
    lang: crate::i18n::Language,
    dark: bool,
    scale: f32,
    search: &str,
    color: [u8; 3],
    background: [u8; 3],
    selected: Option<&[u8]>,
    columns: usize,
    id: &str,
    catalog: &[CatalogIcon],
) -> Option<(usize, String, Vec<u8>)> {
    let mut chosen = None;
    for (group, key) in PICTOGRAM_GROUP_KEYS.iter().enumerate() {
        let entries: Vec<_> = catalog
            .iter()
            .filter(|icon| {
                icon.group == group
                    && (search.is_empty() || icon.name.to_lowercase().contains(search))
            })
            .collect();
        if entries.is_empty() {
            continue;
        }
        ui.separator();
        ui.label(RichText::new(crate::i18n::tr_catalog(lang, key)).strong());
        egui::Grid::new((id, group))
            .num_columns(columns)
            .spacing(egui::vec2(7.0 * scale, 7.0 * scale))
            .show(ui, |ui| {
                for (position, icon) in entries.into_iter().enumerate() {
                    if draw_pictogram_library_tile(
                        ui,
                        dark,
                        scale,
                        &icon.bitmap,
                        color,
                        background,
                        &icon.name,
                        selected == Some(icon.bitmap.as_slice()),
                    ) {
                        chosen = Some((icon.index, icon.name.clone(), icon.bitmap.to_vec()));
                    }
                    if (position + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
    }
    chosen
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy)]
enum PictogramToolbarIcon {
    Choose,
    Pencil,
    Eraser,
    Clear,
    Delete,
    Save,
}

#[cfg(not(target_arch = "wasm32"))]
fn pictogram_toolbar_button(
    ui: &mut egui::Ui,
    dark: bool,
    scale: f32,
    icon: PictogramToolbarIcon,
    tooltip: &str,
    enabled: bool,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(34.0 * scale, 34.0 * scale),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let fill = if selected {
        app_accent().gamma_multiply(0.18)
    } else if enabled && response.hovered() {
        app_hover_fill(dark)
    } else {
        Color32::TRANSPARENT
    };
    let mut color = if selected {
        app_accent()
    } else {
        ui.visuals().text_color()
    };
    if !enabled {
        color = color.gamma_multiply(0.30);
    }
    ui.painter().rect(
        rect,
        7.0 * scale,
        fill,
        Stroke::new(
            1.0_f32,
            if selected {
                app_accent()
            } else {
                crate::ui_style::border_color(dark).gamma_multiply(if enabled { 1.0 } else { 0.45 })
            },
        ),
        egui::StrokeKind::Inside,
    );

    let center = rect.center();
    let p = |x: f32, y: f32| center + egui::vec2(x * scale, y * scale);
    let stroke = Stroke::new(1.7 * scale, color);
    match icon {
        PictogramToolbarIcon::Choose => {
            ui.painter().rect_stroke(
                egui::Rect::from_min_max(p(-7.0, -6.0), p(7.0, 6.0)),
                1.5 * scale,
                stroke,
                egui::StrokeKind::Inside,
            );
            ui.painter().circle_filled(p(3.3, -2.6), 1.5 * scale, color);
            ui.painter()
                .line_segment([p(-5.0, 3.5), p(-1.0, -0.5)], stroke);
            ui.painter()
                .line_segment([p(-1.0, -0.5), p(1.8, 2.3)], stroke);
            ui.painter()
                .line_segment([p(1.8, 2.3), p(4.2, 0.0)], stroke);
            ui.painter()
                .line_segment([p(4.2, 0.0), p(6.0, 2.0)], stroke);
        }
        PictogramToolbarIcon::Pencil => {
            ui.painter()
                .line_segment([p(-5.5, 5.5), p(5.0, -5.0)], stroke);
            ui.painter()
                .line_segment([p(-6.5, 6.5), p(-2.8, 5.5)], stroke);
            ui.painter()
                .line_segment([p(3.0, -7.0), p(7.0, -3.0)], stroke);
        }
        PictogramToolbarIcon::Eraser => {
            paint_closed_shape(
                ui,
                &[p(-7.0, 2.0), p(1.5, -6.5), p(7.0, -1.0), p(-1.5, 7.0)],
                color,
            );
            ui.painter()
                .line_segment([p(-4.0, -1.0), p(2.0, 5.0)], stroke);
        }
        PictogramToolbarIcon::Clear => {
            ui.painter().circle_stroke(center, 6.2 * scale, stroke);
            ui.painter()
                .line_segment([p(2.8, -5.5), p(7.0, -5.5)], stroke);
            ui.painter()
                .line_segment([p(7.0, -5.5), p(7.0, -1.3)], stroke);
        }
        PictogramToolbarIcon::Delete => {
            ui.painter().rect_stroke(
                egui::Rect::from_min_max(p(-4.8, -3.3), p(4.8, 6.5)),
                1.2 * scale,
                stroke,
                egui::StrokeKind::Inside,
            );
            ui.painter()
                .line_segment([p(-6.5, -5.2), p(6.5, -5.2)], stroke);
            ui.painter()
                .line_segment([p(-2.5, -7.0), p(2.5, -7.0)], stroke);
            ui.painter()
                .line_segment([p(-1.8, -1.0), p(-1.8, 4.3)], stroke);
            ui.painter()
                .line_segment([p(1.8, -1.0), p(1.8, 4.3)], stroke);
        }
        PictogramToolbarIcon::Save => {
            ui.painter().rect_stroke(
                egui::Rect::from_min_max(p(-6.5, -6.5), p(6.5, 6.5)),
                1.2 * scale,
                stroke,
                egui::StrokeKind::Inside,
            );
            ui.painter().rect_stroke(
                egui::Rect::from_min_max(p(-3.6, -6.5), p(3.5, -1.8)),
                0.5,
                stroke,
                egui::StrokeKind::Inside,
            );
            ui.painter().rect_stroke(
                egui::Rect::from_min_max(p(-3.8, 1.2), p(3.8, 6.5)),
                0.8,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
    }
    response.on_hover_text(tooltip)
}

#[cfg(not(target_arch = "wasm32"))]
fn draw_no_pictogram_tile(
    ui: &mut egui::Ui,
    dark: bool,
    scale: f32,
    tooltip: &str,
    selected: bool,
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(48.0 * scale, 48.0 * scale), Sense::click());
    let border = if selected {
        Color32::from_rgb(84, 189, 191)
    } else {
        crate::ui_style::border_color(dark)
    };
    ui.painter().rect(
        rect,
        7.0 * scale,
        if response.hovered() {
            app_hover_fill(dark)
        } else {
            app_surface_fill(dark)
        },
        Stroke::new(if selected { 2.0_f32 } else { 1.0_f32 }, border),
        egui::StrokeKind::Inside,
    );
    let icon_rect = rect.shrink(13.0 * scale);
    let color = app_muted_text(dark);
    ui.painter().circle_stroke(
        icon_rect.center(),
        icon_rect.width() * 0.42,
        Stroke::new(1.8 * scale, color),
    );
    ui.painter().line_segment(
        [icon_rect.left_top(), icon_rect.right_bottom()],
        Stroke::new(1.8 * scale, color),
    );
    response.on_hover_text(tooltip).clicked()
}

#[cfg(not(target_arch = "wasm32"))]
fn draw_assigned_pictogram_preview(
    ui: &mut egui::Ui,
    dark: bool,
    scale: f32,
    bitmap: Option<&[u8]>,
    color: [u8; 3],
    tooltip: &str,
) {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(34.0 * scale, 34.0 * scale), Sense::hover());
    ui.painter().rect(
        rect,
        7.0 * scale,
        app_surface_fill(dark),
        Stroke::new(1.0_f32, crate::ui_style::border_color(dark)),
        egui::StrokeKind::Inside,
    );
    if let Some(bitmap) = bitmap {
        let bitmap_rect = egui::Rect::from_center_size(
            rect.center(),
            egui::vec2(
                PICTOGRAM_WIDTH as f32 * scale,
                PICTOGRAM_HEIGHT as f32 * scale,
            ),
        );
        paint_monochrome_pictogram(
            ui,
            bitmap_rect,
            bitmap,
            visible_pictogram_color(dark, color),
            Color32::TRANSPARENT,
        );
    } else {
        let icon_rect = rect.shrink(10.0 * scale);
        ui.painter().line_segment(
            [icon_rect.left_top(), icon_rect.right_bottom()],
            Stroke::new(1.5 * scale, app_muted_text(dark)),
        );
    }
    response.on_hover_text(tooltip);
}

fn display_settings_row(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    label: &str,
    enabled: bool,
    control_width: f32,
    control: impl FnOnce(&mut egui::Ui),
) {
    let mut tooltip = None;
    for lang in [
        crate::i18n::Language::Russian,
        crate::i18n::Language::English,
    ] {
        if label == crate::i18n::tr_catalog(lang, "display_settings.accent_color") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_accent_color",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.background_color") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_background_color",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.brightness") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_brightness",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.button_style") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_button_style",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.layer_language_element") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_layer_language_element",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.clock_element") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_clock_element",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.date_element") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_date_element",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.song_element") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_song_element",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.standby_text_color") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_standby_text_color",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.standby_brightness") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_standby_brightness",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.clock_background_asset") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_clock_background_asset",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.clock_background_dim") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_clock_background_dim",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.clock_background_color") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_clock_background_color",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.clock_font") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_clock_font",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.clock_size") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_clock_size",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.clock_colon_blink") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_clock_colon_blink",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.clock_delay") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_clock_delay",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.date_delay") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_date_delay",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.date_alignment") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_date_alignment",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.date_format") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_date_format",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.display_timeout") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_display_timeout",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.choose_action_type") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_pictogram_action_type",
            ));
        }
        if [
            "display_settings.choose_macro",
            "display_settings.choose_tap_dance",
        ]
        .iter()
        .any(|key| label == crate::i18n::tr_catalog(lang, key))
        {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_pictogram_slot",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.choose_pictogram") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_pictogram_choice",
            ));
        }
        if label == crate::i18n::tr_catalog(lang, "display_settings.pictogram_name") {
            tooltip = Some(crate::i18n::tr_catalog(
                lang,
                "display_settings.tooltip_pictogram_name",
            ));
        }
    }
    crate::ui_style::settings_list_row_with_tooltip(
        ui,
        width,
        height,
        label,
        enabled,
        tooltip,
        control_width,
        control,
    );
}

fn display_settings_row_count(settings: &DisplaySettingsState, mode: u8) -> usize {
    if mode == 3 {
        return 7;
    }
    if mode == 0 {
        return 2;
    }
    if mode == 1 {
        return 1
            + usize::from(settings.background_color_supported)
            + usize::from(settings.brightness_supported)
            + usize::from(settings.button_style_supported);
    }
    if !settings.clock_settings_supported {
        return 0;
    }
    (if settings.clock_overlay_controls_supported {
        2
    } else {
        1 + usize::from(settings.clock_info_color_supported)
    }) + if settings.clock_background_asset_supported {
        2
    } else {
        1
    } + usize::from(settings.clock_background_dim_supported)
        + 4
        + 2 * usize::from(settings.standby_controls_supported)
        + usize::from(settings.clock_colon_blink_supported)
        + if settings.date_supported { 3 } else { 0 }
}

fn display_settings_panel_rects(
    body: egui::Rect,
    settings_width: f32,
) -> (egui::Rect, egui::Rect, bool) {
    let gap = 18.0;
    let side_by_side = body.width() >= 780.0_f32.max(settings_width + gap + 220.0);
    if side_by_side {
        let preview_width = (body.width() - settings_width - gap).clamp(220.0, 292.0);
        let group_width = preview_width + gap + settings_width;
        let group_left = body.center().x - group_width / 2.0;
        return (
            egui::Rect::from_min_size(
                egui::pos2(group_left, body.top()),
                egui::vec2(preview_width, body.height()),
            ),
            egui::Rect::from_min_size(
                egui::pos2(group_left + preview_width + gap, body.top()),
                egui::vec2(settings_width, body.height()),
            ),
            true,
        );
    }

    let preview_height = (body.height() * 0.46).clamp(210.0, 330.0);
    (
        egui::Rect::from_min_size(body.min, egui::vec2(body.width(), preview_height)),
        egui::Rect::from_min_max(
            egui::pos2(body.left(), body.top() + preview_height + 12.0),
            body.max,
        ),
        false,
    )
}

pub(super) fn paint_closed_shape(ui: &egui::Ui, points: &[egui::Pos2], color: Color32) {
    let stroke = Stroke::new(1.35_f32, color);
    for index in 0..points.len() {
        ui.painter()
            .line_segment([points[index], points[(index + 1) % points.len()]], stroke);
    }
}

pub(super) fn paint_trapezoid_icon(
    ui: &egui::Ui,
    rect: egui::Rect,
    narrow_top: bool,
    color: Color32,
) {
    let inset = (rect.width() * 0.2).min(3.0);
    paint_closed_shape(
        ui,
        &[
            egui::pos2(
                if narrow_top {
                    rect.left() + inset
                } else {
                    rect.left()
                },
                rect.top(),
            ),
            egui::pos2(
                if narrow_top {
                    rect.right() - inset
                } else {
                    rect.right()
                },
                rect.top(),
            ),
            egui::pos2(
                if narrow_top {
                    rect.right()
                } else {
                    rect.right() - inset
                },
                rect.bottom(),
            ),
            egui::pos2(
                if narrow_top {
                    rect.left()
                } else {
                    rect.left() + inset
                },
                rect.bottom(),
            ),
        ],
        color,
    );
}

pub(super) fn paint_wave_icon(
    ui: &egui::Ui,
    rect: egui::Rect,
    vertical: bool,
    horizontal: bool,
    phase: bool,
    color: Color32,
) {
    let offsets = if phase {
        [2.0, 1.0, 0.0, 1.0, 2.0, 1.0, 0.0, 1.0, 2.0]
    } else {
        [0.0, 1.0, 2.0, 1.0, 0.0, 1.0, 2.0, 1.0, 0.0]
    };
    let mut points = Vec::with_capacity(32);
    for sample in 0..=8 {
        let t = sample as f32 / 8.0;
        let mut x = egui::lerp(rect.left()..=rect.right(), t);
        if vertical && sample == 0 {
            x += offsets[0];
        }
        if vertical && sample == 8 {
            x -= offsets[8];
        }
        points.push(egui::pos2(
            x,
            rect.top() + if horizontal { offsets[sample] } else { 0.0 },
        ));
    }
    for sample in 1..=8 {
        let t = sample as f32 / 8.0;
        let mut y = egui::lerp(rect.top()..=rect.bottom(), t);
        if horizontal && sample == 8 {
            y -= offsets[8];
        }
        points.push(egui::pos2(
            rect.right() - if vertical { offsets[sample] } else { 0.0 },
            y,
        ));
    }
    for sample in (0..=7).rev() {
        let t = sample as f32 / 8.0;
        let mut x = egui::lerp(rect.left()..=rect.right(), t);
        if vertical && sample == 0 {
            x += offsets[0];
        }
        points.push(egui::pos2(
            x,
            rect.bottom() - if horizontal { offsets[sample] } else { 0.0 },
        ));
    }
    for sample in (1..=7).rev() {
        let t = sample as f32 / 8.0;
        points.push(egui::pos2(
            rect.left() + if vertical { offsets[sample] } else { 0.0 },
            egui::lerp(rect.top()..=rect.bottom(), t),
        ));
    }
    paint_closed_shape(ui, &points, color);
}

pub(super) fn paint_semicircle_icon(
    ui: &egui::Ui,
    rect: egui::Rect,
    points_up: bool,
    color: Color32,
) {
    let stroke = Stroke::new(1.35_f32, color);
    let radius = (rect.width() / 2.0).min(rect.height()) - stroke.width / 2.0;
    let baseline = if points_up {
        rect.bottom() - (rect.height() - radius) / 2.0
    } else {
        rect.top() + (rect.height() - radius) / 2.0
    };
    let center = egui::pos2(rect.center().x, baseline);
    let clip = if points_up {
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), baseline))
    } else {
        egui::Rect::from_min_max(egui::pos2(rect.left(), baseline), rect.max)
    };
    ui.painter()
        .with_clip_rect(clip)
        .circle_stroke(center, radius, stroke);
    ui.painter().line_segment(
        [
            egui::pos2(center.x - radius, baseline),
            egui::pos2(center.x + radius, baseline),
        ],
        stroke,
    );
}

pub(super) fn paint_oval_side_icon(
    ui: &egui::Ui,
    rect: egui::Rect,
    oval_top: bool,
    color: Color32,
) {
    let stroke = Stroke::new(1.35_f32, color);
    let cap_height = (rect.height() * 0.35).min(rect.width() / 2.0);
    let baseline = if oval_top {
        rect.top() + cap_height
    } else {
        rect.bottom() - cap_height
    };
    let cap_clip = if oval_top {
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), baseline))
    } else {
        egui::Rect::from_min_max(egui::pos2(rect.left(), baseline), rect.max)
    };
    ui.painter()
        .with_clip_rect(cap_clip)
        .add(egui::Shape::ellipse_stroke(
            egui::pos2(rect.center().x, baseline),
            Vec2::new(rect.width() / 2.0 - stroke.width / 2.0, cap_height),
            stroke,
        ));

    let body_clip = if oval_top {
        egui::Rect::from_min_max(egui::pos2(rect.left(), baseline), rect.max)
    } else {
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), baseline))
    };
    ui.painter().with_clip_rect(body_clip).rect_stroke(
        rect,
        (rect.height() * 0.18).min(3.0),
        stroke,
        egui::StrokeKind::Inside,
    );
}

fn paint_direction_pattern_icon(
    ui: &egui::Ui,
    rect: egui::Rect,
    variant: u8,
    color: Color32,
    paint: fn(&egui::Ui, egui::Rect, bool, Color32),
) {
    fn fitted_key_rect(rect: egui::Rect) -> egui::Rect {
        const KEY_ASPECT: f32 = 76.0 / 44.0;
        let size = if rect.width() / rect.height() > KEY_ASPECT {
            Vec2::new(rect.height() * KEY_ASPECT, rect.height())
        } else {
            Vec2::new(rect.width(), rect.width() / KEY_ASPECT)
        };
        egui::Rect::from_center_size(rect.center(), size)
    }

    let gap = 2.0;
    match variant {
        0 | 1 => {
            let height = (rect.height() - gap) / 2.0;
            let top = egui::Rect::from_min_size(rect.min, Vec2::new(rect.width(), height));
            let bottom = egui::Rect::from_min_size(
                egui::pos2(rect.left(), top.bottom() + gap),
                Vec2::new(rect.width(), height),
            );
            paint(ui, fitted_key_rect(top), variant == 0, color);
            paint(ui, fitted_key_rect(bottom), variant == 1, color);
        }
        2 | 3 => {
            let width = (rect.width() - gap) / 2.0;
            let left = egui::Rect::from_min_size(rect.min, Vec2::new(width, rect.height()));
            let right = egui::Rect::from_min_size(
                egui::pos2(left.right() + gap, rect.top()),
                Vec2::new(width, rect.height()),
            );
            paint(ui, fitted_key_rect(left), variant == 2, color);
            paint(ui, fitted_key_rect(right), variant == 3, color);
        }
        4 | 5 => paint(ui, fitted_key_rect(rect), variant == 4, color),
        _ => {}
    }
}

fn paint_display_button_style_icon(ui: &egui::Ui, style: u8, rect: egui::Rect, color: Color32) {
    let stroke = Stroke::new(1.35_f32, color);
    let wide =
        egui::Rect::from_center_size(rect.center(), Vec2::new(rect.width(), rect.height() - 2.0));
    match style {
        0 => {
            ui.painter()
                .rect_stroke(wide, 3.0, stroke, egui::StrokeKind::Inside);
        }
        1 => {
            ui.painter()
                .circle_stroke(rect.center(), (rect.height() - 2.0) / 2.0, stroke);
        }
        2 => {
            ui.painter()
                .rect_stroke(wide, wide.height() / 2.0, stroke, egui::StrokeKind::Inside);
        }
        3 => {
            paint_closed_shape(
                ui,
                &[
                    egui::pos2(rect.center().x, wide.top()),
                    egui::pos2(wide.right(), rect.center().y),
                    egui::pos2(rect.center().x, wide.bottom()),
                    egui::pos2(wide.left(), rect.center().y),
                ],
                color,
            );
        }
        4 => {
            let cut = 4.0;
            paint_closed_shape(
                ui,
                &[
                    egui::pos2(wide.left() + cut, wide.top()),
                    egui::pos2(wide.right() - cut, wide.top()),
                    egui::pos2(wide.right(), wide.top() + cut),
                    egui::pos2(wide.right(), wide.bottom() - cut),
                    egui::pos2(wide.right() - cut, wide.bottom()),
                    egui::pos2(wide.left() + cut, wide.bottom()),
                    egui::pos2(wide.left(), wide.bottom() - cut),
                    egui::pos2(wide.left(), wide.top() + cut),
                ],
                color,
            );
        }
        5 => {
            let inset = 5.0;
            paint_closed_shape(
                ui,
                &[
                    egui::pos2(wide.left() + inset, wide.top()),
                    egui::pos2(wide.right() - inset, wide.top()),
                    egui::pos2(wide.right(), rect.center().y),
                    egui::pos2(wide.right() - inset, wide.bottom()),
                    egui::pos2(wide.left() + inset, wide.bottom()),
                    egui::pos2(wide.left(), rect.center().y),
                ],
                color,
            );
        }
        6 => {
            ui.painter()
                .rect_stroke(wide, 6.0, stroke, egui::StrokeKind::Inside);
        }
        7 => {
            let gap = 2.0;
            let half_height = (wide.height() - gap) / 2.0;
            let top = egui::Rect::from_min_size(wide.min, Vec2::new(wide.width(), half_height));
            let bottom = egui::Rect::from_min_size(
                egui::pos2(wide.left(), top.bottom() + gap),
                Vec2::new(wide.width(), half_height),
            );
            paint_trapezoid_icon(ui, top, true, color);
            paint_trapezoid_icon(ui, bottom, false, color);
        }
        8 => {
            let gap = 2.0;
            let half_height = (wide.height() - gap) / 2.0;
            let top = egui::Rect::from_min_size(wide.min, Vec2::new(wide.width(), half_height));
            let bottom = egui::Rect::from_min_size(
                egui::pos2(wide.left(), top.bottom() + gap),
                Vec2::new(wide.width(), half_height),
            );
            paint_trapezoid_icon(ui, top, false, color);
            paint_trapezoid_icon(ui, bottom, true, color);
        }
        9 => paint_trapezoid_icon(ui, wide, true, color),
        10 => paint_trapezoid_icon(ui, wide, false, color),
        11 | 12 | 17 | 18 => {
            let gap = 2.0;
            let half_width = (wide.width() - gap) / 2.0;
            let left = egui::Rect::from_min_size(wide.min, Vec2::new(half_width, wide.height()));
            let right = egui::Rect::from_min_size(
                egui::pos2(left.right() + gap, wide.top()),
                Vec2::new(half_width, wide.height()),
            );
            let all_sides = matches!(style, 17 | 18);
            paint_wave_icon(ui, left, true, all_sides, false, color);
            paint_wave_icon(ui, right, true, all_sides, matches!(style, 12 | 18), color);
        }
        13 | 14 => {
            let gap = 2.0;
            let half_width = (wide.width() - gap) / 2.0;
            let left = egui::Rect::from_min_size(wide.min, Vec2::new(half_width, wide.height()));
            let right = egui::Rect::from_min_size(
                egui::pos2(left.right() + gap, wide.top()),
                Vec2::new(half_width, wide.height()),
            );
            paint_trapezoid_icon(ui, left, style == 13, color);
            paint_trapezoid_icon(ui, right, style == 14, color);
        }
        15 | 16 => {
            let gap = 2.0;
            let half_height = (wide.height() - gap) / 2.0;
            let top = egui::Rect::from_min_size(wide.min, Vec2::new(wide.width(), half_height));
            let bottom = egui::Rect::from_min_size(
                egui::pos2(wide.left(), top.bottom() + gap),
                Vec2::new(wide.width(), half_height),
            );
            paint_wave_icon(ui, top, false, true, false, color);
            paint_wave_icon(ui, bottom, false, true, style == 16, color);
        }
        19..=24 => paint_direction_pattern_icon(ui, wide, style - 19, color, paint_semicircle_icon),
        25..=30 => paint_direction_pattern_icon(ui, wide, style - 25, color, paint_oval_side_icon),
        31 | 32 => {
            let cut = 4.0;
            let points = if style == 31 {
                vec![
                    egui::pos2(wide.left() + cut, wide.top()),
                    egui::pos2(wide.right(), wide.top()),
                    egui::pos2(wide.right(), wide.bottom() - cut),
                    egui::pos2(wide.right() - cut, wide.bottom()),
                    egui::pos2(wide.left(), wide.bottom()),
                    egui::pos2(wide.left(), wide.top() + cut),
                ]
            } else {
                vec![
                    egui::pos2(wide.left(), wide.top()),
                    egui::pos2(wide.right() - cut, wide.top()),
                    egui::pos2(wide.right(), wide.top() + cut),
                    egui::pos2(wide.right(), wide.bottom()),
                    egui::pos2(wide.left() + cut, wide.bottom()),
                    egui::pos2(wide.left(), wide.bottom() - cut),
                ]
            };
            paint_closed_shape(ui, &points, color);
        }
        _ => {}
    }
}

impl EntropyApp {
    pub(super) fn draw_display_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let selected_sm62 = self
            .selected_device
            .and_then(|index| self.device_manager.devices().get(index))
            .is_some_and(crate::device::Device::is_ergohaven_sm62);
        if self.display_settings.sm62_supported || selected_sm62 {
            self.draw_sm62_display_settings_page(ui, content_rect);
            return;
        }
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let transport_ready = self.qmk_setting_transport_available();

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new(crate::i18n::tr_catalog(lang, "display_settings.title"))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr_catalog(
                        lang,
                        "display_settings.description",
                    ))
                    .size(13.0)
                    .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if !self.display_settings.supported {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr_catalog(lang, "display_settings.unavailable"),
                        None,
                    );
                    return;
                }
                if !transport_ready {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr_catalog(lang, "display_settings.connect"),
                        None,
                    );
                    return;
                }

                let mode_id = ui.make_persistent_id("display_settings_mode");
                let mut mode = ui
                    .ctx()
                    .data(|data| data.get_temp::<u8>(mode_id))
                    .unwrap_or(1);
                if mode == 0 || (!self.display_settings.clock_settings_supported && mode > 1) {
                    mode = 1;
                }
                let tab_modes: Vec<u8> = if self.display_settings.clock_settings_supported {
                    vec![1, 2, 3]
                } else {
                    vec![1]
                };
                let tab_labels: Vec<String> = tab_modes
                    .iter()
                    .map(|mode| {
                        crate::i18n::tr_catalog(
                            lang,
                            match mode {
                                1 => "display_settings.main_section",
                                2 => "display_settings.standby_section",
                                0 => "display_settings.startup_section",
                                _ => "display_settings.pictograms_section",
                            },
                        )
                        .to_owned()
                    })
                    .collect();
                let tab_width = (ui.available_width() - 40.0).clamp(220.0, 420.0);
                if let Some(picked) = crate::ui_style::settings_segmented_control(
                    ui,
                    "display_settings_tabs",
                    &tab_labels,
                    tab_modes
                        .iter()
                        .position(|value| *value == mode)
                        .unwrap_or(0),
                    egui::vec2(tab_width, 38.0),
                ) {
                    mode = tab_modes[picked];
                }
                ui.ctx().data_mut(|data| data.insert_temp(mode_id, mode));
                #[cfg(not(target_arch = "wasm32"))]
                if mode == 3
                    && self
                        .display_settings
                        .pictograms
                        .needs_automatic_load(self.connection_generation)
                {
                    self.start_pictogram_load(ui.ctx());
                }
                ui.add_space(18.0);

                let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                let row_count = display_settings_row_count(&self.display_settings, mode);
                let body = ui.available_rect_before_wrap();
                let (preview_rect, settings_rect, _) =
                    display_settings_panel_rects(body, metrics.settings_content_width());
                crate::ui_style::allocate_ui_at_rect(ui, preview_rect, |ui| {
                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        if mode == 3 {
                            #[cfg(not(target_arch = "wasm32"))]
                            self.draw_pictogram_editor_panel(ui, preview_rect.height());
                            #[cfg(target_arch = "wasm32")]
                            self.draw_display_preview_panel(ui, preview_rect.height(), mode);
                        } else {
                            self.draw_display_preview_panel(ui, preview_rect.height(), mode);
                        }
                    });
                });
                crate::ui_style::allocate_ui_at_rect(ui, settings_rect, |ui| {
                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        let scroll_id = match mode {
                            0 => "display_settings_startup",
                            1 => "display_settings_main",
                            2 => "display_settings_standby",
                            _ => "display_settings_pictograms",
                        };
                        let list = allocate_adaptive_settings_list_viewport(
                            ui,
                            scroll_id,
                            metrics,
                            row_count,
                            if mode == 3 { metrics.value(60.0) } else { 0.0 },
                        );
                        // This page renders its rows conditionally rather than from a slice.
                        // Move the complete list by the real scroll offset; using the helper's
                        // virtualized content rectangle would restart drawing at row zero and
                        // make the last controls unreachable.
                        let full_content_rect = egui::Rect::from_min_size(
                            egui::pos2(
                                list.viewport.left(),
                                list.viewport.top() - list.scroll_offset,
                            ),
                            egui::vec2(list.row_content_width, list.content_height),
                        );
                        crate::ui_style::allocate_ui_at_rect(ui, full_content_rect, |ui| {
                            ui.set_clip_rect(list.viewport);
                            ui.set_min_size(full_content_rect.size());
                            ui.spacing_mut().item_spacing.y = 0.0;

                            let scale = (list.row_height / 54.0).clamp(1.0, 1.12);
                            if mode == 0 {
                                for reset in [false, true] {
                                    display_settings_row(
                                        ui,
                                        list.row_content_width,
                                        list.row_height,
                                        crate::i18n::tr_catalog(
                                            lang,
                                            if reset {
                                                "display_settings.restore_default_splash"
                                            } else {
                                                "display_settings.upload_splash"
                                            },
                                        ),
                                        true,
                                        120.0 * scale,
                                        |ui| {
                                            self.draw_startup_image_control(ui, dark, scale, reset)
                                        },
                                    );
                                }
                            }

                            if mode == 1 {
                                display_settings_row(
                                    ui,
                                    list.row_content_width,
                                    list.row_height,
                                    crate::i18n::tr_catalog(lang, "display_settings.accent_color"),
                                    row_count > 1,
                                    64.0 * scale,
                                    |ui| {
                                        self.draw_display_color_swatch(ui, dark, scale, 0);
                                    },
                                );

                                if self.display_settings.background_color_supported {
                                    let has_rows_after = self.display_settings.brightness_supported
                                        || self.display_settings.button_style_supported;
                                    display_settings_row(
                                        ui,
                                        list.row_content_width,
                                        list.row_height,
                                        crate::i18n::tr_catalog(
                                            lang,
                                            "display_settings.background_color",
                                        ),
                                        has_rows_after,
                                        64.0 * scale,
                                        |ui| {
                                            self.draw_display_color_swatch(ui, dark, scale, 1);
                                        },
                                    );
                                }

                                if self.display_settings.brightness_supported {
                                    display_settings_row(
                                        ui,
                                        list.row_content_width,
                                        list.row_height,
                                        crate::i18n::tr_catalog(
                                            lang,
                                            "display_settings.brightness",
                                        ),
                                        self.display_settings.button_style_supported,
                                        196.0 * scale,
                                        |ui| {
                                            self.draw_display_brightness_slider(ui, dark, scale);
                                        },
                                    );
                                }

                                if self.display_settings.button_style_supported {
                                    display_settings_row(
                                        ui,
                                        list.row_content_width,
                                        list.row_height,
                                        crate::i18n::tr_catalog(
                                            lang,
                                            "display_settings.button_style",
                                        ),
                                        false,
                                        196.0 * scale,
                                        |ui| {
                                            self.draw_display_button_style_selector(
                                                ui, dark, scale,
                                            );
                                        },
                                    );
                                }
                            }

                            #[cfg(not(target_arch = "wasm32"))]
                            if mode == 3 {
                                self.draw_pictogram_library_panel(ui, &list, dark, scale);
                            }

                            if mode == 2 && self.display_settings.clock_settings_supported {
                                if self.display_settings.clock_overlay_controls_supported {
                                    self.draw_standby_overlay_row(ui, &list, dark, scale, 1);
                                    self.draw_standby_overlay_row(ui, &list, dark, scale, 0);
                                }
                                if self.display_settings.date_supported {
                                    self.draw_date_overlay_row(ui, &list, dark, scale);
                                }
                                if self.display_settings.standby_controls_supported {
                                    self.draw_standby_overlay_row(ui, &list, dark, scale, 2);
                                }
                                display_settings_row(
                                    ui,
                                    list.row_content_width,
                                    list.row_height,
                                    crate::i18n::tr_catalog(
                                        lang,
                                        "display_settings.standby_text_color",
                                    ),
                                    true,
                                    64.0 * scale,
                                    |ui| self.draw_display_color_swatch(ui, dark, scale, 2),
                                );
                                if self.display_settings.standby_controls_supported {
                                    self.draw_standby_brightness_row(ui, &list, scale);
                                }
                                self.draw_clock_setting_row(
                                    ui,
                                    &list,
                                    scale,
                                    "display_settings.clock_font",
                                    "clock_style",
                                    CLOCK_STYLE_QSID,
                                    self.display_settings.clock_style,
                                    &[
                                        "display_settings.clock_font_montserrat",
                                        "display_settings.clock_font_ubuntu_sans",
                                        "display_settings.clock_font_ubuntu_mono",
                                        "display_settings.clock_font_liberation_mono",
                                        "display_settings.clock_font_dejavu_sans",
                                        "display_settings.clock_font_dejavu_serif",
                                        "display_settings.clock_font_dejavu_mono",
                                        "display_settings.clock_font_liberation_sans",
                                        "display_settings.clock_font_liberation_serif",
                                        "display_settings.clock_font_liberation_narrow",
                                    ],
                                    true,
                                );
                                if self.display_settings.clock_background_asset_supported {
                                    display_settings_row(
                                        ui,
                                        list.row_content_width,
                                        list.row_height,
                                        crate::i18n::tr_catalog(
                                            lang,
                                            "display_settings.clock_background_asset",
                                        ),
                                        true,
                                        230.0 * scale,
                                        |ui| self.draw_standby_background_control(ui, dark, scale),
                                    );
                                }
                                if self.display_settings.clock_background_dim_supported {
                                    display_settings_row(
                                        ui,
                                        list.row_content_width,
                                        list.row_height,
                                        crate::i18n::tr_catalog(
                                            lang,
                                            "display_settings.clock_background_dim",
                                        ),
                                        true,
                                        196.0 * scale,
                                        |ui| self.draw_clock_background_dim_slider(ui, dark, scale),
                                    );
                                }
                                display_settings_row(
                                    ui,
                                    list.row_content_width,
                                    list.row_height,
                                    crate::i18n::tr_catalog(
                                        lang,
                                        "display_settings.clock_background_color",
                                    ),
                                    true,
                                    64.0 * scale,
                                    |ui| self.draw_display_color_swatch(ui, dark, scale, 3),
                                );
                                self.draw_clock_setting_row(
                                    ui,
                                    &list,
                                    scale,
                                    "display_settings.clock_size",
                                    "clock_size",
                                    CLOCK_SIZE_QSID,
                                    self.display_settings.clock_size,
                                    &[
                                        "display_settings.clock_size_compact",
                                        "display_settings.clock_size_medium",
                                        "display_settings.clock_size_large",
                                        "display_settings.clock_size_extra",
                                    ],
                                    true,
                                );
                                if self.display_settings.clock_colon_blink_supported {
                                    self.draw_clock_colon_blink_row(ui, &list, scale);
                                }
                                if self.display_settings.date_supported {
                                    self.draw_date_settings(ui, &list, dark, scale);
                                }
                                self.draw_clock_setting_row(
                                    ui,
                                    &list,
                                    scale,
                                    "display_settings.display_timeout",
                                    "display_timeout",
                                    DISPLAY_TIMEOUT_QSID,
                                    self.display_settings.display_timeout,
                                    &[
                                        "display_settings.never",
                                        "display_settings.minute_1",
                                        "display_settings.minutes_2",
                                        "display_settings.minutes_5",
                                        "display_settings.minutes_10",
                                        "display_settings.minutes_15",
                                        "display_settings.minutes_30",
                                        "display_settings.minutes_60",
                                    ],
                                    false,
                                );
                            }
                        });
                        if list.has_scrollbar {
                            crate::ui_style::paint_floating_scrollbar_handle(
                                ui,
                                list.track_rect,
                                list.handle_height,
                                list.scroll_ratio,
                                list.track_hovered,
                            );
                        }
                    });
                });
                #[cfg(not(target_arch = "wasm32"))]
                if mode == 3 {
                    let scale = (metrics.settings_row_height() / 54.0).clamp(1.0, 1.12);
                    let footer_viewport = egui::Rect::from_min_max(
                        body.min,
                        egui::pos2(body.right(), body.bottom() - metrics.value(60.0)),
                    );
                    let rect = crate::app::fixed_settings_action_bar_rect(
                        footer_viewport,
                        metrics,
                        crate::ui_style::modal_action_button_size() * scale,
                        4,
                        8.0 * scale,
                    );
                    crate::ui_style::allocate_ui_at_rect(ui, rect, |ui| {
                        self.draw_pictogram_footer(ui, scale)
                    });
                }
            });
        });
    }

    fn apply_date_setting(&mut self, index: usize, value: u8) {
        if self.display_settings.date[index] == value {
            return;
        }
        self.display_settings.date[index] = value;
        self.queue_display_setting_write(
            crate::i18n::tr_catalog(self.app_settings.language, "display_settings.date_element")
                .to_owned(),
            DATE_QSIDS[index],
            self.display_settings.confirmed_date[index] as u16,
            value as u16,
        );
    }

    fn draw_date_overlay_row(
        &mut self,
        ui: &mut egui::Ui,
        list: &AdaptiveSettingsListViewport,
        _dark: bool,
        scale: f32,
    ) {
        let mut visible = self.display_settings.date[0] != 0;
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(self.app_settings.language, "display_settings.date_element"),
            true,
            46.0 * scale,
            |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    crate::ui_style::settings_switch_sized_stable(
                        ui,
                        "date_visible",
                        &mut visible,
                        Vec2::new(46.0 * scale, 24.0 * scale),
                    );
                });
            },
        );
        self.apply_date_setting(0, u8::from(visible));
    }

    fn draw_standby_brightness_row(
        &mut self,
        ui: &mut egui::Ui,
        list: &AdaptiveSettingsListViewport,
        scale: f32,
    ) {
        let mut value = self.display_settings.date[10] as f32;
        let mut commit = false;
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(
                self.app_settings.language,
                "display_settings.standby_brightness",
            ),
            true,
            196.0 * scale,
            |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_sized(
                        [40.0 * scale, 34.0 * scale],
                        egui::Label::new(format!("{}%", value.round() as u8))
                            .halign(egui::Align::RIGHT),
                    );
                    ui.spacing_mut().slider_width = 140.0 * scale;
                    let r = ui.add_sized(
                        [140.0 * scale, 34.0 * scale],
                        egui::Slider::new(&mut value, 0.0..=100.0)
                            .step_by(1.0)
                            .show_value(false),
                    );
                    if r.changed() {
                        self.display_settings.date[10] = value.round() as u8;
                    }
                    commit = r.changed();
                });
            },
        );
        if commit {
            self.queue_display_setting_write(
                "Brightness".into(),
                DATE_QSIDS[10],
                self.display_settings.confirmed_date[10] as u16,
                value.round() as u16,
            );
        }
    }

    fn draw_date_settings(
        &mut self,
        ui: &mut egui::Ui,
        list: &AdaptiveSettingsListViewport,
        _dark: bool,
        scale: f32,
    ) {
        let lang = self.app_settings.language;
        let translated = |keys: &[&'static str]| {
            keys.iter()
                .map(|k| crate::i18n::tr_catalog(lang, k).to_owned())
                .collect::<Vec<_>>()
        };
        let delay = translated(&STANDBY_DELAY_KEYS);
        for (index, key, labels) in [
            (
                8,
                "display_settings.date_format",
                vec![
                    "DD.MM.YYYY".into(),
                    "MM.DD.YYYY".into(),
                    "YYYY.MM.DD".into(),
                ],
            ),
            (9, "display_settings.date_delay", delay),
        ] {
            let current = self.display_settings.date[index];
            let selected = if index == 9 {
                [0, 1, 2, 3, 7, 4, 5, 6]
                    .iter()
                    .position(|v| *v == current)
                    .unwrap_or(4)
            } else {
                current as usize
            };
            display_settings_row(
                ui,
                list.row_content_width,
                list.row_height,
                crate::i18n::tr_catalog(lang, key),
                true,
                196.0 * scale,
                |ui| {
                    let (_, choice) = crate::ui_style::modern_dropdown_select_sized(
                        ui,
                        ui.make_persistent_id(("date_option", index)),
                        &labels,
                        selected.min(labels.len() - 1),
                        196.0 * scale,
                        34.0 * scale,
                        12.0 * scale,
                    );
                    if let Some(value) = choice {
                        self.apply_date_setting(
                            index,
                            if index == 9 {
                                [0, 1, 2, 3, 7, 4, 5, 6][value]
                            } else {
                                value as u8
                            },
                        );
                    }
                },
            );
        }
    }

    fn draw_standby_overlay_row(
        &mut self,
        ui: &mut egui::Ui,
        list: &AdaptiveSettingsListViewport,
        _dark: bool,
        scale: f32,
        group: u8,
    ) {
        let (key, _color_kind, mut visible, confirmed, qsid) = match group {
            0 => (
                "display_settings.clock_element",
                2,
                self.display_settings.clock_visible,
                self.display_settings.confirmed_clock_visible,
                CLOCK_VISIBLE_QSID,
            ),
            1 => (
                "display_settings.layer_language_element",
                4,
                self.display_settings.clock_info_visible,
                self.display_settings.confirmed_clock_info_visible,
                CLOCK_INFO_VISIBLE_QSID,
            ),
            _ => (
                "display_settings.song_element",
                7,
                self.display_settings.date[11] != 0,
                self.display_settings.confirmed_date[11] != 0,
                DATE_QSIDS[11],
            ),
        };
        let label = crate::i18n::tr_catalog(self.app_settings.language, key);
        let mut changed = false;
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            label,
            true,
            46.0 * scale,
            |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    changed = crate::ui_style::settings_switch_sized_stable(
                        ui,
                        ("standby_overlay_visible", group),
                        &mut visible,
                        Vec2::new(46.0 * scale, 24.0 * scale),
                    )
                    .changed();
                });
            },
        );
        if changed {
            match group {
                0 => self.display_settings.clock_visible = visible,
                1 => self.display_settings.clock_info_visible = visible,
                _ => self.display_settings.date[11] = u8::from(visible),
            }
            self.queue_display_setting_write(
                label.into(),
                qsid,
                u16::from(confirmed),
                u16::from(visible),
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn restore_pictogram_editor_from_device(&mut self) {
        let pictograms = &self.display_settings.pictograms;
        let bitmap = pictograms
            .library
            .bitmap(pictograms.selected_kind, pictograms.selected_slot)
            .map(ToOwned::to_owned);
        self.set_pictogram_editor_bitmap(bitmap);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn set_pictogram_editor_bitmap(&mut self, bitmap: Option<Vec<u8>>) {
        let accent = self.display_settings.color;
        let pictograms = &mut self.display_settings.pictograms;
        pictograms.threshold = 128;
        pictograms.inverted = false;
        pictograms.source_file_name.clear();
        if let Some(ref bitmap) = bitmap {
            pictograms.selected_builtin = builtin_pictogram_index(&bitmap);
            pictograms.source_levels = pictogram_bitmap_levels(&bitmap);
            pictograms.editor_color = accent;
        } else {
            pictograms.selected_builtin = None;
            pictograms.source_levels.clear();
            pictograms.editor_color = accent;
        }
        pictograms.undo.clear();
        pictograms.selected_saved_pictogram = bitmap.as_ref().and_then(|bitmap| {
            self.app_settings
                .saved_pictograms
                .iter()
                .position(|p| &p.bitmap == bitmap)
        });
        pictograms.editor_name = if let Some(index) = pictograms.selected_saved_pictogram {
            self.app_settings.saved_pictograms[index].name.clone()
        } else if let Some(index) = pictograms.selected_builtin {
            crate::i18n::tr_catalog(self.app_settings.language, BUILTIN_PICTOGRAM_KEYS[index])
                .to_owned()
        } else {
            crate::i18n::tr_catalog(
                self.app_settings.language,
                "display_settings.pictogram_new_name",
            )
            .to_owned()
        };
        pictograms.editor_last_cell = None;
        pictograms.upload_due = None;
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start_pictogram_load(&mut self, ctx: &egui::Context) {
        self.start_vial_hid_operation(
            ctx,
            super::vial_hid_task::VialHidOperation::PictogramLoad {
                preserve_editor: self.display_settings.pictograms.preserve_editor_on_load
                    || self.display_settings.pictograms.supported == Some(true),
            },
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn apply_current_pictogram(&mut self, ctx: &egui::Context) -> bool {
        if !self.display_settings.pictograms.loaded || self.display_settings.pictograms.busy() {
            return false;
        }
        let accent = self.display_settings.color;
        let pictograms = &self.display_settings.pictograms;
        if pictograms.source_levels.is_empty() {
            return false;
        }
        let bitmap = quantize_pictogram(
            &pictograms.source_levels,
            pictograms.threshold,
            pictograms.inverted,
        );
        let mut library = pictograms.library.clone();
        library.set_colored(
            pictograms.selected_kind,
            pictograms.selected_slot,
            &bitmap,
            accent,
        );
        let upload = library.clone();
        if matches!(
            self.start_vial_hid_operation(
                ctx,
                super::vial_hid_task::VialHidOperation::PictogramSlotUpload {
                    library: upload,
                    kind: pictograms.selected_kind,
                    slot: pictograms.selected_slot,
                },
            ),
            super::vial_hid_task::VialHidTaskStart::Started
        ) {
            self.display_settings.pictograms.saving = true;
            self.display_settings.pictograms.upload_due = None;
            true
        } else {
            false
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn assign_selected_pictogram(&mut self, ctx: &egui::Context, bitmap: Option<&[u8]>) -> bool {
        if !self.display_settings.pictograms.loaded || self.display_settings.pictograms.busy() {
            return false;
        }
        let pictograms = &self.display_settings.pictograms;
        let kind = pictograms.selected_kind;
        let slot = pictograms.selected_slot;
        let mut library = pictograms.library.clone();
        if let Some(bitmap) = bitmap {
            library.set_colored(kind, slot, bitmap, self.display_settings.color);
        } else {
            library.clear(kind, slot);
        }
        let upload = library.clone();
        if matches!(
            self.start_vial_hid_operation(
                ctx,
                super::vial_hid_task::VialHidOperation::PictogramSlotUpload {
                    library: upload,
                    kind,
                    slot,
                },
            ),
            super::vial_hid_task::VialHidTaskStart::Started
        ) {
            self.display_settings.pictograms.saving = true;
            true
        } else {
            false
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn remember_pictogram_undo(&mut self) {
        let p = &mut self.display_settings.pictograms;
        let levels = pictogram_bitmap_levels(&quantize_pictogram(
            &p.source_levels,
            p.threshold,
            p.inverted,
        ));
        if p.undo.len() >= 64 {
            p.undo.remove(0);
        }
        p.undo.push(levels);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn clear_pictogram_editor(&mut self) {
        self.remember_pictogram_undo();
        self.detach_builtin_pictogram_for_edit();
        let pictograms = &mut self.display_settings.pictograms;
        pictograms.source_levels = vec![0; PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT];
        pictograms.source_file_name.clear();
        pictograms.selected_builtin = None;
        pictograms.threshold = 128;
        pictograms.inverted = false;
        pictograms.editor_last_cell = None;
        pictograms.upload_due = None;
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn detach_builtin_pictogram_for_edit(&mut self) {
        if self.display_settings.pictograms.selected_builtin.is_none() {
            return;
        }
        let lang = self.app_settings.language;
        let base_name = self.display_settings.pictograms.editor_name.trim();
        let base_name = if base_name.is_empty() {
            crate::i18n::tr_catalog(lang, "display_settings.pictogram_new_name")
        } else {
            base_name
        };
        let copy_name = crate::i18n::tr_catalog_format(
            lang,
            "display_settings.pictogram_copy_name",
            &[("name", base_name)],
        );
        let builtin_names: Vec<String> = BUILTIN_PICTOGRAM_KEYS
            .iter()
            .map(|key| crate::i18n::tr_catalog(lang, key).to_owned())
            .collect();
        let mut candidate = copy_name.clone();
        let mut suffix = 2usize;
        while pictogram_name_error(
            &candidate,
            &self.app_settings.saved_pictograms,
            None,
            &builtin_names,
        )
        .is_some()
        {
            candidate = format!("{copy_name} {suffix}");
            suffix += 1;
        }
        let pictograms = &mut self.display_settings.pictograms;
        pictograms.selected_builtin = None;
        pictograms.selected_saved_pictogram = None;
        pictograms.editor_name = candidate;
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn reset_current_pictogram(&mut self, ctx: &egui::Context) -> bool {
        if !self.display_settings.pictograms.loaded || self.display_settings.pictograms.busy() {
            return false;
        }
        let pictograms = &self.display_settings.pictograms;
        let mut library = pictograms.library.clone();
        library.clear(pictograms.selected_kind, pictograms.selected_slot);
        let upload = library.clone();
        if matches!(
            self.start_vial_hid_operation(
                ctx,
                super::vial_hid_task::VialHidOperation::PictogramSlotUpload {
                    library: upload,
                    kind: pictograms.selected_kind,
                    slot: pictograms.selected_slot,
                },
            ),
            super::vial_hid_task::VialHidTaskStart::Started
        ) {
            let pictograms = &mut self.display_settings.pictograms;
            pictograms.saving = true;
            pictograms.source_levels.clear();
            pictograms.source_file_name.clear();
            pictograms.selected_builtin = None;
            pictograms.selected_saved_pictogram = None;
            pictograms.editor_name.clear();
            pictograms.threshold = 128;
            pictograms.inverted = false;
            pictograms.upload_due = None;
            true
        } else {
            false
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn draw_pictogram_editor_panel(&mut self, ui: &mut egui::Ui, available_height: f32) {
        let dark = ui.visuals().dark_mode;
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.pictogram_editor",
                ))
                .strong(),
            );
            ui.add_space(8.0);
            let side = (ui.available_width() - 24.0)
                .min(available_height - 52.0)
                .max(96.0);
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(side, side), Sense::click_and_drag());
            let bg = self.display_settings.background_color;
            ui.painter()
                .rect_filled(rect, 4.0, Color32::from_rgb(bg[0], bg[1], bg[2]));
            if !self.display_settings.pictograms.source_levels.is_empty() {
                let bitmap = quantize_pictogram(
                    &self.display_settings.pictograms.source_levels,
                    self.display_settings.pictograms.threshold,
                    self.display_settings.pictograms.inverted,
                );
                // Assigned monochrome icons follow the live display color, not stored RGB.
                let color = self.display_settings.color;
                paint_monochrome_pictogram(
                    ui,
                    rect,
                    &bitmap,
                    Color32::from_rgb(color[0], color[1], color[2]),
                    Color32::TRANSPARENT,
                );
            }
            let cell = side / PICTOGRAM_WIDTH as f32;
            let grid_color = crate::ui_style::border_color(dark).gamma_multiply(0.55);
            for index in 0..=PICTOGRAM_WIDTH {
                let x = rect.left() + index as f32 * cell;
                let y = rect.top() + index as f32 * cell;
                ui.painter().line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    Stroke::new(0.7, grid_color),
                );
                ui.painter().line_segment(
                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    Stroke::new(0.7, grid_color),
                );
            }
            ui.painter().rect_stroke(
                rect,
                4.0,
                Stroke::new(1.0, crate::ui_style::border_color(dark)),
                egui::StrokeKind::Inside,
            );

            let pointer = response.interact_pointer_pos();
            let primary = ui.input(|input| input.pointer.primary_down());
            let secondary = ui.input(|input| input.pointer.secondary_down());
            if let Some(pointer) = pointer.filter(|point| rect.contains(*point)) {
                if primary || secondary || response.clicked() {
                    let column = (((pointer.x - rect.left()) / cell).floor() as usize)
                        .min(PICTOGRAM_WIDTH - 1) as u8;
                    let row = (((pointer.y - rect.top()) / cell).floor() as usize)
                        .min(PICTOGRAM_HEIGHT - 1) as u8;
                    let current = (column, row);
                    let draw_foreground = if secondary {
                        false
                    } else {
                        self.display_settings.pictograms.editor_draw_foreground
                    };
                    if self.display_settings.pictograms.source_levels.len()
                        != PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT
                    {
                        self.display_settings.pictograms.source_levels =
                            vec![0; PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT];
                    } else if self.display_settings.pictograms.inverted
                        || self.display_settings.pictograms.threshold != 128
                    {
                        let bitmap = quantize_pictogram(
                            &self.display_settings.pictograms.source_levels,
                            self.display_settings.pictograms.threshold,
                            self.display_settings.pictograms.inverted,
                        );
                        self.display_settings.pictograms.source_levels =
                            pictogram_bitmap_levels(&bitmap);
                        self.display_settings.pictograms.threshold = 128;
                        self.display_settings.pictograms.inverted = false;
                    }
                    let from = self
                        .display_settings
                        .pictograms
                        .editor_last_cell
                        .unwrap_or(current);
                    if self.display_settings.pictograms.editor_last_cell.is_none() {
                        self.remember_pictogram_undo();
                    }
                    self.detach_builtin_pictogram_for_edit();
                    for (x, y) in pictogram_editor_line(from, current) {
                        let index = usize::from(y) * PICTOGRAM_WIDTH + usize::from(x);
                        self.display_settings.pictograms.source_levels[index] =
                            if draw_foreground { 255 } else { 0 };
                    }
                    let pictograms = &mut self.display_settings.pictograms;
                    pictograms.editor_last_cell = Some(current);
                    pictograms.selected_builtin = None;
                    pictograms.source_file_name.clear();
                    pictograms.upload_due = None;
                }
            }
            if !primary && !secondary {
                self.display_settings.pictograms.editor_last_cell = None;
            }
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn draw_pictogram_library_panel(
        &mut self,
        ui: &mut egui::Ui,
        list: &AdaptiveSettingsListViewport,
        dark: bool,
        scale: f32,
    ) {
        let lang = self.app_settings.language;
        let busy =
            self.display_settings.pictograms.busy() || self.vial_hid_task_blocks_user_action();
        let supported = self.display_settings.pictograms.supported != Some(false);
        let kind_labels = vec![
            crate::i18n::tr_catalog(lang, "display_settings.pictogram_macros").to_owned(),
            crate::i18n::tr_catalog(lang, "display_settings.pictogram_tap_dance").to_owned(),
        ];
        let mut changed_slot = false;
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(lang, "display_settings.choose_action_type"),
            true,
            196.0 * scale,
            |ui| {
                let (_, choice) = crate::ui_style::modern_dropdown_select_sized(
                    ui,
                    ui.make_persistent_id("pictogram_kind"),
                    &kind_labels,
                    usize::from(
                        self.display_settings.pictograms.selected_kind == PictogramKind::TapDance,
                    ),
                    196.0 * scale,
                    34.0 * scale,
                    12.0 * scale,
                );
                if let Some(choice) = choice {
                    self.display_settings.pictograms.selected_kind = if choice == 0 {
                        PictogramKind::Macro
                    } else {
                        PictogramKind::TapDance
                    };
                    self.display_settings.pictograms.selected_slot = 0;
                    changed_slot = true;
                }
            },
        );
        let kind = self.display_settings.pictograms.selected_kind;
        let slot_labels: Vec<String> = match kind {
            PictogramKind::Macro => (0..self.keycode_picker.macro_count.min(256))
                .map(|i| macro_display_name(&self.keycode_picker.macro_names, i))
                .collect(),
            PictogramKind::TapDance => (0..self.keycode_picker.tap_dance_entries.len().min(256))
                .map(|i| tap_dance_display_name(&self.keycode_picker.tap_dance_names, i))
                .collect(),
        };
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(
                lang,
                if kind == PictogramKind::Macro {
                    "display_settings.choose_macro"
                } else {
                    "display_settings.choose_tap_dance"
                },
            ),
            true,
            196.0 * scale,
            |ui| {
                if !slot_labels.is_empty() {
                    let (_, choice) = crate::ui_style::modern_dropdown_select_sized(
                        ui,
                        ui.make_persistent_id("pictogram_slot"),
                        &slot_labels,
                        self.display_settings
                            .pictograms
                            .selected_slot
                            .min(slot_labels.len() - 1),
                        196.0 * scale,
                        34.0 * scale,
                        12.0 * scale,
                    );
                    if let Some(slot) = choice {
                        self.display_settings.pictograms.selected_slot = slot;
                        changed_slot = true;
                    }
                }
            },
        );
        if changed_slot {
            self.restore_pictogram_editor_from_device();
        }
        let selected_bitmap = self
            .display_settings
            .pictograms
            .library
            .bitmap(kind, self.display_settings.pictograms.selected_slot)
            .map(ToOwned::to_owned);
        // Catalog/filter work belongs to the open popup, not every editor frame.
        // Saved records are read afresh there: edits/imports to name, RGB or bitmap
        // need no revision bookkeeping and can never leave stale filtered entries.
        let picker_color = self.display_settings.color;
        let picker_background = self.display_settings.background_color;
        let mut assignment_choice: Option<Option<Vec<u8>>> = None;
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(lang, "display_settings.choose_pictogram"),
            true,
            196.0 * scale,
            |ui| {
                let picker_id = ui.make_persistent_id("pictogram_assignment_picker_v1");
                let picker_response = crate::ui_style::modern_button(
                    ui,
                    crate::i18n::tr_catalog(lang, "display_settings.choose"),
                    Vec2::new(196.0 * scale, 34.0 * scale),
                    supported && !busy,
                );
                if picker_response.clicked() {
                    if self.display_settings.pictograms.loaded {
                        egui::Popup::toggle_id(ui.ctx(), picker_id);
                    } else {
                        // Retry the read explicitly before offering assignments
                        // based on an uncertain device snapshot.
                        self.start_pictogram_load(ui.ctx());
                    }
                }
                let popup_width = 6.0 * 48.0 * scale + 5.0 * 7.0 * scale + 20.0 * scale;
                crate::ui_style::popup_below_widget_with_width(
                    ui,
                    picker_id,
                    &picker_response,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    popup_width,
                    |ui| {
                        let picker_builtins = pictogram_catalog(lang);
                        let picker_saved = visible_saved_pictograms(
                            &self.app_settings.saved_pictograms,
                            picker_builtins,
                        );
                        ui.style_mut().interaction.tooltip_delay = 1.0;
                        ui.label(
                            RichText::new(crate::i18n::tr_catalog(
                                lang,
                                "display_settings.pictogram_assign_title",
                            ))
                            .strong(),
                        );
                        ui.add_space(5.0 * scale);
                        ui.add(
                            egui::TextEdit::singleline(
                                &mut self.display_settings.pictograms.search_query,
                            )
                            .hint_text(crate::i18n::tr_catalog(
                                lang,
                                "display_settings.pictogram_search",
                            ))
                            .desired_width(f32::INFINITY),
                        );
                        ui.add_space(5.0 * scale);
                        let search = self
                            .display_settings
                            .pictograms
                            .search_query
                            .trim()
                            .to_lowercase();
                        egui::ScrollArea::vertical()
                            .id_salt("pictogram_assignment_picker_scroll_v1")
                            .max_height(260.0 * scale)
                            .show(ui, |ui| {
                                if draw_no_pictogram_tile(
                                    ui,
                                    dark,
                                    scale,
                                    crate::i18n::tr_catalog(
                                        lang,
                                        "display_settings.pictogram_none",
                                    ),
                                    selected_bitmap.is_none(),
                                ) {
                                    assignment_choice = Some(None);
                                }
                                if let Some((_, _, bitmap)) = draw_grouped_catalog(
                                    ui,
                                    lang,
                                    dark,
                                    scale,
                                    &search,
                                    picker_color,
                                    picker_background,
                                    selected_bitmap.as_deref(),
                                    6,
                                    "assignment",
                                    &picker_builtins,
                                ) {
                                    assignment_choice = Some(Some(bitmap));
                                }
                                if !picker_saved.is_empty() {
                                    ui.separator();
                                    ui.label(
                                        RichText::new(crate::i18n::tr_catalog(
                                            lang,
                                            "display_settings.pictogram_group_saved",
                                        ))
                                        .strong(),
                                    );
                                    egui::Grid::new("assignment_saved")
                                        .num_columns(6)
                                        .spacing(egui::vec2(7.0 * scale, 7.0 * scale))
                                        .show(ui, |ui| {
                                            let mut position = 0;
                                            for icon in &picker_saved {
                                                let name = &icon.name;
                                                let bitmap = &icon.bitmap;
                                                if !search.is_empty()
                                                    && !name.to_lowercase().contains(&search)
                                                {
                                                    continue;
                                                }
                                                if draw_pictogram_library_tile(
                                                    ui,
                                                    dark,
                                                    scale,
                                                    bitmap,
                                                    picker_color,
                                                    picker_background,
                                                    name,
                                                    selected_bitmap.as_deref()
                                                        == Some(bitmap.as_slice()),
                                                ) {
                                                    assignment_choice = Some(Some(bitmap.clone()));
                                                }
                                                position += 1;
                                                if position % 6 == 0 {
                                                    ui.end_row();
                                                }
                                            }
                                        });
                                }
                            });
                    },
                );
                if assignment_choice.is_some() {
                    egui::Popup::close_id(ui.ctx(), picker_id);
                }
            },
        );
        if let Some(bitmap) = assignment_choice {
            if self.assign_selected_pictogram(ui.ctx(), bitmap.as_deref()) {
                self.set_pictogram_editor_bitmap(bitmap);
            }
        }
        let (mut name_row, _) = ui.allocate_exact_size(
            egui::vec2(list.row_content_width, list.row_height),
            egui::Sense::hover(),
        );
        let field = egui::Rect::from_min_size(
            egui::pos2(
                name_row.right() - 196.0 * scale,
                name_row.top() + (list.row_height - 34.0 * scale) / 2.0,
            ),
            egui::vec2(196.0 * scale, 34.0 * scale),
        );
        let builtin_names = pictogram_builtin_names(lang);
        let p = &self.display_settings.pictograms;
        let duplicate =
            !pictogram_is_builtin_selection(&p.editor_name, p.selected_builtin, &builtin_names)
                && pictogram_name_error(
                    &p.editor_name,
                    &self.app_settings.saved_pictograms,
                    p.selected_saved_pictogram,
                    &builtin_names,
                ) == Some("display_settings.pictogram_name_duplicate_error");
        crate::ui_style::allocate_ui_at_rect(ui, field, |ui| {
            if duplicate {
                ui.visuals_mut().override_text_color = Some(Color32::from_rgb(190, 63, 69));
            }
            if crate::ui_style::modern_text_field_sized(
                ui,
                ui.make_persistent_id("pictogram_editor_name"),
                &mut self.display_settings.pictograms.editor_name,
                field.width(),
                field.height(),
                "",
                128,
                egui::Align::LEFT,
            )
            .changed()
            {
                ui.ctx().request_repaint();
            }
        });
        let p = &self.display_settings.pictograms;
        let is_builtin =
            pictogram_is_builtin_selection(&p.editor_name, p.selected_builtin, &builtin_names);
        let error = (!is_builtin && !p.editor_name.trim().is_empty())
            .then(|| {
                pictogram_name_error(
                    &p.editor_name,
                    &self.app_settings.saved_pictograms,
                    p.selected_saved_pictogram,
                    &builtin_names,
                )
            })
            .flatten();
        ui.painter().text(
            egui::pos2(name_row.left() + 2.0, field.center().y),
            egui::Align2::LEFT_CENTER,
            crate::i18n::tr_catalog(lang, "display_settings.pictogram_name"),
            egui::FontId::proportional(13.0 * scale),
            ui.visuals().text_color(),
        );
        ui.interact(
            egui::Rect::from_min_max(
                name_row.min,
                egui::pos2(field.left() - 8.0 * scale, name_row.bottom()),
            ),
            ui.make_persistent_id("pictogram_name_help"),
            Sense::hover(),
        )
        .on_hover_text(crate::i18n::tr_catalog(
            lang,
            "display_settings.tooltip_pictogram_name",
        ));
        if let Some(key) =
            error.filter(|key| *key != "display_settings.pictogram_name_duplicate_error")
        {
            name_row.max.y += 26.0 * scale;
            let rect = egui::Rect::from_min_max(
                egui::pos2(field.left(), field.bottom() + 3.0 * scale),
                egui::pos2(field.right(), name_row.bottom() - 3.0 * scale),
            );
            let color = Color32::from_rgb(190, 63, 69);
            let text = ui.painter().layout(
                crate::i18n::tr_catalog(lang, key).to_owned(),
                egui::FontId::proportional(10.0 * scale),
                color,
                rect.width(),
            );
            ui.painter()
                .with_clip_rect(rect.intersect(ui.clip_rect()))
                .galley(rect.min, text, color);
        }
        ui.painter().line_segment(
            [name_row.left_bottom(), name_row.right_bottom()],
            Stroke::new(
                1.0,
                crate::ui_style::border_color(dark).gamma_multiply(if dark { 0.72 } else { 0.9 }),
            ),
        );
        ui.advance_cursor_after_rect(name_row);
        ui.add_space(10.0 * scale);
        let tools = [
            "display_settings.pictogram_pencil",
            "display_settings.pictogram_eraser",
        ]
        .map(|key| crate::i18n::tr_catalog(lang, key).to_owned());
        ui.horizontal(|ui| {
            if let Some(selected) = crate::ui_style::settings_segmented_control(
                ui,
                "pictogram_tools",
                &tools,
                usize::from(!self.display_settings.pictograms.editor_draw_foreground),
                Vec2::new(248.0 * scale, 34.0 * scale),
            ) {
                self.display_settings.pictograms.editor_draw_foreground = selected == 0;
            }
        });

        ui.add_space(16.0 * scale);
        ui.horizontal(|ui| {
            for (export, key) in [
                (false, "display_settings.import_icons"),
                (true, "display_settings.export_icons"),
            ] {
                if crate::ui_style::modern_button(
                    ui,
                    crate::i18n::tr_catalog(lang, key),
                    Vec2::new((list.row_content_width - 8.0) / 2.0, 34.0 * scale),
                    self.pending_file_dialog.is_none(),
                )
                .clicked()
                {
                    self.spawn_file_dialog(
                        if export {
                            crate::app::file_dialog::FileDialogAction::ExportPictograms
                        } else {
                            crate::app::file_dialog::FileDialogAction::ImportPictograms
                        },
                        rfd::FileDialog::new()
                            .add_filter("Entropy pictograms", &["json"])
                            .set_file_name("entropy-pictograms.json"),
                        export,
                    );
                }
            }
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn draw_pictogram_footer(&mut self, ui: &mut egui::Ui, scale: f32) {
        let lang = self.app_settings.language;
        let busy =
            self.display_settings.pictograms.busy() || self.vial_hid_task_blocks_user_action();
        let names = pictogram_builtin_names(lang);
        let name_error = pictogram_name_error(
            &self.display_settings.pictograms.editor_name,
            &self.app_settings.saved_pictograms,
            self.display_settings.pictograms.selected_saved_pictogram,
            &names,
        );
        let can_delete =
            can_delete_saved_pictogram(self.display_settings.pictograms.selected_saved_pictogram);
        let can_save = name_error.is_none()
            && !pictogram_is_builtin_selection(
                &self.display_settings.pictograms.editor_name,
                self.display_settings.pictograms.selected_builtin,
                &names,
            )
            && !self.display_settings.pictograms.source_levels.is_empty();

        let mut clear_clicked = false;
        let mut undo_clicked = false;
        let mut delete_clicked = false;
        let mut save_clicked = false;
        ui.horizontal(|ui| {
            let size = crate::ui_style::modal_action_button_size() * scale;
            ui.spacing_mut().item_spacing.x = 8.0 * scale;
            clear_clicked = crate::ui_style::modern_button(
                ui,
                crate::i18n::tr_catalog(lang, "display_settings.pictogram_clear"),
                size,
                true,
            )
            .clicked();
            undo_clicked = crate::ui_style::modern_button(
                ui,
                crate::i18n::tr_catalog(lang, "display_settings.undo"),
                size,
                !self.display_settings.pictograms.undo.is_empty(),
            )
            .clicked();
            delete_clicked = crate::ui_style::modern_button(
                ui,
                crate::i18n::tr_catalog(lang, "display_settings.pictogram_remove"),
                size,
                can_delete,
            )
            .clicked();
            save_clicked = crate::ui_style::modern_button(
                ui,
                crate::i18n::tr_catalog(lang, "display_settings.pictogram_save_library"),
                size,
                can_save && !busy,
            )
            .clicked();
        });
        // The drawing editor owns undo only while no text editor or picker
        // owns keyboard input. TextEdit must retain its own name/search undo.
        let undo_shortcut = !ui.ctx().egui_wants_keyboard_input()
            && !self.modal_or_popup_open(ui.ctx())
            && self.pending_file_dialog.is_none()
            && self.display_settings.pictograms.editor_last_cell.is_none()
            && ui.input_mut(|input| {
                // egui's consume_shortcut ignores extra Shift/Alt. Match the
                // exact chord so redo (Command+Shift+Z) never becomes undo.
                let event = input.events.iter().position(|event| {
                    matches!(
                        event,
                        egui::Event::Key { key: egui::Key::Z, pressed: true, modifiers, .. }
                            if modifiers.matches_exact(egui::Modifiers::COMMAND)
                    )
                });
                if let Some(index) = event {
                    input.events.remove(index);
                    true
                } else {
                    false
                }
            });
        if undo_clicked || undo_shortcut {
            if let Some(levels) = self.display_settings.pictograms.undo.pop() {
                self.display_settings.pictograms.source_levels = levels;
            }
        }
        if clear_clicked {
            self.clear_pictogram_editor();
        }
        if delete_clicked {
            if let Some(index) = self.display_settings.pictograms.selected_saved_pictogram {
                if index < self.app_settings.saved_pictograms.len() {
                    self.app_settings.saved_pictograms.remove(index);
                    self.display_settings.pictograms.selected_saved_pictogram = None;
                    self.display_settings.pictograms.editor_name.clear();
                    save_app_settings(&self.app_settings);
                    self.status_msg =
                        crate::i18n::tr_catalog(lang, "display_settings.pictogram_library_deleted")
                            .into();
                }
            }
        }
        if save_clicked {
            let preset = SavedPictogram {
                origin: PictogramOrigin::User,
                name: self
                    .display_settings
                    .pictograms
                    .editor_name
                    .trim()
                    .to_owned(),
                color: self.display_settings.color,
                bitmap: quantize_pictogram(
                    &self.display_settings.pictograms.source_levels,
                    self.display_settings.pictograms.threshold,
                    self.display_settings.pictograms.inverted,
                )
                .to_vec(),
            };
            if let Some(index) = self.display_settings.pictograms.selected_saved_pictogram {
                if let Some(existing) = self.app_settings.saved_pictograms.get_mut(index) {
                    *existing = preset;
                }
            } else {
                self.app_settings.saved_pictograms.push(preset);
                self.display_settings.pictograms.selected_saved_pictogram =
                    Some(self.app_settings.saved_pictograms.len() - 1);
            }
            self.display_settings.pictograms.selected_builtin = None;
            save_app_settings(&self.app_settings);
            self.apply_current_pictogram(ui.ctx());
            self.status_msg =
                crate::i18n::tr_catalog(lang, "display_settings.pictogram_library_saved").into();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    fn draw_pictogram_library_panel_legacy(
        &mut self,
        ui: &mut egui::Ui,
        list: &AdaptiveSettingsListViewport,
        dark: bool,
        scale: f32,
    ) {
        let lang = self.app_settings.language;
        let busy =
            self.display_settings.pictograms.busy() || self.vial_hid_task_blocks_user_action();
        let supported = self.display_settings.pictograms.supported != Some(false);
        ui.set_min_width(list.row_content_width);

        let kind_labels = vec![
            crate::i18n::tr_catalog(lang, "display_settings.pictogram_macros").to_owned(),
            crate::i18n::tr_catalog(lang, "display_settings.pictogram_tap_dance").to_owned(),
        ];
        let kind = self.display_settings.pictograms.selected_kind;
        let slot_labels: Vec<String> = match kind {
            PictogramKind::Macro => (0..self.keycode_picker.macro_count.min(256))
                .map(|index| macro_display_name(&self.keycode_picker.macro_names, index))
                .collect(),
            PictogramKind::TapDance => (0..self.keycode_picker.tap_dance_entries.len().min(256))
                .map(|index| tap_dance_display_name(&self.keycode_picker.tap_dance_names, index))
                .collect(),
        };
        ui.horizontal(|ui| {
            ui.add_enabled_ui(supported && !busy, |ui| {
                let (_, picked_kind) = crate::ui_style::modern_dropdown_select_sized(
                    ui,
                    ui.make_persistent_id("pictogram_grid_kind"),
                    &kind_labels,
                    usize::from(kind == PictogramKind::TapDance),
                    138.0 * scale,
                    32.0 * scale,
                    12.0 * scale,
                );
                if let Some(choice) = picked_kind {
                    self.display_settings.pictograms.selected_kind = if choice == 0 {
                        PictogramKind::Macro
                    } else {
                        PictogramKind::TapDance
                    };
                    self.display_settings.pictograms.selected_slot = 0;
                    self.restore_pictogram_editor_from_device();
                }
                if !slot_labels.is_empty() {
                    let (_, picked_slot) = crate::ui_style::modern_dropdown_select_sized(
                        ui,
                        ui.make_persistent_id("pictogram_grid_slot"),
                        &slot_labels,
                        self.display_settings
                            .pictograms
                            .selected_slot
                            .min(slot_labels.len() - 1),
                        138.0 * scale,
                        32.0 * scale,
                        12.0 * scale,
                    );
                    if let Some(slot) = picked_slot {
                        self.display_settings.pictograms.selected_slot = slot;
                        self.restore_pictogram_editor_from_device();
                    }
                }
            });
        });
        ui.add_space(8.0 * scale);
        ui.label(
            RichText::new(crate::i18n::tr_catalog(
                lang,
                "display_settings.pictogram_library",
            ))
            .strong(),
        );
        let saved = self.app_settings.saved_pictograms.clone();
        let accent = self.display_settings.color;
        let mut chosen: Option<(Vec<u8>, [u8; 3], String, Option<usize>)> = None;
        egui::ScrollArea::vertical()
            .id_salt("pictogram_library_grid")
            .max_height(190.0 * scale)
            .show(ui, |ui| {
                ui.style_mut().interaction.tooltip_delay = 1.0;
                let catalog = pictogram_catalog(lang);
                if let Some((index, name, bitmap)) = draw_grouped_catalog(
                    ui,
                    lang,
                    dark,
                    scale,
                    "",
                    accent,
                    self.display_settings.background_color,
                    None,
                    5,
                    "editor",
                    &catalog,
                ) {
                    if !busy {
                        chosen = Some((bitmap, accent, name, Some(index)));
                    }
                }
                let saved_visible = visible_saved_pictograms(&saved, &catalog);
                if !saved_visible.is_empty() {
                    ui.separator();
                    ui.label(
                        RichText::new(crate::i18n::tr_catalog(
                            lang,
                            "display_settings.pictogram_group_saved",
                        ))
                        .strong(),
                    );
                    egui::Grid::new("editor_saved")
                        .num_columns(5)
                        .spacing(egui::vec2(7.0 * scale, 7.0 * scale))
                        .show(ui, |ui| {
                            for (position, preset) in saved_visible.into_iter().enumerate() {
                                if draw_pictogram_library_tile(
                                    ui,
                                    dark,
                                    scale,
                                    &preset.bitmap,
                                    preset.color,
                                    self.display_settings.background_color,
                                    &preset.name,
                                    false,
                                ) && !busy
                                {
                                    chosen = Some((
                                        preset.bitmap.clone(),
                                        preset.color,
                                        preset.name.clone(),
                                        None,
                                    ));
                                }
                                if (position + 1) % 5 == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                }
            });
        if let Some((bitmap, color, name, builtin)) = chosen {
            let pictograms = &mut self.display_settings.pictograms;
            pictograms.source_levels = pictogram_bitmap_levels(&bitmap);
            pictograms.threshold = 128;
            pictograms.inverted = false;
            pictograms.editor_color = color;
            pictograms.editor_name = name;
            pictograms.selected_builtin = builtin;
            pictograms.source_file_name.clear();
            self.apply_current_pictogram(ui.ctx());
        }

        ui.add_space(8.0 * scale);
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.display_settings.pictograms.editor_draw_foreground,
                true,
                crate::i18n::tr_catalog(lang, "display_settings.pictogram_pencil"),
            );
            ui.selectable_value(
                &mut self.display_settings.pictograms.editor_draw_foreground,
                false,
                crate::i18n::tr_catalog(lang, "display_settings.pictogram_eraser"),
            );
            if ui
                .add_enabled(
                    supported && !busy && self.pending_file_dialog.is_none(),
                    egui::Button::new(crate::i18n::tr_catalog(
                        lang,
                        "display_settings.pictogram_choose",
                    )),
                )
                .clicked()
            {
                self.spawn_file_dialog(
                    crate::app::file_dialog::FileDialogAction::Pictogram,
                    rfd::FileDialog::new().add_filter(
                        crate::i18n::tr_catalog(lang, "display_settings.pictogram_files"),
                        &["png", "jpg", "jpeg", "webp", "bmp"],
                    ),
                    false,
                );
            }
            if ui
                .add_enabled(
                    supported && !busy,
                    egui::Button::new(crate::i18n::tr_catalog(
                        lang,
                        "display_settings.pictogram_reset",
                    )),
                )
                .clicked()
            {
                self.reset_current_pictogram(ui.ctx());
            }
        });

        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr_catalog(
                lang,
                "display_settings.pictogram_color",
            ));
            let color = self.display_settings.pictograms.editor_color;
            let mut picked = Color32::from_rgb(color[0], color[1], color[2]);
            if egui::color_picker::color_edit_button_srgba(
                ui,
                &mut picked,
                egui::color_picker::Alpha::Opaque,
            )
            .changed()
            {
                self.display_settings.pictograms.editor_color =
                    [picked.r(), picked.g(), picked.b()];
                self.display_settings.pictograms.upload_due =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(250));
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(260));
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.display_settings.pictograms.editor_name)
                    .hint_text(crate::i18n::tr_catalog(
                        lang,
                        "display_settings.pictogram_name",
                    ))
                    .desired_width(170.0 * scale),
            );
            let can_save = !self
                .display_settings
                .pictograms
                .editor_name
                .trim()
                .is_empty()
                && !self.display_settings.pictograms.source_levels.is_empty();
            if ui
                .add_enabled(
                    can_save,
                    egui::Button::new(crate::i18n::tr_catalog(
                        lang,
                        "display_settings.pictogram_save_library",
                    )),
                )
                .clicked()
            {
                let name = self
                    .display_settings
                    .pictograms
                    .editor_name
                    .trim()
                    .to_owned();
                let bitmap = quantize_pictogram(
                    &self.display_settings.pictograms.source_levels,
                    self.display_settings.pictograms.threshold,
                    self.display_settings.pictograms.inverted,
                );
                let preset = SavedPictogram {
                    origin: PictogramOrigin::User,
                    name: name.clone(),
                    color: self.display_settings.pictograms.editor_color,
                    bitmap: bitmap.to_vec(),
                };
                if let Some(existing) = self
                    .app_settings
                    .saved_pictograms
                    .iter_mut()
                    .find(|existing| existing.name == name)
                {
                    *existing = preset;
                } else {
                    self.app_settings.saved_pictograms.push(preset);
                }
                save_app_settings(&self.app_settings);
                self.status_msg =
                    crate::i18n::tr_catalog(lang, "display_settings.pictogram_library_saved")
                        .into();
            }
        });
        ui.label(
            RichText::new(if self.display_settings.pictograms.loading {
                crate::i18n::tr_catalog(lang, "display_settings.pictograms_loading")
            } else if self.display_settings.pictograms.saving {
                crate::i18n::tr_catalog(lang, "display_settings.pictograms_saving")
            } else {
                crate::i18n::tr_catalog(lang, "display_settings.pictogram_auto_apply")
            })
            .size(11.0 * scale)
            .color(app_muted_text(dark)),
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    fn draw_pictogram_settings(
        &mut self,
        ui: &mut egui::Ui,
        list: &AdaptiveSettingsListViewport,
        dark: bool,
        scale: f32,
    ) {
        let lang = self.app_settings.language;
        let busy =
            self.display_settings.pictograms.busy() || self.vial_hid_task_blocks_user_action();
        let supported = self.display_settings.pictograms.supported != Some(false);
        let kind = self.display_settings.pictograms.selected_kind;
        let slot_count = match kind {
            PictogramKind::Macro => self.keycode_picker.macro_count,
            PictogramKind::TapDance => self.keycode_picker.tap_dance_entries.len(),
        }
        .min(256);
        self.display_settings.pictograms.selected_slot = self
            .display_settings
            .pictograms
            .selected_slot
            .min(slot_count.saturating_sub(1));

        let status = if !supported {
            crate::i18n::tr_catalog(lang, "display_settings.pictograms_firmware_required")
        } else if self.display_settings.pictograms.loading {
            crate::i18n::tr_catalog(lang, "display_settings.pictograms_loading")
        } else if self.display_settings.pictograms.saving {
            crate::i18n::tr_catalog(lang, "display_settings.pictograms_saving")
        } else {
            crate::i18n::tr_catalog(lang, "display_settings.pictograms_loaded")
        };
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(lang, "display_settings.pictograms_status"),
            true,
            196.0 * scale,
            |ui| {
                ui.label(
                    RichText::new(status)
                        .size(12.0 * scale)
                        .color(app_muted_text(dark)),
                );
            },
        );

        let kind_labels = vec![
            crate::i18n::tr_catalog(lang, "display_settings.pictogram_macros").to_owned(),
            crate::i18n::tr_catalog(lang, "display_settings.pictogram_tap_dance").to_owned(),
        ];
        let mut picked_kind = None;
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(lang, "display_settings.pictogram_type"),
            supported,
            196.0 * scale,
            |ui| {
                ui.add_enabled_ui(supported && !busy, |ui| {
                    let (_, choice) = crate::ui_style::modern_dropdown_select_sized(
                        ui,
                        ui.make_persistent_id("pictogram_kind"),
                        &kind_labels,
                        usize::from(kind == PictogramKind::TapDance),
                        196.0 * scale,
                        crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx())
                            .settings_control_height(),
                        crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx())
                            .settings_control_font_size(),
                    );
                    picked_kind = choice;
                });
            },
        );
        if let Some(picked) = picked_kind {
            self.display_settings.pictograms.selected_kind = if picked == 0 {
                PictogramKind::Macro
            } else {
                PictogramKind::TapDance
            };
            self.display_settings.pictograms.selected_slot = 0;
            self.restore_pictogram_editor_from_device();
        }

        let kind = self.display_settings.pictograms.selected_kind;
        let slot_labels: Vec<String> = match kind {
            PictogramKind::Macro => (0..self.keycode_picker.macro_count.min(256))
                .map(|index| macro_display_name(&self.keycode_picker.macro_names, index))
                .collect(),
            PictogramKind::TapDance => (0..self.keycode_picker.tap_dance_entries.len().min(256))
                .map(|index| tap_dance_display_name(&self.keycode_picker.tap_dance_names, index))
                .collect(),
        };
        let mut picked_slot = None;
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(lang, "display_settings.pictogram_slot"),
            supported && !slot_labels.is_empty(),
            196.0 * scale,
            |ui| {
                if slot_labels.is_empty() {
                    ui.label(
                        RichText::new(crate::i18n::tr_catalog(
                            lang,
                            "display_settings.pictogram_no_slots",
                        ))
                        .size(12.0 * scale)
                        .color(app_muted_text(dark)),
                    );
                } else {
                    ui.add_enabled_ui(supported && !busy, |ui| {
                        let (_, choice) = crate::ui_style::modern_dropdown_select_sized(
                            ui,
                            ui.make_persistent_id("pictogram_slot"),
                            &slot_labels,
                            self.display_settings.pictograms.selected_slot,
                            196.0 * scale,
                            crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx())
                                .settings_control_height(),
                            crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx())
                                .settings_control_font_size(),
                        );
                        picked_slot = choice;
                    });
                }
            },
        );
        if let Some(slot) = picked_slot {
            self.display_settings.pictograms.selected_slot = slot;
            self.restore_pictogram_editor_from_device();
        }

        let builtin_catalog = pictogram_catalog(lang);
        let mut builtin_labels = Vec::with_capacity(builtin_catalog.len() + 2);
        builtin_labels
            .push(crate::i18n::tr_catalog(lang, "display_settings.pictogram_default").to_owned());
        builtin_labels.extend(builtin_catalog.iter().map(|icon| icon.name.clone()));
        builtin_labels
            .push(crate::i18n::tr_catalog(lang, "display_settings.pictogram_custom").to_owned());
        let builtin_selected = if self.display_settings.pictograms.source_levels.is_empty() {
            0
        } else {
            self.display_settings
                .pictograms
                .selected_builtin
                .and_then(|index| builtin_catalog.iter().position(|icon| icon.index == index))
                .map_or(builtin_catalog.len() + 1, |position| position + 1)
        };
        let mut picked_builtin = None;
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(lang, "display_settings.pictogram_builtin"),
            supported,
            196.0 * scale,
            |ui| {
                ui.add_enabled_ui(supported && !busy, |ui| {
                    let (_, choice) = crate::ui_style::modern_dropdown_select_sized(
                        ui,
                        ui.make_persistent_id("pictogram_builtin"),
                        &builtin_labels,
                        builtin_selected,
                        196.0 * scale,
                        crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx())
                            .settings_control_height(),
                        crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx())
                            .settings_control_font_size(),
                    );
                    picked_builtin = choice;
                });
            },
        );
        if let Some(choice) = picked_builtin {
            if choice == 0 {
                self.reset_current_pictogram(ui.ctx());
            } else if choice <= builtin_catalog.len() {
                let index = builtin_catalog[choice - 1].index;
                let bitmap = builtin_pictogram_bitmap(index);
                self.display_settings.pictograms.source_levels = pictogram_bitmap_levels(&bitmap);
                self.display_settings.pictograms.source_file_name.clear();
                self.display_settings.pictograms.threshold = 128;
                self.display_settings.pictograms.inverted = false;
                self.display_settings.pictograms.selected_builtin = Some(index);
                self.apply_current_pictogram(ui.ctx());
            } else if self.pending_file_dialog.is_none() {
                self.spawn_file_dialog(
                    crate::app::file_dialog::FileDialogAction::Pictogram,
                    rfd::FileDialog::new().add_filter(
                        crate::i18n::tr_catalog(lang, "display_settings.pictogram_files"),
                        &["png", "jpg", "jpeg", "webp", "bmp"],
                    ),
                    false,
                );
            }
        }

        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(lang, "display_settings.pictogram_source"),
            supported,
            260.0 * scale,
            |ui| {
                if ui
                    .add_enabled(
                        supported && !busy && self.pending_file_dialog.is_none(),
                        egui::Button::new(crate::i18n::tr_catalog(
                            lang,
                            "display_settings.pictogram_choose",
                        )),
                    )
                    .clicked()
                {
                    self.spawn_file_dialog(
                        crate::app::file_dialog::FileDialogAction::Pictogram,
                        rfd::FileDialog::new().add_filter(
                            crate::i18n::tr_catalog(lang, "display_settings.pictogram_files"),
                            &["png", "jpg", "jpeg", "webp", "bmp"],
                        ),
                        false,
                    );
                }
                if !self.display_settings.pictograms.source_file_name.is_empty() {
                    ui.label(
                        RichText::new(&self.display_settings.pictograms.source_file_name)
                            .size(11.0 * scale)
                            .color(app_muted_text(dark)),
                    );
                }
            },
        );

        let has_source = !self.display_settings.pictograms.source_levels.is_empty();
        let mut threshold = self.display_settings.pictograms.threshold as f32;
        let mut threshold_changed = false;
        let mut threshold_commit = false;
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(lang, "display_settings.pictogram_threshold"),
            supported && has_source,
            196.0 * scale,
            |ui| {
                let response = ui.add_enabled(
                    supported && !busy && has_source,
                    egui::Slider::new(&mut threshold, 0.0..=255.0)
                        .step_by(1.0)
                        .show_value(true),
                );
                threshold_changed = response.changed();
                threshold_commit =
                    response.drag_stopped() || (response.changed() && !response.dragged());
            },
        );
        if threshold_changed {
            self.display_settings.pictograms.threshold = threshold.round() as u8;
        }

        let mut inverted = self.display_settings.pictograms.inverted;
        let mut inverted_changed = false;
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(lang, "display_settings.pictogram_invert"),
            supported && has_source,
            46.0 * scale,
            |ui| {
                inverted_changed = crate::ui_style::settings_switch_sized_stable_interactive(
                    ui,
                    "pictogram_inverted",
                    &mut inverted,
                    egui::vec2(46.0 * scale, 24.0 * scale),
                    supported && !busy && has_source,
                )
                .changed();
            },
        );
        if inverted_changed {
            self.display_settings.pictograms.inverted = inverted;
        }

        let selected_slot = self.display_settings.pictograms.selected_slot;
        let source_bitmap = has_source.then(|| {
            quantize_pictogram(
                &self.display_settings.pictograms.source_levels,
                self.display_settings.pictograms.threshold,
                self.display_settings.pictograms.inverted,
            )
        });
        let stored_bitmap = self
            .display_settings
            .pictograms
            .library
            .bitmap(kind, selected_slot)
            .and_then(|bitmap| <[u8; PICTOGRAM_BYTES]>::try_from(bitmap).ok());
        let preview_bitmap = source_bitmap.as_ref().or(stored_bitmap.as_ref());
        let can_reset = supported
            && !busy
            && !slot_labels.is_empty()
            && (source_bitmap.is_some() || stored_bitmap.is_some());
        let mut reset = false;
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(lang, "display_settings.pictogram_actions"),
            supported,
            260.0 * scale,
            |ui| {
                if let Some(bitmap) = preview_bitmap {
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(44.0 * scale, 44.0 * scale),
                        Sense::hover(),
                    );
                    paint_monochrome_pictogram(
                        ui,
                        rect.shrink(2.0 * scale),
                        bitmap,
                        ui.visuals().text_color(),
                        Color32::TRANSPARENT,
                    );
                }
                ui.label(
                    RichText::new(crate::i18n::tr_catalog(
                        lang,
                        "display_settings.pictogram_auto_apply",
                    ))
                    .size(11.0 * scale)
                    .color(app_muted_text(dark)),
                );
                reset = ui
                    .add_enabled(
                        can_reset,
                        egui::Button::new(crate::i18n::tr_catalog(
                            lang,
                            "display_settings.pictogram_reset",
                        )),
                    )
                    .clicked();
            },
        );

        if reset {
            self.reset_current_pictogram(ui.ctx());
        } else if threshold_commit || inverted_changed {
            self.apply_current_pictogram(ui.ctx());
        }
    }

    fn draw_standby_background_scale_row(
        &mut self,
        ui: &mut egui::Ui,
        list: &AdaptiveSettingsListViewport,
        scale: f32,
    ) {
        let lang = self.app_settings.language;
        let labels = vec![
            crate::i18n::tr_catalog(lang, "display_settings.background_scale_fit").to_owned(),
            crate::i18n::tr_catalog(lang, "display_settings.background_scale_fill").to_owned(),
            crate::i18n::tr_catalog(lang, "display_settings.background_scale_stretch").to_owned(),
        ];
        let selected = match self.app_settings.standby_background_scale {
            StandbyBackgroundScale::Fit => 0,
            StandbyBackgroundScale::Fill => 1,
            StandbyBackgroundScale::Stretch => 2,
        };
        let mut picked = None;
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(lang, "display_settings.background_scale"),
            true,
            196.0 * scale,
            |ui| {
                let dropdown_id = ui.make_persistent_id(("display_clock", "background_scale"));
                let (_, choice) = crate::ui_style::modern_dropdown_select_sized(
                    ui,
                    dropdown_id,
                    &labels,
                    selected,
                    196.0 * scale,
                    crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx())
                        .settings_control_height(),
                    crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx())
                        .settings_control_font_size(),
                );
                picked = choice;
            },
        );
        if let Some(value) = picked {
            let value = match value {
                0 => StandbyBackgroundScale::Fit,
                1 => StandbyBackgroundScale::Fill,
                _ => StandbyBackgroundScale::Stretch,
            };
            let old_value = self.app_settings.standby_background_scale;
            if value != old_value {
                self.app_settings.standby_background_scale = value;
                let source_path = self
                    .app_settings
                    .standby_background_source_path
                    .as_ref()
                    .map(std::path::PathBuf::from);
                if let Some(path) = source_path {
                    let operation = super::vial_hid_task::VialHidOperation::BackgroundUpload {
                        path,
                        fallback: self.display_settings.clock_background_color,
                        scale: value,
                    };
                    match self.start_vial_hid_operation(ui.ctx(), operation) {
                        super::vial_hid_task::VialHidTaskStart::Started => {
                            save_app_settings(&self.app_settings);
                            self.status_msg = crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "display_settings.background_rescaling",
                            )
                            .into();
                        }
                        super::vial_hid_task::VialHidTaskStart::Busy
                        | super::vial_hid_task::VialHidTaskStart::NoDevice => {
                            self.app_settings.standby_background_scale = old_value;
                        }
                    }
                } else {
                    save_app_settings(&self.app_settings);
                    self.status_msg = crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "display_settings.background_reselect_for_scale",
                    )
                    .into();
                }
            }
        }
    }

    fn draw_display_color_swatch(
        &mut self,
        ui: &mut egui::Ui,
        dark: bool,
        scale: f32,
        color_kind: u8,
    ) {
        let popup_id = ui.make_persistent_id(match color_kind {
            1 => "display_background_color_popup",
            2 => "clock_text_color_popup",
            3 => "clock_background_color_popup",
            4 => "clock_info_color_popup",
            5 => "clock_modifiers_color_popup",
            7 => "song_color_popup",
            6 => "date_color_popup",
            _ => "display_accent_color_popup",
        });
        let popup_hsva_id = popup_id.with("hsva");
        let popup_hex_id = popup_id.with("hex");
        let popup_open = egui::Popup::is_id_open(ui.ctx(), popup_id);
        let border = if dark {
            Color32::from_gray(95)
        } else {
            Color32::from_gray(185)
        };
        let swatch_border = if popup_open { app_accent() } else { border };
        let rgb = match color_kind {
            1 => self.display_settings.background_color,
            2 => self.display_settings.clock_text_color,
            3 => self.display_settings.clock_background_color,
            4 => self.display_settings.clock_info_color,
            5 => self.display_settings.clock_modifiers_color,
            7 => [
                self.display_settings.date[12],
                self.display_settings.date[13],
                self.display_settings.date[14],
            ],
            6 => [
                self.display_settings.date[2],
                self.display_settings.date[3],
                self.display_settings.date[4],
            ],
            _ => self.display_settings.color,
        };
        let swatch_color = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        let (swatch_rect, swatch_response) =
            ui.allocate_exact_size(Vec2::new(64.0 * scale, 34.0 * scale), Sense::click());
        if swatch_response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if swatch_response.clicked() {
            let hsva = egui::ecolor::Hsva::from(swatch_color);
            ui.ctx().data_mut(|data| {
                data.insert_temp(popup_hsva_id, hsva);
                data.insert_temp(popup_hex_id, format_rgb_hex(rgb));
            });
            egui::Popup::toggle_id(ui.ctx(), popup_id);
        }

        ui.painter().rect(
            swatch_rect,
            9.0,
            app_surface_fill(dark),
            Stroke::new(1.0_f32, swatch_border),
            egui::StrokeKind::Inside,
        );
        ui.painter().rect(
            swatch_rect.shrink(5.0 * scale),
            6.0,
            swatch_color,
            Stroke::new(1.0_f32, swatch_border.gamma_multiply(0.85)),
            egui::StrokeKind::Inside,
        );
        let mut picked_hsva = ui
            .ctx()
            .data(|data| data.get_temp::<egui::ecolor::Hsva>(popup_hsva_id))
            .unwrap_or_else(|| swatch_color.into());
        crate::ui_style::popup_below_widget(
            ui,
            popup_id,
            &swatch_response,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.spacing_mut().slider_width = 136.0 * scale;
                if super::rgb_settings_ui::compact_rgb_color_picker(ui, &mut picked_hsva) {
                    let color: Color32 = picked_hsva.into();
                    let new_color = [color.r(), color.g(), color.b()];
                    self.apply_display_color(new_color, color_kind);
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(popup_hsva_id, picked_hsva);
                        data.insert_temp(popup_hex_id, format_rgb_hex(new_color));
                    });
                }
                ui.add_space(8.0 * scale);
                let mut hex = ui
                    .ctx()
                    .data(|data| data.get_temp::<String>(popup_hex_id))
                    .unwrap_or_else(|| format_rgb_hex(rgb));
                ui.vertical_centered(|ui| {
                    let response = crate::ui_style::modern_text_field_sized(
                        ui,
                        popup_hex_id.with("input"),
                        &mut hex,
                        ui.available_width(),
                        32.0 * scale,
                        "#RRGGBB",
                        7,
                        egui::Align::Center,
                    );
                    if response.changed() {
                        hex.make_ascii_uppercase();
                        if let Some(new_color) = parse_rgb_hex(&hex) {
                            picked_hsva =
                                Color32::from_rgb(new_color[0], new_color[1], new_color[2]).into();
                            self.apply_display_color(new_color, color_kind);
                            ui.ctx()
                                .data_mut(|data| data.insert_temp(popup_hsva_id, picked_hsva));
                        }
                        ui.ctx()
                            .data_mut(|data| data.insert_temp(popup_hex_id, hex.clone()));
                    }
                });
            },
        );
    }

    fn apply_display_color(&mut self, new_color: [u8; 3], color_kind: u8) {
        if color_kind == 7 {
            for (i, value) in new_color.iter().enumerate() {
                self.apply_date_setting(i + 12, *value);
            }
            return;
        }
        if color_kind == 6 {
            for (i, value) in new_color.iter().enumerate() {
                self.apply_date_setting(i + 2, *value);
            }
            return;
        }
        let current_color = match color_kind {
            1 => self.display_settings.background_color,
            2 => self.display_settings.clock_text_color,
            3 => self.display_settings.clock_background_color,
            4 => self.display_settings.clock_info_color,
            5 => self.display_settings.clock_modifiers_color,
            7 => [
                self.display_settings.date[12],
                self.display_settings.date[13],
                self.display_settings.date[14],
            ],
            6 => [
                self.display_settings.date[2],
                self.display_settings.date[3],
                self.display_settings.date[4],
            ],
            _ => self.display_settings.color,
        };
        if new_color == current_color {
            return;
        }

        match color_kind {
            1 => self.display_settings.background_color = new_color,
            2 => self.display_settings.clock_text_color = new_color,
            3 => self.display_settings.clock_background_color = new_color,
            4 => self.display_settings.clock_info_color = new_color,
            5 => self.display_settings.clock_modifiers_color = new_color,
            _ => self.display_settings.color = new_color,
        }
        let label_key = match color_kind {
            1 => "display_settings.background_color",
            2 => "display_settings.clock_text_color",
            3 => "display_settings.clock_background_color",
            4 => "display_settings.clock_info_color",
            5 => "display_settings.clock_modifiers_color",
            _ => "display_settings.accent_color",
        };
        let label = crate::i18n::tr_catalog(self.app_settings.language, label_key).to_owned();
        let qsids = match color_kind {
            1 => DISPLAY_BACKGROUND_COLOR_QSIDS,
            2 => CLOCK_TEXT_COLOR_QSIDS,
            3 => CLOCK_BACKGROUND_COLOR_QSIDS,
            4 => CLOCK_INFO_COLOR_QSIDS,
            5 => CLOCK_MODIFIERS_COLOR_QSIDS,
            _ => DISPLAY_COLOR_QSIDS,
        };
        let confirmed = match color_kind {
            1 => self.display_settings.confirmed_background_color,
            2 => self.display_settings.confirmed_clock_text_color,
            3 => self.display_settings.confirmed_clock_background_color,
            4 => self.display_settings.confirmed_clock_info_color,
            5 => self.display_settings.confirmed_clock_modifiers_color,
            _ => self.display_settings.confirmed_color,
        };
        for (index, qsid) in qsids.iter().copied().enumerate() {
            let displayed_value = self
                .pending_settings_write_value(qsid)
                .unwrap_or(confirmed[index] as u16);
            let requested = new_color[index] as u16;
            if requested != displayed_value {
                self.queue_display_setting_write(
                    label.clone(),
                    qsid,
                    confirmed[index] as u16,
                    requested,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_clock_setting_row(
        &mut self,
        ui: &mut egui::Ui,
        list: &AdaptiveSettingsListViewport,
        scale: f32,
        label_key: &'static str,
        id_salt: &'static str,
        qsid: u16,
        selected: u8,
        option_keys: &[&'static str],
        separator: bool,
    ) {
        let lang = self.app_settings.language;
        let labels: Vec<String> = option_keys
            .iter()
            .map(|key| crate::i18n::tr_catalog(lang, key).to_owned())
            .collect();
        let mut picked = None;
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(lang, label_key),
            separator,
            196.0 * scale,
            |ui| {
                let dropdown_id = ui.make_persistent_id(("display_clock", id_salt));
                let (_, choice) = crate::ui_style::modern_dropdown_select_sized(
                    ui,
                    dropdown_id,
                    &labels,
                    (if qsid == CLOCK_DELAY_QSID {
                        [0, 1, 2, 3, 7, 4, 5, 6]
                            .iter()
                            .position(|v| *v == selected)
                            .unwrap_or(4)
                    } else {
                        selected as usize
                    })
                    .min(labels.len().saturating_sub(1)),
                    196.0 * scale,
                    crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx())
                        .settings_control_height(),
                    crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx())
                        .settings_control_font_size(),
                );
                picked = choice;
            },
        );
        if let Some(value) = picked {
            self.apply_clock_setting(
                qsid,
                if qsid == CLOCK_DELAY_QSID {
                    [0, 1, 2, 3, 7, 4, 5, 6][value]
                } else {
                    value as u8
                },
                label_key,
            );
        }
    }

    fn apply_clock_setting(&mut self, qsid: u16, value: u8, label_key: &'static str) {
        let (current, confirmed, value) = match qsid {
            CLOCK_STYLE_QSID => (
                self.display_settings.clock_style,
                self.display_settings.confirmed_clock_style,
                value.min(9),
            ),
            CLOCK_SIZE_QSID => (
                self.display_settings.clock_size,
                self.display_settings.confirmed_clock_size,
                value.min(3),
            ),
            CLOCK_ALIGNMENT_QSID => (
                self.display_settings.clock_alignment,
                self.display_settings.confirmed_clock_alignment,
                value.min(2),
            ),
            CLOCK_DELAY_QSID => (
                self.display_settings.clock_delay,
                self.display_settings.confirmed_clock_delay,
                value.min(7),
            ),
            DISPLAY_TIMEOUT_QSID => (
                self.display_settings.display_timeout,
                self.display_settings.confirmed_display_timeout,
                value.min(7),
            ),
            _ => return,
        };
        if value == current {
            return;
        }
        match qsid {
            CLOCK_STYLE_QSID => self.display_settings.clock_style = value,
            CLOCK_SIZE_QSID => self.display_settings.clock_size = value,
            CLOCK_ALIGNMENT_QSID => self.display_settings.clock_alignment = value,
            CLOCK_DELAY_QSID => self.display_settings.clock_delay = value,
            DISPLAY_TIMEOUT_QSID => self.display_settings.display_timeout = value,
            _ => return,
        }
        let displayed_value = self
            .pending_settings_write_value(qsid)
            .unwrap_or(confirmed as u16);
        if displayed_value != value as u16 {
            self.queue_display_setting_write(
                crate::i18n::tr_catalog(self.app_settings.language, label_key).to_owned(),
                qsid,
                confirmed as u16,
                value as u16,
            );
        }
    }

    fn draw_clock_colon_blink_row(
        &mut self,
        ui: &mut egui::Ui,
        list: &AdaptiveSettingsListViewport,
        scale: f32,
    ) {
        let mut enabled = self.display_settings.clock_colon_blink;
        let mut changed = false;
        display_settings_row(
            ui,
            list.row_content_width,
            list.row_height,
            crate::i18n::tr_catalog(
                self.app_settings.language,
                "display_settings.clock_colon_blink",
            ),
            true,
            46.0 * scale,
            |ui| {
                changed = crate::ui_style::settings_switch_sized_stable(
                    ui,
                    ("display_clock", "colon_blink"),
                    &mut enabled,
                    egui::vec2(46.0 * scale, 24.0 * scale),
                )
                .changed();
            },
        );
        if changed {
            self.apply_clock_colon_blink(enabled);
        }
    }

    fn apply_clock_colon_blink(&mut self, enabled: bool) {
        if enabled == self.display_settings.clock_colon_blink {
            return;
        }
        self.display_settings.clock_colon_blink = enabled;
        let displayed_value = self
            .pending_settings_write_value(CLOCK_COLON_BLINK_QSID)
            .unwrap_or(u16::from(self.display_settings.confirmed_clock_colon_blink));
        if displayed_value != u16::from(enabled) {
            self.queue_display_setting_write(
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.clock_colon_blink",
                )
                .to_owned(),
                CLOCK_COLON_BLINK_QSID,
                u16::from(self.display_settings.confirmed_clock_colon_blink),
                u16::from(enabled),
            );
        }
    }

    fn draw_display_brightness_slider(&mut self, ui: &mut egui::Ui, dark: bool, scale: f32) {
        let mut brightness = self.display_settings.brightness as f32;
        let value_color = if dark {
            Color32::from_gray(230)
        } else {
            Color32::from_gray(55)
        };
        ui.visuals_mut().selection.bg_fill = app_accent();
        ui.visuals_mut().widgets.active.bg_fill = app_accent();
        ui.visuals_mut().widgets.active.weak_bg_fill = app_accent();
        ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(1.0_f32, app_accent());
        let mut changed = false;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_sized(
                [42.0 * scale, 34.0 * scale],
                egui::Label::new(
                    RichText::new(format!("{}%", brightness as u8))
                        .size(12.0 * scale)
                        .color(value_color),
                )
                .halign(egui::Align::RIGHT),
            );
            ui.spacing_mut().slider_width = 146.0 * scale;
            changed = ui
                .add_sized(
                    [146.0 * scale, 34.0 * scale],
                    egui::Slider::new(&mut brightness, 0.0..=100.0)
                        .step_by(1.0)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed();
        });
        if changed {
            self.apply_display_brightness(brightness.round() as u8);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn draw_startup_image_control(
        &mut self,
        ui: &mut egui::Ui,
        _dark: bool,
        scale: f32,
        reset: bool,
    ) {
        let progress = if reset {
            None
        } else {
            self.startup_image_upload_progress()
        };
        let busy = self.startup_image_upload_progress().is_some()
            || self.vial_hid_task_blocks_user_action();
        let enabled = if reset {
            startup_image_reset_enabled(
                self.display_settings.startup_image_supported,
                self.display_settings.startup_image_present,
                busy,
            )
        } else {
            self.display_settings.startup_image_supported
                && !busy
                && self.pending_file_dialog.is_none()
        };
        let key = if reset {
            "display_settings.startup_image_reset"
        } else {
            "display_settings.upload"
        };
        let label = progress
            .map(|p| {
                format!(
                    "{} {}%",
                    crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "display_settings.upload_progress"
                    ),
                    (p.clamp(0.0, 1.0) * 100.0).floor() as u32
                )
            })
            .unwrap_or_else(|| crate::i18n::tr_catalog(self.app_settings.language, key).to_owned());
        if crate::ui_style::modern_progress_button(
            ui,
            &label,
            Vec2::new(120.0 * scale, 34.0 * scale),
            enabled,
            progress,
        )
        .clicked()
        {
            if reset {
                let _ = self.start_vial_hid_operation(
                    ui.ctx(),
                    super::vial_hid_task::VialHidOperation::StartupImageClear,
                );
            } else {
                self.spawn_file_dialog(
                    crate::app::file_dialog::FileDialogAction::StartupImage,
                    rfd::FileDialog::new()
                        .add_filter("Image", &["png", "jpg", "jpeg", "webp", "bmp"]),
                    false,
                );
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    fn draw_startup_image_control(
        &mut self,
        _ui: &mut egui::Ui,
        _dark: bool,
        _scale: f32,
        _reset: bool,
    ) {
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn draw_standby_background_control(&mut self, ui: &mut egui::Ui, _dark: bool, scale: f32) {
        let progress = self.standby_background_upload_progress();
        let busy = progress.is_some() || self.vial_hid_task_blocks_user_action();
        let lang = self.app_settings.language;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = progress
                .map(|p| {
                    format!(
                        "{} {}%",
                        crate::i18n::tr_catalog(lang, "display_settings.upload_progress"),
                        (p.clamp(0.0, 1.0) * 100.0).floor() as u32
                    )
                })
                .unwrap_or_else(|| {
                    crate::i18n::tr_catalog(lang, "display_settings.upload").to_owned()
                });
            if crate::ui_style::modern_progress_button(
                ui,
                &label,
                Vec2::new(110.0 * scale, 34.0 * scale),
                !busy && self.pending_file_dialog.is_none(),
                progress,
            )
            .clicked()
            {
                self.spawn_file_dialog(
                    crate::app::file_dialog::FileDialogAction::StandbyBackground,
                    rfd::FileDialog::new()
                        .add_filter("Image / GIF", &["png", "jpg", "jpeg", "webp", "bmp", "gif"]),
                    false,
                );
            }
            if crate::ui_style::modern_button(
                ui,
                crate::i18n::tr_catalog(
                    lang,
                    if progress.is_some() {
                        "display_settings.upload_cancel"
                    } else {
                        "display_settings.startup_image_reset"
                    },
                ),
                Vec2::new(110.0 * scale, 34.0 * scale),
                progress.is_some() || (!busy && self.display_settings.clock_background_kind != 0),
            )
            .clicked()
            {
                if progress.is_some() {
                    self.cancel_background_upload();
                } else {
                    let _ = self.start_vial_hid_operation(
                        ui.ctx(),
                        super::vial_hid_task::VialHidOperation::BackgroundClear,
                    );
                }
            }
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn draw_standby_background_control(&mut self, _ui: &mut egui::Ui, _dark: bool, _scale: f32) {}

    #[cfg(not(target_arch = "wasm32"))]
    fn draw_clock_background_speed_slider(&mut self, ui: &mut egui::Ui, dark: bool, scale: f32) {
        let mut speed = self.display_settings.clock_background_speed_percent as f32;
        let value_color = if dark {
            Color32::from_gray(230)
        } else {
            Color32::from_gray(55)
        };
        ui.visuals_mut().selection.bg_fill = app_accent();
        ui.visuals_mut().widgets.active.bg_fill = app_accent();
        ui.spacing_mut().slider_width = 146.0 * scale;
        let busy = self.vial_hid_task_blocks_user_action();
        let mut response = None;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_sized(
                [42.0 * scale, 34.0 * scale],
                egui::Label::new(
                    RichText::new(format!("{:.2}×", speed / 100.0))
                        .size(12.0 * scale)
                        .color(value_color),
                )
                .halign(egui::Align::RIGHT),
            );
            response = Some(
                ui.add_enabled(
                    !busy,
                    egui::Slider::new(&mut speed, 25.0..=400.0)
                        .step_by(5.0)
                        .show_value(false)
                        .trailing_fill(true),
                ),
            );
        });
        let response = response.expect("speed slider response");
        if response.changed() {
            self.display_settings.clock_background_speed_percent = speed.round() as u16;
            self.display_settings.clock_background_speed_write_due =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(150));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(150));
        }

        let Some(due) = self.display_settings.clock_background_speed_write_due else {
            return;
        };
        let now = std::time::Instant::now();
        if response.dragged() || now < due {
            ui.ctx().request_repaint_after(
                due.saturating_duration_since(now)
                    .max(std::time::Duration::from_millis(16)),
            );
            return;
        }
        let old_percent = self
            .display_settings
            .confirmed_clock_background_speed_percent;
        let percent = self.display_settings.clock_background_speed_percent;
        if percent == old_percent {
            self.display_settings.clock_background_speed_write_due = None;
            return;
        }
        match self.start_vial_hid_operation(
            ui.ctx(),
            super::vial_hid_task::VialHidOperation::BackgroundSpeed {
                old_percent,
                percent,
            },
        ) {
            super::vial_hid_task::VialHidTaskStart::Started => {
                self.display_settings.clock_background_speed_write_due = None;
            }
            super::vial_hid_task::VialHidTaskStart::Busy => {
                self.display_settings.clock_background_speed_write_due =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(100));
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100));
            }
            super::vial_hid_task::VialHidTaskStart::NoDevice => {
                self.display_settings.clock_background_speed_write_due = None;
                self.display_settings.clock_background_speed_percent = old_percent;
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.background_no_device",
                )
                .into();
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn draw_clock_background_speed_slider(&mut self, _ui: &mut egui::Ui, _dark: bool, _scale: f32) {
    }

    fn draw_clock_background_dim_slider(&mut self, ui: &mut egui::Ui, dark: bool, scale: f32) {
        let mut dim = self.display_settings.clock_background_dim as f32;
        let value_color = if dark {
            Color32::from_gray(230)
        } else {
            Color32::from_gray(55)
        };
        ui.visuals_mut().selection.bg_fill = app_accent();
        ui.visuals_mut().widgets.active.bg_fill = app_accent();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_sized(
                [42.0 * scale, 34.0 * scale],
                egui::Label::new(
                    RichText::new(format!("{}%", dim as u8))
                        .size(12.0 * scale)
                        .color(value_color),
                )
                .halign(egui::Align::RIGHT),
            );
            ui.spacing_mut().slider_width = 146.0 * scale;
            if ui
                .add_sized(
                    [146.0 * scale, 34.0 * scale],
                    egui::Slider::new(&mut dim, 0.0..=100.0)
                        .step_by(1.0)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed()
            {
                self.apply_clock_background_dim(dim.round() as u8);
            }
        });
    }

    fn apply_clock_background_dim(&mut self, dim: u8) {
        let dim = dim.min(100);
        if dim == self.display_settings.clock_background_dim {
            return;
        }
        self.display_settings.clock_background_dim = dim;
        let displayed = self
            .pending_settings_write_value(CLOCK_BACKGROUND_DIM_QSID)
            .unwrap_or(self.display_settings.confirmed_clock_background_dim as u16);
        if displayed != dim as u16 {
            self.queue_display_setting_write(
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.clock_background_dim",
                )
                .to_owned(),
                CLOCK_BACKGROUND_DIM_QSID,
                self.display_settings.confirmed_clock_background_dim as u16,
                dim as u16,
            );
        }
    }

    fn apply_display_brightness(&mut self, brightness: u8) {
        let brightness = brightness.min(100);
        if brightness == self.display_settings.brightness {
            return;
        }
        self.display_settings.brightness = brightness;
        let displayed_value = self
            .pending_settings_write_value(DISPLAY_BRIGHTNESS_QSID)
            .unwrap_or(self.display_settings.confirmed_brightness as u16);
        if displayed_value != brightness as u16 {
            self.queue_display_setting_write(
                crate::i18n::tr_catalog(self.app_settings.language, "display_settings.brightness")
                    .to_owned(),
                DISPLAY_BRIGHTNESS_QSID,
                self.display_settings.confirmed_brightness as u16,
                brightness as u16,
            );
        }
    }

    fn draw_display_button_style_selector(&mut self, ui: &mut egui::Ui, dark: bool, scale: f32) {
        let lang = self.app_settings.language;
        let variants = [
            (0, "display_settings.style_rounded"),
            (6, "display_settings.style_squircle"),
            (4, "display_settings.style_chamfered"),
            (2, "display_settings.style_oval"),
            (5, "display_settings.style_hexagon"),
        ]
        .map(|(style, key)| (style, crate::i18n::tr_catalog(lang, key).to_owned()));
        if !DISPLAY_BUTTON_STYLE_IDS.contains(&self.display_settings.button_style) {
            self.apply_display_button_style(DISPLAY_BUTTON_STYLE_IDS[0]);
        }
        let selected_style = self
            .display_settings
            .button_style
            .min(DISPLAY_BUTTON_STYLE_MAX_ID);
        let selected = variants
            .iter()
            .position(|(style, _)| *style == selected_style)
            .unwrap_or(0);
        let dropdown_id = ui.make_persistent_id("display_button_style_dropdown");
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let width = 196.0 * scale;
        let option_font = FontId::proportional(metrics.value(12.0));
        let longest_label_width = variants
            .iter()
            .map(|(_, label)| {
                ui.painter()
                    .layout_no_wrap(
                        label.clone(),
                        option_font.clone(),
                        ui.visuals().text_color(),
                    )
                    .size()
                    .x
            })
            .fold(0.0_f32, f32::max);
        let popup_width = (metrics.value(42.0 + 16.0) + longest_label_width)
            .max(width)
            .min((ui.ctx().content_rect().width() - metrics.value(24.0)).max(width));
        let selected_label = variants
            .get(selected)
            .map(|(_, label)| label.as_str())
            .unwrap_or("");
        let padded_label = format!("        {selected_label}");
        let dropdown_response = crate::ui_style::modern_dropdown_button_sized(
            ui,
            dropdown_id,
            &padded_label,
            ui.visuals().text_color(),
            width,
            metrics.settings_control_height(),
            metrics.settings_control_font_size(),
        );
        let selected_icon_rect = egui::Rect::from_center_size(
            egui::pos2(
                dropdown_response.rect.left() + metrics.value(22.0),
                dropdown_response.rect.center().y,
            ),
            Vec2::new(metrics.value(24.0), metrics.value(16.0)),
        );
        paint_display_button_style_icon(
            ui,
            selected_style,
            selected_icon_rect,
            ui.visuals().text_color(),
        );

        let mut picked = None;
        crate::ui_style::popup_below_widget_with_width(
            ui,
            dropdown_id,
            &dropdown_response,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            popup_width,
            |ui| {
                ui.set_min_width(popup_width);
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                egui::ScrollArea::vertical()
                    .id_salt(("display_button_style_scroll", dropdown_id))
                    .max_height(metrics.value(198.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (index, (style, label)) in variants.iter().enumerate() {
                            let is_selected = index == selected;
                            let (option_rect, option_response) = ui.allocate_exact_size(
                                Vec2::new(popup_width, metrics.value(30.0)),
                                Sense::click(),
                            );
                            if option_response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            let option_fill = if is_selected {
                                if dark {
                                    Color32::from_rgb(58, 58, 61)
                                } else {
                                    Color32::from_rgb(236, 236, 238)
                                }
                            } else if option_response.hovered() {
                                crate::ui_style::hover_fill(dark)
                            } else {
                                Color32::TRANSPARENT
                            };
                            ui.painter().rect_filled(option_rect, 7.0, option_fill);
                            let text_color = if is_selected {
                                ui.visuals().text_color()
                            } else {
                                app_muted_text(dark)
                            };
                            let icon_rect = egui::Rect::from_center_size(
                                egui::pos2(
                                    option_rect.left() + metrics.value(22.0),
                                    option_rect.center().y,
                                ),
                                Vec2::new(metrics.value(24.0), metrics.value(16.0)),
                            );
                            paint_display_button_style_icon(ui, *style, icon_rect, text_color);
                            ui.painter().text(
                                egui::pos2(
                                    option_rect.left() + metrics.value(42.0),
                                    option_rect.center().y,
                                ),
                                egui::Align2::LEFT_CENTER,
                                label,
                                option_font.clone(),
                                text_color,
                            );
                            if option_response.clicked() {
                                picked = Some(*style);
                                egui::Popup::close_all(ui.ctx());
                            }
                        }
                    });
            },
        );
        if let Some(style) = picked {
            self.apply_display_button_style(style);
        }
    }

    fn apply_display_button_style(&mut self, style: u8) {
        let style = style.min(DISPLAY_BUTTON_STYLE_MAX_ID);
        if style == self.display_settings.button_style {
            return;
        }

        self.display_settings.button_style = style;
        let displayed_value = self
            .pending_settings_write_value(DISPLAY_BUTTON_STYLE_QSID)
            .unwrap_or(self.display_settings.confirmed_button_style as u16);
        if displayed_value != style as u16 {
            self.queue_display_setting_write(
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.button_style",
                )
                .to_owned(),
                DISPLAY_BUTTON_STYLE_QSID,
                self.display_settings.confirmed_button_style as u16,
                style as u16,
            );
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn backspace_delete_catalog_uses_backspace_name_without_changing_pixels_or_order() {
        let start=38+crate::action_icons::ACTION_ICON_COUNT;
        assert_eq!(BUILTIN_PICTOGRAM_KEYS[start+11], "display_settings.pictogram_firmware_backspace");
        assert_eq!(crate::firmware_builtin_icons::FIRMWARE_ICON_KEYS[11], BUILTIN_PICTOGRAM_KEYS[start+11]);
        assert_eq!(&builtin_pictogram_bitmap(start+11), include_bytes!("../../tools/artwork/backspace-delete/backspace-approved.bin"));
        assert_eq!(crate::firmware_builtin_icons::FIRMWARE_ICON_COUNT,22);
        assert_eq!(BUILTIN_PICTOGRAM_KEYS[start+20],"display_settings.pictogram_firmware_layer_prev_icon");
        assert_eq!(BUILTIN_PICTOGRAM_KEYS[start+21],"display_settings.pictogram_firmware_layer_next_icon");
        assert_eq!(crate::i18n::tr_catalog(crate::i18n::Language::Russian, BUILTIN_PICTOGRAM_KEYS[start+11]),"Клавиша Backspace");
    }

    #[test]
    fn backspace_delete_old_delete_stock_alias_stays_hidden_but_user_art_stays_user() {
        for lang in [crate::i18n::Language::English,crate::i18n::Language::Russian] {
            let mut icon=SavedPictogram {
                origin:PictogramOrigin::Legacy,
                name:crate::i18n::tr_catalog(lang,"display_settings.pictogram_firmware_delete").into(),
                color:[84,189,191],
                bitmap:include_bytes!("../../tools/artwork/backspace-delete/backspace-approved.bin").to_vec(),
            };
            assert!(crate::app::pictogram_origin::is_known_migrated_stock_pictogram(&icon));
            icon.origin=PictogramOrigin::User;
            assert!(!crate::app::pictogram_origin::is_known_migrated_stock_pictogram(&icon));
            icon.origin=PictogramOrigin::Legacy;icon.bitmap[50]^=0x01;
            assert!(!crate::app::pictogram_origin::is_known_migrated_stock_pictogram(&icon));
        }
    }

    /// Optional real Glow/X11 render of the settings page, with fixture data only.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires an X11 display and ENTROPY_RENDER_OUTPUT"]
    fn sm62_render_settings_screenshot() {
        use winit::platform::x11::EventLoopBuilderExtX11;
        struct Fixture { app: EntropyApp, frames: usize, path: std::path::PathBuf }
        impl eframe::App for Fixture {
            fn persist_egui_memory(&self) -> bool { false }
            fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
                let ctx = ui.ctx().clone();
                // EntropyApp::new may restore its saved native window size.
                // Explicitly resize the fixture after construction, not just its builder.
                if self.frames == 0 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        std::env::var("ENTROPY_RENDER_WIDTH").ok().and_then(|v|v.parse::<f32>().ok()).unwrap_or(1000.0),
                        std::env::var("ENTROPY_RENDER_HEIGHT").ok().and_then(|v|v.parse::<f32>().ok()).unwrap_or(820.0),
                    )));
                    ctx.set_visuals(egui::Visuals::light());
                }
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.label("Disconnected render fixture — no HID writes / not hardware verification");
                    self.app.draw_sm62_display_settings_page(ui,ui.available_rect_before_wrap());
                    if self.frames==2 {
                        if let Some((_,max))=ctx.data(|data|data.get_temp::<(egui::Id,f32)>(egui::Id::unique("sm62_scroll_fixture"))) {
                            scroll_sm62_display_to(&ctx,if std::env::var("ENTROPY_RENDER_BOTTOM").as_deref()==Ok("1") {max} else {0.0});
                        }
                    }
                });
                self.frames += 1;
                if self.frames == 5 {
                    let (path, close) = (self.path.clone(), ctx.clone());
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |image| {
                        let rgba: Vec<u8> = image.pixels.iter().flat_map(|pixel| pixel.to_array()).collect();
                        image::save_buffer(&path, &rgba, image.size[0] as u32,
                            image.size[1] as u32, image::ColorType::Rgba8).unwrap();
                        close.send_viewport_cmd(egui::ViewportCommand::Close);
                    })));
                }
                assert!(self.frames < 120, "screenshot did not arrive");
                ctx.request_repaint();
            }
        }
        let path = std::path::PathBuf::from(std::env::var("ENTROPY_RENDER_OUTPUT").unwrap());
        let output = path.clone();
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([
                std::env::var("ENTROPY_RENDER_WIDTH").ok().and_then(|v|v.parse::<f32>().ok()).unwrap_or(1000.0),
                std::env::var("ENTROPY_RENDER_HEIGHT").ok().and_then(|v|v.parse::<f32>().ok()).unwrap_or(820.0),
            ]),
            renderer: eframe::Renderer::Glow,
            persist_window: false,
            event_loop_builder: Some(Box::new(|builder| { builder.with_any_thread(true); })),
            ..Default::default()
        };
        eframe::run_native("SM62 settings render fixture (not hardware)", options, Box::new(move |cc| {
            let mut fonts = egui::FontDefinitions::default();
            for (name, data) in [("roboto", include_bytes!("../../assets/Roboto-Regular.ttf").as_slice()),
                ("montserrat", include_bytes!("../../assets/Montserrat-Medium.ttf").as_slice())] {
                fonts.font_data.insert(name.into(), egui::FontData::from_static(data).into());
            }
            fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "roboto".into());
            fonts.families.insert(egui::FontFamily::Name("display_preview".into()), vec!["montserrat".into()]);
            cc.egui_ctx.set_fonts(fonts);
            let mut app = EntropyApp::new(cc);
            app.app_settings.language = if std::env::var("ENTROPY_RENDER_LANGUAGE").as_deref() == Ok("ru") {
                crate::i18n::Language::Russian
            } else { crate::i18n::Language::English };
            app.display_settings.sm62_values = if std::env::var("ENTROPY_RENDER_SCENE").as_deref() == Ok("keymap") {
                [3, 0, 47, 1, 1, 3, 1, 83, 1, 1]
            } else if std::env::var("ENTROPY_RENDER_SCENE").as_deref() == Ok("unsynced") {
                [0,0,20,1,1,0,0,20,1,1]
            } else if std::env::var("ENTROPY_RENDER_SCENE").as_deref() == Ok("clock") {
                [0, 0, 20, 1, 1, 1, 0, 20, 1, 1]
            } else { [1, 0, 47, 1, 1, 2, 1, 83, 1, 1] };
            app.display_settings.sm62_language_values = [1, 0];
            app.display_settings.sm62_language_supported = true;
            app.display_settings.sm62_supported = true;
            app.display_settings.sm62_stars_supported = true;
            app.display_settings.sm62_screens_off_supported = true;
            app.display_settings.sm62_stars_values = if std::env::var("ENTROPY_RENDER_EFFECT").as_deref()==Ok("plain") {[0,0]} else {[1,1]};
            app.display_settings.sm62_screens_off_values = if std::env::var("ENTROPY_RENDER_EFFECT").as_deref()==Ok("off") {[1,1]} else {[0,0]};

            app.display_settings.sm62_date_supported = true;
            app.display_settings.sm62_hold_supported = true;
            Ok(Box::new(Fixture { app, frames: 0, path }))
        })).unwrap();
        assert!(output.is_file());
    }

    #[test]
    fn v7_sm62_async_render_does_not_requeue_or_reverse_first_star_edit() {
        for initial in [0u8,1] {
            let ctx=egui::Context::default();
            let cc=eframe::CreationContext::_new_kittest(ctx.clone());
            let mut app=EntropyApp::new(&cc);
            let mut fonts=egui::FontDefinitions::default();
            fonts.families.insert(egui::FontFamily::Name("display_preview".into()),fonts.families[&egui::FontFamily::Proportional].clone()); ctx.set_fonts(fonts);
            let (hid,recorder)=crate::hid::HidDevice::test_device(); app.hid_device=Some(hid);
            app.display_settings.sm62_supported=true; app.display_settings.sm62_stars_supported=true;
            app.display_settings.sm62_stars_values=[initial;2];
            app.display_settings.confirmed_sm62_stars_values=[initial;2];
            let render=|app:&mut EntropyApp,events:Vec<egui::Event>| crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput {events,screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1000.0,1800.0))),..Default::default()},|ctx| {
                ctx.root_ui(|ui| egui::CentralPanel::default().show(ui,|ui| {app.draw_sm62_display_settings_page(ui,ui.max_rect());}));
            }));
            render(&mut app,vec![]);
            let output=render(&mut app,vec![]);
            let pos=output.shapes.iter().filter_map(|s|match &s.shape {egui::Shape::Rect(r) if r.rect.width()==46.0 && r.rect.height()==24.0=>Some(r.rect.center()),_=>None}).nth(5).expect("star switch after individual/layout/date controls");
            render(&mut app,vec![egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()},
                egui::Event::PointerButton {pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
            assert_eq!(app.display_settings.sm62_stars_values,[1-initial;2],"first actual pointer click");
            for _ in 0..80 {
                app.poll_settings_write(&ctx);
                ctx.run_pass(egui::RawInput {screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1000.0,1800.0))),..Default::default()},|ctx| {
                    ctx.root_ui(|ui| egui::CentralPanel::default().show(ui,|ui| {app.draw_sm62_display_settings_page(ui,ui.max_rect());}));
                }).drop_without_applying_deltas();
                assert_eq!(app.display_settings.sm62_stars_values,[1-initial;2]);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert_eq!(app.display_settings.confirmed_sm62_stars_values,[1-initial;2]);
            assert!(!app.qmk_settings_write_busy(),"paint must not continuously supersede the in-flight write");
            let requests=recorder.requests();
            let sets:Vec<_>=requests.iter().filter(|r|r[1]==0x0B && (246..=247).contains(&u16::from_le_bytes([r[2],r[3]]))).collect();
            assert_eq!(sets.len(),2,"one explicit edit sends exactly one SET per half");
        }
    }

    #[test]
    fn sm62_settings_show_both_halves_without_scene_categories() {
        for language in [crate::i18n::Language::English, crate::i18n::Language::Russian] {
            let ctx = egui::Context::default();
            let cc = eframe::CreationContext::_new_kittest(ctx.clone());
            let mut app = EntropyApp::new(&cc);
            app.app_settings.language = language;
            app.display_settings.sm62_date_supported = true;
            app.display_settings.sm62_hold_supported = true;
            let mut fonts = egui::FontDefinitions::default();
            fonts.families.insert(egui::FontFamily::Name("display_preview".into()),
                fonts.families[&egui::FontFamily::Proportional].clone());
            ctx.set_fonts(fonts);
            let output = crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1800.0))),
                ..Default::default()
            }, |ctx| {
                ctx.root_ui(|ui| egui::CentralPanel::default().show(ui, |ui| {
                    app.draw_sm62_display_settings_page(ui, ui.max_rect());
                }));
            }));
            let labels: Vec<_> = output.shapes.iter().filter_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape { Some(text.galley.job.text.as_str()) }
                else { None }
            }).collect();
            for category in ["After timeout", "Key press", "После таймаута", "Нажатие"] {
                assert!(!labels.contains(&category), "unnecessary category {category}: {labels:?}");
            }
            let russian = language == crate::i18n::Language::Russian;
            assert!(labels.contains(&if russian { "Режимы экранов настраиваются отдельно. Настройки сохраняются в клавиатуре." } else { "Display modes are configured independently. Settings are saved on the keyboard." }));
            assert!(!labels.iter().any(|l| l.starts_with("Корабль как режим") || l.starts_with("Spaceship mode is continuous")));
            let layout=labels.iter().position(|l| *l==if russian {"Показывать текущую раскладку RU/EN"} else {"Show current layout RU/EN"}).unwrap();
            let date=labels.iter().position(|l| *l==if russian {"Дата на часах"} else {"Date on clock"}).unwrap();
            assert!(layout<date);
            for heading in if russian { ["Левый экран", "Правый экран"] } else { ["Left display", "Right display"] } {
                assert!(labels.contains(&heading), "missing {heading}: {labels:?}");
            }
            for control in if russian {
                ["Режим по умолчанию", "Анимация ожидания через",
                 "Показывать раскладку при нажатии", "Кириллические символы", "Показывать раскладку в течение", "Дата на часах", "Показывать текущую раскладку RU/EN", "Анимация звёзд", "Выключить экраны"]
            } else {
                ["Default mode", "Enable standby mode after",
                 "Show layout on key press", "Cyrillic symbols", "Show layout for", "Date on clock", "Show current layout RU/EN", "Star animation", "Turn off displays"]
            } {
                let shared = if russian { ["Кириллические символы", "Показывать раскладку в течение", "Дата на часах", "Показывать текущую раскладку RU/EN", "Анимация звёзд", "Выключить экраны"] }
                    else { ["Cyrillic symbols", "Show layout for", "Date on clock", "Show current layout RU/EN", "Star animation", "Turn off displays"] };
                assert_eq!(labels.iter().filter(|label| **label == control).count(), if shared.contains(&control) { 1 } else { 2 },
                    "shared settings once, individual controls twice: {control}: {labels:?}");
            }
        }
    }

    #[test]
    fn sm62_idle_dropdown_transitions_preserve_timeout_and_other_half() {
        for side in 0..2 {
        let base = side * 5;
        let ctx = egui::Context::default();
        let mut values = [0, 1, 83, 1, 1, 0, 1, 83, 1, 1];
        values[base + 1] = 0;
        values[base + 2] = 47;
        let initial = values;
        let mut changed = Vec::new();
        let mut run = |events: Vec<egui::Event>| {
            crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 900.0))),
                events, ..Default::default()
            }, |ctx| { ctx.root_ui(|ui| egui::CentralPanel::default().show(ui, |ui| {
                sm62_display_controls(ui, side, &mut values, &mut changed, false);
            })); }))
        };
        let click = |pos| vec![egui::Event::PointerMoved(pos),
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }];
        let position = |output: &egui::FullOutput, label: &str| {
            output.shapes.iter().find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == label => Some(t.pos + t.galley.size() / 2.0), _ => None
            }).unwrap_or_else(|| panic!("missing {label}"))
        };
        let output = run(vec![]);
        run(click(position(&output, "No animation")));
        // Let the animated popup become fully laid out before selecting.
        run(vec![]);
        let output = run(vec![]);
        run(click(position(&output, "10 seconds")));
        let output = run(vec![]);
        run(click(position(&output, "10 seconds")));
        run(vec![]);
        let output = run(vec![]);
        run(click(position(&output, "No animation")));
        drop(run);
        assert_eq!(values[base + 1], 0);
        assert_eq!(values[base + 2], 10);
        let other = (1 - side) * 5;
        assert_eq!(&values[other..other + 5], &initial[other..other + 5]);
        assert_eq!(changed, [base + 2, base + 1, base + 1]);
        }
    }

    #[test]
    fn sm62_idle_dropdown_preserves_custom_zero_and_ship_settings_on_render() {
        for (mode, enabled, timeout, expected) in [(0, 1, 47, "47 s"), (0, 1, 0, "0 s"),
            (2, 0, 83, "No animation"), (2, 1, 83, "83 s")] {
            let ctx = egui::Context::default();
            let mut values = [mode, enabled, timeout, 1, 1, 0, 1, 120, 1, 1];
            let saved = values;
            let mut changed = Vec::new();
            let mut id = egui::Id::NULL;
            let output = crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput::default(), |ctx| {
                ctx.root_ui(|ui| egui::CentralPanel::default().show(ui, |ui| {
                    id = sm62_display_controls(ui, 0, &mut values, &mut changed, false);
                }));
            }));
            assert_eq!(values, saved);
            assert!(changed.is_empty());
            assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text == expected)));
            if mode != 2 {
                egui::Popup::open_id(&ctx, id);
                let draw = |ctx: &egui::Context| {
                    ctx.root_ui(|ui| egui::CentralPanel::default().show(ui, |ui| {
                        sm62_display_controls(ui, 0, &mut values, &mut changed, false);
                    }));
                };
                ctx.run_pass(egui::RawInput::default(), draw).drop_without_applying_deltas();
                let output = crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput::default(), |ctx| {
                    ctx.root_ui(|ui| egui::CentralPanel::default().show(ui, |ui| {
                        sm62_display_controls(ui, 0, &mut values, &mut changed, false);
                    }));
                }));
                let labels: Vec<_> = output.shapes.iter().filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) => Some(t.galley.job.text.as_str()), _ => None }).collect();
                assert!(labels.contains(&"No animation"));
                assert!(!labels.contains(&"Never"));
                for key in &STANDBY_DELAY_KEYS[1..] { assert!(labels.contains(&crate::i18n::tr_catalog(crate::i18n::Language::English, key)), "{labels:?}"); }
                assert_eq!(crate::i18n::tr_catalog(crate::i18n::Language::English, STANDBY_DELAY_KEYS[0]), "Never");
                assert!(labels.contains(&expected));
            }
        }
    }

    #[test]
    fn sm62_common_language_switch_changes_both_halves() {
        for side in 0..2 {
            let ctx = egui::Context::default();
            let mut values = [0,0,20,1,1,0,0,20,1,1];
            let mut language = [1,1];
            let mut draw = |events| crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput { events, ..Default::default() }, |ctx| {
                ctx.root_ui(|ui| egui::CentralPanel::default().show(ui, |ui| {
                    sm62_shared_controls(ui,&mut values,&mut Vec::new(),false,None,None,Some(&mut language),None,None);
                }));
            }));
            let output = draw(vec![]);
            let pos = output.shapes.iter().filter_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.rect.width() == 46.0 && r.rect.height() == 24.0 => Some(r.rect.center()), _ => None
            }).nth(1).expect("language switch paint");
            draw(vec![egui::Event::PointerMoved(pos),
                egui::Event::PointerButton { pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default() },
                egui::Event::PointerButton { pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default() }]);
            drop(draw);
            assert_eq!(language[side],0);
            assert_eq!(language[1-side],0);
        }
    }

    #[test]
    fn sm62_shared_controls_preserve_mixed_values_until_explicit_edit() {
        let ctx = egui::Context::default();
        let mut values = [0,0,20,1,1,2,1,0,1,0];
        let mut date = [1,0];
        let mut hold = [112,333];
        let mut language = [0,1];
        let before = (values,date,hold,language);
        let mut changed = Vec::new();
        let output = crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput::default(), |ctx| {
            ctx.root_ui(|ui| egui::CentralPanel::default().show(ui, |ui| {
                sm62_shared_controls(ui,&mut values,&mut changed,false,Some(&mut date),Some(&mut hold),Some(&mut language),None,None);
            }));
        }));
        assert_eq!((values,date,hold,language),before);
        assert!(changed.is_empty());
        assert!(output.shapes.iter().any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.job.text.contains("Mixed"))));
    }

    #[test]
    fn sm62_shared_switches_apply_both_without_changing_other_settings() {
        for index in 0..3 {
            let ctx = egui::Context::default();
            let mut values = [0,0,20,1,1,2,1,0,1,1];
            let mut date = [1,1]; let mut hold = [112,333]; let mut language = [1,1];
            let before = values; let mut changed = Vec::new();
            let mut draw = |events| crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput { events,..Default::default() },|ctx| {
                ctx.root_ui(|ui| egui::CentralPanel::default().show(ui,|ui| {
                    sm62_shared_controls(ui,&mut values,&mut changed,false,Some(&mut date),Some(&mut hold),Some(&mut language),None,None);
                }));
            }));
            let output = draw(vec![]);
            let switches: Vec<_> = output.shapes.iter().filter_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.rect.width()==46.0 && r.rect.height()==24.0 => Some(r.rect.center()), _=>None }).collect();
            assert_eq!(switches.len(),5);
            let pos = switches[index];
            draw(vec![egui::Event::PointerMoved(pos),
                egui::Event::PointerButton { pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default() },
                egui::Event::PointerButton { pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default() }]);
            drop(draw);
            if index==0 { assert_eq!([values[4],values[9]],[0,0]); assert_eq!(changed,[4,9]); }
            else { assert_eq!(values,before); assert!(changed.is_empty()); }
            assert_eq!(date,if index==2 {[0,0]} else {[1,1]});
            assert_eq!(language,if index==1 {[0,0]} else {[1,1]});
            assert_eq!(hold,[112,333]);
            assert_eq!([values[0],values[1],values[2],values[3],values[5],values[6],values[7],values[8]],
                [before[0],before[1],before[2],before[3],before[5],before[6],before[7],before[8]]);
        }
    }

    #[test]
    fn sm62_shared_hold_drag_applies_to_both_halves() {
        let ctx = egui::Context::default();
        let mut values=[0,0,20,1,1,0,0,20,1,1]; let mut hold=[112,333];
        let mut draw=|events| crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput { events,..Default::default() },|ctx| {
            ctx.root_ui(|ui| egui::CentralPanel::default().show(ui,|ui| { sm62_shared_controls(ui,&mut values,&mut Vec::new(),false,None,Some(&mut hold),None,None,None); }));
        }));
        let output=draw(vec![]);
        let pos=output.shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.job.text=="Mixed" => Some(t.pos+t.galley.size()/2.0), _=>None }).unwrap();
        draw(vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton {pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        let moved=pos+egui::vec2(20.0,0.0);
        draw(vec![egui::Event::PointerMoved(moved)]);
        draw(vec![egui::Event::PointerButton {pos:moved,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
        drop(draw);
        assert_eq!(hold[0],hold[1]);
        assert_ne!(hold,[112,333]);
    }

    #[test]
    fn sm62_shared_hold_can_commit_the_existing_left_value_to_both() {
        let ctx=egui::Context::default();
        let mut values=[0,0,20,1,1,0,0,20,1,1]; let mut hold=[112,333];
        let mut draw=|events| crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput {events,..Default::default()},|ctx| {
            ctx.root_ui(|ui| egui::CentralPanel::default().show(ui,|ui| {sm62_shared_controls(ui,&mut values,&mut Vec::new(),false,None,Some(&mut hold),None,None,None);}));
        }));
        let output=draw(vec![]);
        let pos=output.shapes.iter().find_map(|s| match &s.shape {egui::Shape::Text(t) if t.galley.job.text=="Mixed"=>Some(t.pos+t.galley.size()/2.0),_=>None}).unwrap();
        for _ in 0..2 {draw(vec![egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()},
            egui::Event::PointerButton {pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);}
        draw(vec![]);
        let mods=egui::Modifiers {ctrl:true,command:true,..Default::default()};
        draw(vec![egui::Event::Key {key:egui::Key::A,physical_key:None,pressed:true,repeat:false,modifiers:mods},egui::Event::Text("112".into())]);
        draw(vec![egui::Event::Key {key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
        drop(draw);
        assert_eq!(hold,[112,112]);
    }

    #[test]
    fn sm62_scrollbar_uses_common_style_at_right_edge() {
        let ctx=egui::Context::default();
        let cc=eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app=EntropyApp::new(&cc);
        let mut fonts=egui::FontDefinitions::default();
        fonts.families.insert(egui::FontFamily::Name("display_preview".into()),fonts.families[&egui::FontFamily::Proportional].clone());
        ctx.set_fonts(fonts);
        let mut output=None;
        for _ in 0..3 {output=Some(crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput {
            screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1000.0,650.0))),..Default::default()
        },|ctx| {ctx.root_ui(|ui| egui::CentralPanel::default().show(ui,|ui| {app.draw_sm62_display_settings_page(ui,ui.max_rect());}));})));}
        let output=output.unwrap();
        let thumbs:Vec<_>=output.shapes.iter().filter_map(|s|match &s.shape {
            egui::Shape::Rect(r) if r.fill==Color32::from_rgb(62,62,66) && (r.rect.width()-6.0).abs()<0.1=>Some(r.rect),_=>None}).collect();
        assert_eq!(thumbs.len(),1,"common floating scrollbar handle must be painted once");
        assert!((thumbs[0].right()-959.0).abs()<1.0,"scrollbar belongs beside the centered 918px viewport, not at the window edge: {:?}",thumbs[0]);
    }

    #[test]
    fn sm62_effect_preview_obeys_shared_stars_and_screen_off_settings() {
        for off in [false,true] {
            let ctx=egui::Context::default(); let cc=eframe::CreationContext::_new_kittest(ctx.clone());
            let mut app=EntropyApp::new(&cc);
            let mut fonts=egui::FontDefinitions::default();
            fonts.families.insert(egui::FontFamily::Name("display_preview".into()),fonts.families[&egui::FontFamily::Proportional].clone());ctx.set_fonts(fonts);
            app.display_settings.sm62_stars_values=[0,0];
            app.display_settings.sm62_screens_off_values=if off {[1,1]} else {[0,0]};
            let output=crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput {screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1000.0,1800.0))),..Default::default()},|ctx| {
                ctx.root_ui(|ui| egui::CentralPanel::default().show(ui,|ui| {app.draw_sm62_display_settings_page(ui,ui.max_rect());}));
            }));
            assert!(!output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Circle(c) if c.radius<2.0 && c.fill==Color32::from_rgb(131,127,118).gamma_multiply(0.6))),"stars disabled means plain background");
            let shared_y = output.shapes.iter().find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == "Shared display settings" || t.galley.job.text == "Общие настройки экранов" => Some(t.pos.y),
                _ => None,
            }).expect("shared settings heading");
            // Only the two previews are relevant: shared control meshes are
            // not display image content.
            let meshes=output.shapes.iter().filter(|s|matches!(&s.shape,egui::Shape::Mesh(m) if m.vertices.iter().all(|v| v.pos.y < shared_y))).count();
            if off {assert_eq!(meshes,0,"screens off hides all content, including ship");}
            else {assert!(meshes>0,"star setting must not remove clock content or ship texture");}
        }
    }

    #[test]
    fn sm62_new_shared_effect_switches_apply_to_both_only() {
        for index in [3,4] {
            let ctx=egui::Context::default();
            let mut values=[0,0,20,1,1,2,1,0,1,1];let before=values;
            let mut stars=[1,1];let mut off=[0,0];let mut changed=Vec::new();
            let mut draw=|events|crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput {events,..Default::default()},|ctx| {
                ctx.root_ui(|ui| egui::CentralPanel::default().show(ui,|ui| {sm62_shared_controls(ui,&mut values,&mut changed,false,None,None,None,Some(&mut stars),Some(&mut off));}));
            }));
            let output=draw(vec![]);let switches:Vec<_>=output.shapes.iter().filter_map(|s|match &s.shape {egui::Shape::Rect(r) if r.rect.width()==46.0&&r.rect.height()==24.0=>Some(r.rect.center()),_=>None}).collect();
            let pos=switches[index];draw(vec![egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()},
                egui::Event::PointerButton {pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
            drop(draw);assert_eq!(values,before);assert!(changed.is_empty());
            assert_eq!(stars,if index==3 {[0,0]} else {[1,1]});assert_eq!(off,if index==4 {[1,1]} else {[0,0]});
        }
    }

    #[test]
    fn sm62_right_scroll_track_reaches_last_shared_setting() {
        let ctx=egui::Context::default();let cc=eframe::CreationContext::_new_kittest(ctx.clone());let mut app=EntropyApp::new(&cc);app.app_settings.language=crate::i18n::Language::English;
        let mut fonts=egui::FontDefinitions::default();fonts.families.insert(egui::FontFamily::Name("display_preview".into()),fonts.families[&egui::FontFamily::Proportional].clone());ctx.set_fonts(fonts);
        let mut draw=|events|crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput {events,screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1000.0,650.0))),..Default::default()},|ctx| {
            ctx.root_ui(|ui| egui::CentralPanel::default().show(ui,|ui| {app.draw_sm62_display_settings_page(ui,ui.max_rect());}));
        }));
        for _ in 0..3 {draw(vec![]);}
        let pos=egui::pos2(956.0,625.0);draw(vec![egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()},
            egui::Event::PointerButton {pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
        let output=draw(vec![]);
        let (id,max)=ctx.data(|data|data.get_temp::<(egui::Id,f32)>(egui::Id::unique("sm62_scroll_fixture"))).unwrap();assert!(max>0.0);assert!(egui::scroll_area::State::load(&ctx,id).unwrap().unclamped_offset().y>0.0);
        assert!(output.shapes.iter().any(|s|match &s.shape {egui::Shape::Text(t) if t.galley.job.text=="Turn off displays"=>s.clip_rect.contains(t.pos+t.galley.size()/2.0),_=>false}),"last setting must be visible after scrolling");
    }

    #[test]
    fn sm62_settings_column_is_centered_bounded_and_headings_centered() {
        for language in [crate::i18n::Language::English,crate::i18n::Language::Russian] {
            for width in [540.0,1000.0,1600.0,2560.0] {
                let ctx=egui::Context::default();let cc=eframe::CreationContext::_new_kittest(ctx.clone());let mut app=EntropyApp::new(&cc);app.app_settings.language=language;
                let mut fonts=egui::FontDefinitions::default();fonts.families.insert(egui::FontFamily::Name("display_preview".into()),fonts.families[&egui::FontFamily::Proportional].clone());ctx.set_fonts(fonts);
                let mut full=egui::Rect::NOTHING;
                let mut draw=||crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput {screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(width,650.0))),..Default::default()},|ctx|{
                    ctx.root_ui(|ui| egui::CentralPanel::default().show(ui,|ui|{full=ui.max_rect();app.draw_sm62_display_settings_page(ui,full);}));
                }));
                draw();draw();let top=draw();
                let russian=language==crate::i18n::Language::Russian;
                let left_label=if russian {"Левый экран"} else {"Left display"};
                let center=top.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Text(t) if t.galley.job.text==left_label=>Some(t.pos.x+t.galley.rect.center().x),_=>None}).unwrap();
                let expected_center=width/2.0-9.0;
                assert!((center-expected_center).abs()<2.0,"half heading must center within the bounded column: width={width}, got={center}");
                if width>=1000.0 {
                    let frame=top.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Rect(r) if r.fill==Color32::from_rgb(7,7,5)&&r.stroke.color==Color32::from_gray(75)=>Some(r.rect),_=>None}).unwrap();
                    assert!((frame.left()-(width-918.0)/2.0).abs()<2.0,"preview must align with centered block left: {frame:?}");
                }
                let (_,max)=ctx.data(|d|d.get_temp::<(egui::Id,f32)>(egui::Id::unique("sm62_scroll_fixture"))).unwrap();
                scroll_sm62_display_to(&ctx,max);
                draw();let bottom=draw();drop(draw);
                let shared=if russian {"Общие настройки экранов"} else {"Shared display settings"};
                let symbol=if russian {"Кириллические символы"} else {"Cyrillic symbols"};
                let shared_center=bottom.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Text(t) if t.galley.job.text==shared=>Some(t.pos.x+t.galley.rect.center().x),_=>None}).unwrap();
                assert!((shared_center-expected_center).abs()<2.0,"shared heading must align with half headings, width={width}");
                let left=bottom.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Text(t) if t.galley.job.text==symbol=>Some(t.pos.x-2.0),_=>None}).unwrap();
                let block_width=(full.width()-18.0).min(900.0);
                assert!((left-(expected_center-block_width/2.0)).abs()<2.0,"shared row must not stretch to window width: width={width}, left={left}, expected_center={expected_center}, block_width={block_width}, full={full:?}");
                let switches:Vec<_>=bottom.shapes.iter().filter_map(|s|match &s.shape {egui::Shape::Rect(r) if r.rect.width()==46.0&&r.rect.height()==24.0=>Some(r.rect),_=>None}).collect();
                let right=switches.iter().map(|r|r.right()).fold(0.0_f32,f32::max);
                assert!((right-(left+block_width)).abs()<2.0,"shared controls must stay in the same bounded block");
            }
        }
    }

    #[test]
    fn changing_factory_right_ship_mode_reveals_new_default() {
        for new_mode in [0u8, 1, 3] {
            let mut values = [0, 0, 20, 1, 1, 2, 1, 0, 1, 1];
            let mut changed = Vec::new();
            sm62_apply_mode_change(&mut values, &mut changed, 1, new_mode);
            assert_eq!(values[5], new_mode);
            assert_eq!(values[6], 0);
            assert_eq!(values[7], 20);
            assert_eq!(changed, [5, 6, 7]);
        }
    }

    #[test]
    fn selecting_ship_mode_makes_it_continuous() {
        let mut values = [0, 0, 20, 1, 1, 0, 0, 20, 1, 1];
        let mut changed = Vec::new();
        sm62_apply_mode_change(&mut values, &mut changed, 1, 2);
        assert_eq!(values[5], 2);
        assert_eq!(values[6], 1);
        assert_eq!(changed, [5, 6]);
    }

    #[test]
    fn assigned_icon_editor_follows_live_accent_without_reloading_library() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let mut bitmap = vec![0u8; PICTOGRAM_BYTES];
        bitmap[0] = 0x80;
        app.display_settings.pictograms.library.set_colored(
            PictogramKind::Macro,
            0,
            &bitmap,
            [255, 0, 255],
        );
        app.display_settings.pictograms.source_levels = pictogram_bitmap_levels(&bitmap);
        app.display_settings.pictograms.selected_kind = PictogramKind::Macro;
        app.display_settings.pictograms.selected_slot = 0;
        for color in [[255, 255, 255], [19, 137, 220]] {
            app.display_settings.color = color;
            let output = crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput::default(), |ctx| {
                ctx.root_ui(|ui| egui::CentralPanel::default().show(ui, |ui| {
                    app.draw_pictogram_editor_panel(ui, 400.0);
                }));
            }));
            let icons: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) if mesh.vertices.len() == 4 => Some(mesh),
                    _ => None,
                })
                .collect();
            assert_eq!(icons.len(), 1);
            let expected = Color32::from_rgb(color[0], color[1], color[2]);
            assert!(icons[0]
                .vertices
                .iter()
                .all(|vertex| vertex.color == expected));
        }
    }

    #[test]
    fn standard_selection_is_not_a_duplicate_but_an_edited_copy_is() {
        let names = vec!["Камера".to_owned()];
        assert!(pictogram_is_builtin_selection("Камера", Some(0), &names));
        assert!(!pictogram_is_builtin_selection("Камера", None, &names));
        assert!(pictogram_name_error("Камера", &[], None, &names).is_some());
        assert!(!pictogram_is_builtin_selection(
            "Камера копия",
            Some(0),
            &names
        ));
        assert!(pictogram_name_error("Камера копия", &[], None, &names).is_none());
    }

    #[test]
    fn display_tabs_count_only_their_own_rows() {
        let mut settings = DisplaySettingsState {
            clock_settings_supported: true,
            background_color_supported: true,
            brightness_supported: true,
            button_style_supported: true,
            clock_info_color_supported: true,
            clock_background_asset_supported: true,
            clock_background_speed_supported: true,
            clock_background_dim_supported: true,
            clock_colon_blink_supported: true,
            ..DisplaySettingsState::default()
        };

        assert_eq!(display_settings_row_count(&settings, 0), 2);
        assert_eq!(display_settings_row_count(&settings, 1), 4);
        assert_eq!(display_settings_row_count(&settings, 2), 10);

        settings.clock_background_asset_supported = false;
        assert_eq!(display_settings_row_count(&settings, 2), 9);
    }

    #[test]
    fn color_hex_accepts_hash_and_plain_values() {
        assert_eq!(parse_rgb_hex("#12AbF0"), Some([0x12, 0xAB, 0xF0]));
        assert_eq!(parse_rgb_hex("12abf0"), Some([0x12, 0xAB, 0xF0]));
        assert_eq!(format_rgb_hex([0x12, 0xAB, 0xF0]), "#12ABF0");
        assert_eq!(parse_rgb_hex("#12345"), None);
        assert_eq!(parse_rgb_hex("#12GGF0"), None);
    }

    #[test]
    fn unsupported_standby_tab_has_no_rows() {
        let settings = DisplaySettingsState::default();
        assert_eq!(display_settings_row_count(&settings, 0), 2);
        assert_eq!(display_settings_row_count(&settings, 1), 1);
        assert_eq!(display_settings_row_count(&settings, 2), 0);
    }

    #[test]
    fn display_panels_stay_centered_in_wide_and_narrow_windows() {
        let wide = egui::Rect::from_min_size(egui::pos2(20.0, 100.0), egui::vec2(1160.0, 800.0));
        let (preview, settings, side_by_side) = display_settings_panel_rects(wide, 470.0);
        assert!(side_by_side);
        let group_center = (preview.left() + settings.right()) / 2.0;
        assert!((group_center - wide.center().x).abs() < f32::EPSILON);

        let narrow = egui::Rect::from_min_size(egui::pos2(20.0, 100.0), egui::vec2(720.0, 1000.0));
        let (preview, settings, side_by_side) = display_settings_panel_rects(narrow, 470.0);
        assert!(!side_by_side);
        assert!((preview.center().x - narrow.center().x).abs() < f32::EPSILON);
        assert!((settings.center().x - narrow.center().x).abs() < f32::EPSILON);
    }

    #[test]
    fn standby_rows_omit_modifiers() {
        let settings = DisplaySettingsState {
            clock_settings_supported: true,
            clock_info_color_supported: true,
            clock_overlay_controls_supported: true,
            ..DisplaySettingsState::default()
        };
        assert_eq!(display_settings_row_count(&settings, 2), 7);
    }

    #[test]
    fn pictogram_names_require_a_letter_safe_characters_and_uniqueness() {
        let saved = vec![SavedPictogram {
            origin: PictogramOrigin::Legacy,
            name: "Мой пресет_1".to_owned(),
            color: [1, 2, 3],
            bitmap: vec![0; PICTOGRAM_BYTES],
        }];
        let builtins = vec!["Воспроизведение".to_owned()];
        assert!(pictogram_name_error("Новый preset_2", &saved, None, &builtins).is_none());
        assert!(pictogram_name_error("2 preset", &saved, None, &builtins).is_some());
        assert!(pictogram_name_error("Preset-2", &saved, None, &builtins).is_some());
        assert!(pictogram_name_error("МОЙ ПРЕСЕТ_1", &saved, None, &builtins).is_some());
        assert!(pictogram_name_error("МОЙ ПРЕСЕТ_1", &saved, Some(0), &builtins).is_none());
        assert!(pictogram_name_error("воспроизведение", &saved, None, &builtins).is_some());
    }

    #[test]
    fn pictogram_tiles_preserve_device_colors_in_both_app_themes() {
        assert_eq!(visible_pictogram_color(true, [0, 0, 0]), Color32::BLACK);
        assert_eq!(
            visible_pictogram_color(false, [255, 255, 255]),
            Color32::WHITE
        );
        assert_eq!(
            visible_pictogram_color(true, [84, 189, 191]),
            Color32::from_rgb(84, 189, 191)
        );
    }

    #[test]
    fn delete_is_available_only_for_a_saved_user_pictogram() {
        assert!(can_delete_saved_pictogram(Some(0)));
        assert!(!can_delete_saved_pictogram(None));
    }

    #[test]
    fn startup_image_reset_requires_a_stored_custom_image_and_an_idle_device() {
        assert!(startup_image_reset_enabled(true, true, false));
        assert!(!startup_image_reset_enabled(true, false, false));
        assert!(!startup_image_reset_enabled(false, true, false));
        assert!(!startup_image_reset_enabled(true, true, true));
    }

    #[test]
    fn pictogram_editor_interpolates_every_cell_in_a_drag() {
        let cells = pictogram_editor_line((2, 3), (7, 3));
        assert_eq!(cells, vec![(2, 3), (3, 3), (4, 3), (5, 3), (6, 3), (7, 3)]);
        let diagonal = pictogram_editor_line((1, 1), (4, 4));
        assert_eq!(diagonal, vec![(1, 1), (2, 2), (3, 3), (4, 4)]);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod pictogram_confirmation_tests {
    use super::super::vial_hid_task::{VialHidOperation, VialHidTaskStart};
    use super::*;

    fn poll(app: &mut EntropyApp, ctx: &egui::Context) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.vial_hid_task.is_some() {
            assert!(
                std::time::Instant::now() < deadline,
                "pictogram worker timed out"
            );
            app.poll_vial_hid_task(ctx);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn two_actual_picker_assignments_replace_the_same_slot_without_reconnect() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid);
        app.display_settings.pictograms.supported = Some(true);
        app.display_settings.pictograms.loaded = true;
        let generation = app.connection_generation;
        for byte in [0xAA, 0x55] {
            recorder.respond_with(test_pictogram_upload_responses(true, 0));
            let before = app.display_settings.pictograms.library.clone();
            let bitmap = [byte; PICTOGRAM_BYTES];
            assert!(app.assign_selected_pictogram(&ctx, Some(&bitmap)));
            assert_eq!(app.display_settings.pictograms.library, before);
            poll(&mut app, &ctx);
            assert_eq!(app.connection_generation, generation);
            assert!(app.hid_device.is_some());
            assert!(app.display_settings.pictograms.loaded);
            assert!(!app.display_settings.pictograms.loading);
            assert!(!app.vial_hid_task_blocks_user_action());
            assert_eq!(
                app.display_settings
                    .pictograms
                    .library
                    .bitmap(PictogramKind::Macro, 0),
                Some(bitmap.as_slice())
            );
        }
        assert_eq!(
            recorder
                .requests()
                .iter()
                .filter(|request| request[0] == 0xC9)
                .count(),
            2
        );
    }

    #[test]
    fn rejected_slot_assignment_save_reset_and_full_upload_never_confirm_pending_library() {
        for action in 0..4 {
            let ctx = egui::Context::default();
            let mut app = EntropyApp::new_inert_for_test();
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            let backup = tempfile::tempdir_in(
                std::env::var_os("ENTROPY_TEST_ARTIFACT_DIR")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir),
            )
            .unwrap();
            recorder.set_pictogram_backup_directory(backup.path().join("pictogram-backups"));
            recorder.respond_with(test_pictogram_upload_responses(action != 3, 4));
            app.hid_device = Some(hid);
            let p = &mut app.display_settings.pictograms;
            p.supported = Some(true);
            p.loaded = true;
            p.library
                .set(PictogramKind::Macro, 0, &vec![0xAA; PICTOGRAM_BYTES]);
            p.library
                .set(PictogramKind::TapDance, 2, &vec![0xBB; PICTOGRAM_BYTES]);
            let before = p.library.clone();
            let mut desired = before.clone();
            desired.set(PictogramKind::Macro, 0, &vec![0xFF; PICTOGRAM_BYTES]);
            match action {
                0 => {
                    assert!(app.assign_selected_pictogram(&ctx, Some(&vec![0xFF; PICTOGRAM_BYTES])))
                }
                1 => assert!(app.apply_current_pictogram(&ctx)),
                2 => assert!(app.reset_current_pictogram(&ctx)),
                _ => assert!(matches!(
                    app.start_vial_hid_operation(
                        &ctx,
                        VialHidOperation::PictogramUpload { library: desired }
                    ),
                    VialHidTaskStart::Started
                )),
            }
            assert_eq!(
                app.display_settings.pictograms.library, before,
                "pending write must not change confirmed state"
            );
            // A user can continue editing while the worker finishes.
            app.display_settings.pictograms.source_levels =
                vec![42; PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT];
            app.display_settings.pictograms.editor_name = "unsaved draft".into();
            app.display_settings.pictograms.undo =
                vec![vec![17; PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT]];
            poll(&mut app, &ctx);
            assert!(app.status_msg.contains("status 4"), "{}", app.status_msg);
            assert!(recorder
                .requests()
                .iter()
                .any(|request| request[0] == if action == 3 { 0xC4 } else { 0xC9 }));
            let p = &app.display_settings.pictograms;
            assert!(!p.loaded);
            assert!(!p.loading);
            assert!(!p.library.has(PictogramKind::Macro, 0));
            assert_eq!(p.editor_name, "unsaved draft");
            let draft = p.source_levels.clone();
            let undo = p.undo.clone();
            assert!(
                !app.apply_current_pictogram(&ctx),
                "unknown device state cannot seed another slot/full upload"
            );
            recorder.respond_with(test_pictogram_read_responses(&before));
            assert!(matches!(
                app.start_vial_hid_operation(
                    &ctx,
                    VialHidOperation::PictogramLoad {
                        preserve_editor: true
                    }
                ),
                VialHidTaskStart::Started
            ));
            poll(&mut app, &ctx);
            let p = &app.display_settings.pictograms;
            assert!(p.loaded);
            assert_eq!(p.library, before);
            assert_eq!(p.source_levels, draft);
            assert_eq!(p.undo, undo);
            assert_eq!(p.editor_name, "unsaved draft");
        }
    }

    #[test]
    fn successful_slot_and_full_upload_confirm_only_on_completion_and_keep_newer_edits() {
        for slot_upload in [true, false] {
            let ctx = egui::Context::default();
            let mut app = EntropyApp::new_inert_for_test();
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            let backup = tempfile::tempdir_in(
                std::env::var_os("ENTROPY_TEST_ARTIFACT_DIR")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir),
            )
            .unwrap();
            recorder.set_pictogram_backup_directory(backup.path().join("pictogram-backups"));
            recorder.respond_with(test_pictogram_upload_responses(slot_upload, 0));
            app.hid_device = Some(hid);
            app.display_settings.pictograms.loaded = true;
            app.display_settings.pictograms.supported = Some(true);
            let before = app.display_settings.pictograms.library.clone();
            let mut desired = before.clone();
            desired.set_colored(
                PictogramKind::Macro,
                0,
                &vec![0xFF; PICTOGRAM_BYTES],
                app.display_settings.color,
            );
            if slot_upload {
                assert!(app.assign_selected_pictogram(&ctx, Some(&vec![0xFF; PICTOGRAM_BYTES])));
            } else {
                assert!(matches!(
                    app.start_vial_hid_operation(
                        &ctx,
                        VialHidOperation::PictogramUpload {
                            library: desired.clone()
                        }
                    ),
                    VialHidTaskStart::Started
                ));
            }
            assert_eq!(app.display_settings.pictograms.library, before);
            app.display_settings.pictograms.editor_name = "newer draft".into();
            poll(&mut app, &ctx);
            assert!(app.display_settings.pictograms.loaded);
            assert_eq!(app.display_settings.pictograms.library, desired);
            assert_eq!(app.display_settings.pictograms.editor_name, "newer draft");
        }
    }

    #[test]
    fn uncertain_transport_error_invalidates_confirmed_library_without_losing_editor() {
        for fault in [
            crate::hid::TestHidFault::Timeout,
            crate::hid::TestHidFault::Disconnect,
            crate::hid::TestHidFault::WorkerPanic,
        ] {
            let ctx = egui::Context::default();
            let mut app = EntropyApp::new_inert_for_test();
            let (hid, _) =
                crate::hid::HidDevice::test_device_with_fault_after_requests(Some((0, fault)));
            app.hid_device = Some(hid);
            app.display_settings.pictograms.loaded = true;
            app.display_settings.pictograms.supported = Some(true);
            app.display_settings.pictograms.editor_name = "retain".into();
            app.display_settings.pictograms.source_levels =
                vec![42; PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT];
            let draft = app.display_settings.pictograms.source_levels.clone();
            assert!(app.apply_current_pictogram(&ctx));
            poll(&mut app, &ctx);
            assert!(!app.display_settings.pictograms.loaded);
            assert!(!app.display_settings.pictograms.loading);
            assert_eq!(app.display_settings.pictograms.source_levels, draft);
            assert_eq!(app.display_settings.pictograms.editor_name, "retain");
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod icon_catalog_tests {
    use super::*;

    #[test]
    fn only_exact_migrated_stock_copies_are_hidden_in_russian_picker() {
        let catalog = pictogram_catalog(crate::i18n::Language::Russian);
        let stock_index = 0;
        let stock_name = crate::i18n::tr_catalog(
            crate::i18n::Language::English,
            BUILTIN_PICTOGRAM_KEYS[stock_index],
        );
        let saved = vec![
            SavedPictogram {
                origin: PictogramOrigin::Legacy,
                name: stock_name.to_owned(),
                color: [84, 189, 191],
                bitmap: legacy_builtin_pictogram_bitmap(stock_index).to_vec(),
            },
            SavedPictogram {
                origin: PictogramOrigin::Legacy,
                name: "My drawing".to_owned(),
                color: [255, 255, 255],
                bitmap: vec![0x5A; PICTOGRAM_BYTES],
            },
        ];
        let visible = visible_saved_pictograms(&saved, &catalog);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].name, "My drawing");
    }

    #[test]
    fn saved_name_bitmap_and_recolored_stock_collisions_remain_selectable() {
        let catalog = pictogram_catalog(crate::i18n::Language::English);
        let stock_name =
            crate::i18n::tr_catalog(crate::i18n::Language::English, BUILTIN_PICTOGRAM_KEYS[0]);
        let saved = vec![
            SavedPictogram {
                origin: PictogramOrigin::Legacy,
                name: stock_name.to_owned(),
                color: [84, 189, 191],
                bitmap: vec![0xA5; PICTOGRAM_BYTES],
            },
            SavedPictogram {
                origin: PictogramOrigin::Legacy,
                name: "My stock shape".to_owned(),
                color: [84, 189, 191],
                bitmap: builtin_pictogram_bitmap(0).to_vec(),
            },
            SavedPictogram {
                origin: PictogramOrigin::User,
                name: stock_name.to_owned(),
                color: [255, 0, 0],
                bitmap: legacy_builtin_pictogram_bitmap(0).to_vec(),
            },
        ];
        let visible = visible_saved_pictograms(&saved, &catalog);
        assert_eq!(visible.len(), 3);
        let builtin_names = vec![stock_name.to_owned()];
        assert!(pictogram_name_error(stock_name, &saved[..1], Some(0), &builtin_names).is_none());
    }

    #[test]
    fn stock_catalog_keys_match_the_bitmap_sources() {
        assert_eq!(
            &BUILTIN_PICTOGRAM_KEYS[38..38 + crate::action_icons::ACTION_ICON_COUNT],
            &crate::action_icons::ACTION_ICON_KEYS,
        );
        let firmware_start = 38 + crate::action_icons::ACTION_ICON_COUNT;
        assert_eq!(
            &BUILTIN_PICTOGRAM_KEYS[firmware_start
                ..firmware_start + crate::firmware_builtin_icons::FIRMWARE_ICON_COUNT],
            &crate::firmware_builtin_icons::FIRMWARE_ICON_KEYS,
        );
        let appended_start = firmware_start
            + crate::firmware_builtin_icons::FIRMWARE_ICON_COUNT
            + crate::integration_icons::INTEGRATION_ICON_COUNT;
        assert_eq!(
            &BUILTIN_PICTOGRAM_KEYS[appended_start..],
            &["display_settings.pictogram_firmware_delete"],
            "native additions must be append-only after the unchanged source families",
        );
        assert_eq!(BUILTIN_PICTOGRAM_KEYS.len(), appended_start + 1);
    }

    #[test]
    fn equivalent_legacy_and_firmware_icons_have_one_picker_tile() {
        for language in crate::i18n::Language::ALL {
            let catalog = pictogram_catalog(language);
            for (legacy, current) in [
                (14, 160), // mail
                (17, 163), // home
                (23, 167), // screenshot
                (24, 168), // left mouse button
                (25, 170), // right mouse button
                (26, 169), // middle mouse button
                (31, 155), // brightness down
                (32, 156), // brightness up
                (33, 158), // computer
                (34, 159), // web search
                (37, 161), // calculator
            ] {
                assert!(!catalog.iter().any(|icon| icon.index == legacy));
                assert!(catalog.iter().any(|icon| icon.index == current));
            }
        }
    }

    #[test]
    fn picker_has_unique_names_and_artwork_in_each_language() {
        for language in crate::i18n::Language::ALL {
            let catalog = pictogram_catalog(language);
            let names: std::collections::HashSet<_> = catalog
                .iter()
                .map(|icon| icon.name.trim().to_lowercase())
                .collect();
            let bitmaps: std::collections::HashSet<_> =
                catalog.iter().map(|icon| icon.bitmap).collect();
            assert_eq!(names.len(), catalog.len());
            assert_eq!(bitmaps.len(), catalog.len());
            assert!(catalog
                .iter()
                .all(|icon| icon.bitmap.iter().any(|byte| *byte != 0)));
            let layer_prev = 38 + crate::action_icons::ACTION_ICON_COUNT + 20;
            let layer_next = layer_prev + 1;
            assert!(catalog.iter().any(|icon| icon.index == layer_prev));
            assert!(catalog.iter().any(|icon| icon.index == layer_next));
            assert!(
                catalog
                    .iter()
                    .all(|icon| { icon.name != BUILTIN_PICTOGRAM_KEYS[icon.index] }),
                "untranslated pictogram tooltip in {language:?}"
            );
            let prev = catalog
                .iter()
                .position(|icon| icon.index == layer_prev)
                .unwrap();
            let next = catalog
                .iter()
                .position(|icon| icon.index == layer_next)
                .unwrap();
            assert_eq!(next, prev + 1, "layer navigation icons must be adjacent");
            assert_eq!(catalog[prev].group, 0);
            for group in 5..=13 {
                assert!(catalog.iter().any(|icon| icon.group == group));
            }
            let titles: std::collections::HashSet<_> = PICTOGRAM_GROUP_KEYS
                .iter()
                .map(|key| crate::i18n::tr_catalog(language, key))
                .collect();
            assert_eq!(titles.len(), PICTOGRAM_GROUP_KEYS.len());
        }
    }

    #[test]
    fn playback_split_does_not_reassign_other_groups() {
        assert_eq!(
            pictogram_group("display_settings.pictogram_builtin_play"),
            5
        );
        assert_eq!(
            pictogram_group("display_settings.pictogram_builtin_copy"),
            6
        );
        assert_eq!(
            pictogram_group("display_settings.pictogram_builtin_action_arrow_left"),
            9
        );
        assert_eq!(
            pictogram_group("display_settings.pictogram_builtin_previous_track"),
            14
        );
        assert_eq!(
            pictogram_group("display_settings.pictogram_builtin_action_step_back"),
            15
        );
        assert_eq!(
            pictogram_group("display_settings.pictogram_builtin_volume"),
            16
        );
        assert_eq!(
            pictogram_group("display_settings.pictogram_builtin_camera"),
            17
        );
        assert_eq!(
            pictogram_group("display_settings.pictogram_firmware_mouse_left"),
            18
        );
        assert_eq!(
            pictogram_group("display_settings.pictogram_integration_obs_record"),
            20
        );
    }

    #[test]
    fn every_picker_icon_has_an_explicit_order_within_its_existing_group() {
        for language in crate::i18n::Language::ALL {
            let catalog = pictogram_catalog(language);
            for icon in catalog {
                let key = BUILTIN_PICTOGRAM_KEYS[icon.index];
                assert!(
                    !pictogram_sort_key(key).starts_with("999_"),
                    "missing order for {key} in group {}",
                    icon.group
                );
            }
            let position = |suffix: &str| {
                catalog
                    .iter()
                    .position(|icon| BUILTIN_PICTOGRAM_KEYS[icon.index].ends_with(suffix))
                    .unwrap()
            };
            assert_eq!(
                position("pictogram_builtin_double_tap"),
                position("pictogram_builtin_single_tap") + 1
            );
            assert_eq!(
                position("pictogram_builtin_hold"),
                position("pictogram_builtin_double_tap") + 1
            );
            assert_eq!(
                position("pictogram_builtin_cut") + 1,
                position("pictogram_builtin_action_copy")
            );
            assert_eq!(
                position("pictogram_builtin_action_copy") + 1,
                position("pictogram_builtin_action_paste")
            );
        }
    }
}

// Shared with the macropad standby dropdown. SM62 sends seconds, not enum IDs.
include!("sm62_shared_settings.rs");
#[cfg(test)]
#[path = "display_settings/sm62_shared_write_tests.rs"]
mod sm62_shared_write_tests;
#[cfg(test)]
#[path = "display_settings/sm62_review_controls_tests.rs"]
mod sm62_review_controls_tests;
include!("sm62_scroll.rs");

const STANDBY_DELAY_KEYS: [&str; 8] = [
    "display_settings.never", "display_settings.seconds_5", "display_settings.seconds_10",
    "display_settings.seconds_15", "display_settings.seconds_20", "display_settings.seconds_30",
    "display_settings.minute_1", "display_settings.minutes_2",
];

fn sm62_apply_mode_change(values: &mut [u8; 10], changed: &mut Vec<usize>, side: usize, mode: u8) {
    let base = side * 5;
    if mode != values[base] {
        let leaving_ship = values[base] == 2 && mode != 2;
        values[base] = mode;
        changed.push(base);
        // The factory right screen has an idle delay of zero. On leaving its
        // continuous ship mode, turn idle animation off so the new default is
        // actually visible. The user can enable idle animation explicitly.
        if leaving_ship {
            if values[base + 1] != 0 {
                values[base + 1] = 0;
                changed.push(base + 1);
            }
            if values[base + 2] == 0 {
                values[base + 2] = 20;
                changed.push(base + 2);
            }
        } else if mode == 2 && values[base + 1] == 0 {
            values[base + 1] = 1;
            changed.push(base + 1);
        }
    }
}

fn sm62_display_controls(
    ui: &mut egui::Ui,
    side: usize,
    values: &mut [u8; 10],
    changed: &mut Vec<usize>,
    russian: bool,
) -> egui::Id {
    let base = side * 5;
    let width = ui.available_width();
    let row_height = 54.0;
    let control_width = (width * 0.34).clamp(120.0, 196.0);
    let modes: Vec<String> = (if russian { ["Часы", "Плеер", "Корабль", "Раскладка"] }
        else { ["Clock", "Player", "Spaceship", "Layout"] })
        .into_iter().map(str::to_owned).collect();
    let mut mode = values[base].min(3);
    let mut mode_edited = false;
    display_settings_row(ui, width, row_height,
        if russian { "Режим по умолчанию" } else { "Default mode" },
        true, control_width, |ui| {
            let (_, picked) = crate::ui_style::modern_dropdown_select_sized(
                ui, ui.make_persistent_id(("sm62_mode", side)), &modes,
                mode as usize, control_width, 34.0, 12.0);
            if let Some(picked) = picked {
                mode = picked as u8;
                mode_edited = ui.is_enabled();
            }
        });
    if mode_edited {
        sm62_apply_mode_change(values, changed, side, mode);
        // Couple this option only to an explicit enabled mode selection.
        // Stored Layout/keypress=0 is valid input to a read-only render.
        if mode == 3 && values[base + 3] == 0 {
            values[base + 3] = 1;
            changed.push(base + 3);
        }
    }

    let language = if russian { crate::i18n::Language::Russian } else { crate::i18n::Language::English };
    let mut options: Vec<Option<u8>> = vec![None, Some(5), Some(10), Some(15), Some(20), Some(30), Some(60), Some(120)];
    let mut labels: Vec<String> = STANDBY_DELAY_KEYS.iter()
        .map(|key| crate::i18n::tr_catalog(language, key).to_owned()).collect();
    // SM62 wording only; macropad's shared intervals/catalog remain unchanged.
    labels[0] = if russian { "Без анимации" } else { "No animation" }.to_owned();
    let current = if values[base + 1] == 0 { None } else { Some(values[base + 2]) };
    let selected = options.iter().position(|value| *value == current).unwrap_or_else(|| {
        options.push(current);
        labels.push(format!("{} {}", values[base + 2], if russian { "с" } else { "s" }));
        options.len() - 1
    });
    let mut picked = None;
    let mut idle_dropdown_id = egui::Id::NULL;
    display_settings_row(ui, width, row_height,
        if russian { "Анимация ожидания через" } else { "Enable standby mode after" },
        true, control_width, |ui| {
            ui.add_enabled_ui(mode != 2, |ui| {
                idle_dropdown_id = ui.make_persistent_id(("sm62_idle_delay", side));
                let (_, choice) = crate::ui_style::modern_dropdown_select_sized(
                    ui, idle_dropdown_id, &labels,
                    selected, control_width, 34.0, 12.0);
                picked = choice;
            });
        });
    if let Some(picked) = picked {
        let timeout = options[picked];
        if let Some(timeout) = timeout {
            if values[base + 2] != timeout {
                values[base + 2] = timeout;
                changed.push(base + 2);
            }
        }
        let enabled = u8::from(timeout.is_some());
        if values[base + 1] != enabled {
            values[base + 1] = enabled;
            changed.push(base + 1);
        }
    }
    for (offset, label_ru, label_en) in [
        (3, "Показывать раскладку при нажатии", "Show layout on key press"),
    ] {
        let locked_layout = offset == 3 && mode == 3;
        let locked_ship = offset == 1 && mode == 2;
        let mut enabled = values[base + offset] != 0;
        let mut edited = false;
        display_settings_row(ui, width, row_height,
            if russian { label_ru } else { label_en },
            true, control_width, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(!locked_layout && !locked_ship, |ui| {
                        edited = crate::ui_style::settings_switch_sized_stable(
                            ui, ("sm62_switch", side, offset), &mut enabled,
                            Vec2::new(46.0, 24.0)).clicked();
                    });
                });
            });
        if edited && u8::from(enabled) != values[base + offset] {
            values[base + offset] = u8::from(enabled);
            changed.push(base + offset);
        }
    }
    idle_dropdown_id
}

impl EntropyApp {
    fn draw_sm62_display_settings_page(&mut self, ui: &mut egui::Ui, content_rect: egui::Rect) {
        let russian = self.app_settings.language == crate::i18n::Language::Russian;
        let preview_language = match self.standby_preview_language() { "EN" => Some("EN"), "RU" => Some("RU"), _ => None };
        let supported = self.display_settings.sm62_supported;
        // A disconnected preview must not pretend that firmware has live time.
        let clock_live = self.shared_hid_output.is_some();
        #[cfg(test)]
        let clock_live = clock_live || std::env::var("ENTROPY_RENDER_CLOCK_LIVE").as_deref()==Ok("1");
        let ready = supported && self.qmk_setting_transport_available();
        let mut values = self.display_settings.sm62_values;
        let confirmed = self.display_settings.confirmed_sm62_values;
        let mut date_values = self.display_settings.sm62_date_values;
        let confirmed_date = self.display_settings.confirmed_sm62_date_values;
        let language_supported = self.display_settings.sm62_language_supported;
        let mut language_values = self.display_settings.sm62_language_values;
        let confirmed_language = self.display_settings.confirmed_sm62_language_values;
        let stars_supported = self.display_settings.sm62_stars_supported;
        let mut stars_values = self.display_settings.sm62_stars_values;
        let confirmed_stars = self.display_settings.confirmed_sm62_stars_values;
        let screens_off_supported = self.display_settings.sm62_screens_off_supported;
        let mut screens_off_values = self.display_settings.sm62_screens_off_values;
        let confirmed_screens_off = self.display_settings.confirmed_sm62_screens_off_values;
        let date_supported = self.display_settings.sm62_date_supported;
        let hold_supported = self.display_settings.sm62_hold_supported;
        let mut hold_values = self.display_settings.sm62_hold_ms;
        let confirmed_hold = self.display_settings.confirmed_sm62_hold_ms;
        // Enqueue only edits made by this frame's controls, not an older
        // optimistic value left unconfirmed after bounded worker retries.
        let before_shared = (stars_values, screens_off_values, language_values, hold_values, date_values);
        let mut changed = Vec::new();
        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(RichText::new(if russian { "Дисплеи SM62" } else { "SM62 displays" })
                    .size(18.0).strong());
                ui.add_space(6.0);
                ui.label(RichText::new(if russian {
                    "Режимы экранов настраиваются отдельно. Настройки сохраняются в клавиатуре."
                } else {
                    "Display modes are configured independently. Settings are saved on the keyboard."
                }).size(13.0).color(app_muted_text(ui.visuals().dark_mode)));
                if !supported {
                    ui.add_space(8.0);
                    ui.colored_label(egui::Color32::from_rgb(220, 92, 76), if russian {
                        "Entropy не удалось прочитать настройки дисплеев из подключённой SM62. Проверьте соединение и версию прошивки."
                    } else {
                        "Entropy could not read display settings from the connected SM62. Check the connection and firmware version."
                    });
                } else if !ready {
                    ui.add_space(8.0);
                    ui.colored_label(egui::Color32::from_rgb(220, 92, 76), if russian {
                        "Нет связи с SM62. Переподключите клавиатуру для изменения настроек."
                    } else { "SM62 is disconnected. Reconnect the keyboard to change settings." });
                }
                ui.add_space(16.0);
                // Each half exposes all settings directly; preview its default mode.
                let scene = 0;
                let body=ui.available_rect_before_wrap();
                sm62_display_scroll(ui,body,|ui| {
                        ui.set_width(ui.available_width().min(900.0));
                        for side in 0..2usize {
                            ui.separator();
                            ui.heading(if side == 0 {
                                if russian { "Левый экран" } else { "Left display" }
                            } else if russian { "Правый экран" } else { "Right display" });
                            ui.add_space(8.0);
                            let base = side * 5;
                            let side_values: [u8; 5] = values[base..base + 5].try_into().unwrap();
                            let wide = ui.available_width() >= 620.0;
                            if wide {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.set_width(245.0);
                                        super::display_preview::draw_sm62_preview(ui, &side_values, scene, date_supported && date_values[side] != 0, 245.0, side, if language_values[side] != 0 { preview_language } else { None }, stars_values[side] != 0, screens_off_values[side] != 0, clock_live);
                                    });
                                    ui.add_space(20.0);
                                    ui.vertical(|ui| {
                                        ui.set_width(ui.available_width().min(560.0));
                                        ui.add_enabled_ui(ready, |ui| {
                                            sm62_display_controls(ui, side, &mut values, &mut changed, russian);
                                        });
                                    });
                                });
                            } else {
                                ui.vertical_centered(|ui| {
                                    super::display_preview::draw_sm62_preview(ui, &side_values, scene, date_supported && date_values[side] != 0, 210.0, side, if language_values[side] != 0 { preview_language } else { None }, stars_values[side] != 0, screens_off_values[side] != 0, clock_live);
                                });
                                ui.add_space(10.0);
                                ui.add_enabled_ui(ready, |ui| {
                                    sm62_display_controls(ui, side, &mut values, &mut changed, russian);
                                });
                            }
                            ui.add_space(16.0);
                        }
                        ui.separator();
                        ui.add_space(12.0);
                        ui.add_enabled_ui(ready, |ui| {
                            sm62_shared_controls(ui, &mut values, &mut changed, russian,
                                date_supported.then_some(&mut date_values),
                                hold_supported.then_some(&mut hold_values),
                                language_supported.then_some(&mut language_values),
                                stars_supported.then_some(&mut stars_values),
                                screens_off_supported.then_some(&mut screens_off_values));
                            self.draw_sm62_timeout_control(ui);
                        });
                    });
            });
        });
        self.display_settings.sm62_values = values;
        self.display_settings.sm62_date_values = date_values;
        self.display_settings.sm62_language_values = language_values;
        self.display_settings.sm62_stars_values = stars_values;
        self.display_settings.sm62_screens_off_values = screens_off_values;
        for (supported, values, before, confirmed, qsid, label) in [
            (stars_supported,stars_values,before_shared.0,confirmed_stars,246u16,"stars"),
            (screens_off_supported,screens_off_values,before_shared.1,confirmed_screens_off,248u16,"screens off"),
        ] {
            if ready && supported { for side in 0..2 {
                if values[side] != before[side] && self.explicit_display_setting_needs_write(qsid+side as u16, confirmed[side] as u16, values[side] as u16) {
                    self.queue_display_setting_write(format!("SM62 {} {}",if side==0 {"left"} else {"right"},label),
                        qsid+side as u16,confirmed[side] as u16,values[side] as u16);
                }
            } }
        }
        if ready && language_supported {
            for side in 0..2 {
                if language_values[side] != before_shared.2[side] && self.explicit_display_setting_needs_write(244+side as u16, confirmed_language[side] as u16, language_values[side] as u16) {
                    self.queue_display_setting_write(format!("SM62 {} language", if side == 0 { "left" } else { "right" }),
                        244 + side as u16, confirmed_language[side] as u16, language_values[side] as u16);
                }
            }
        }
        self.display_settings.sm62_hold_ms = hold_values;
        if ready && hold_supported {
            for side in 0..2 {
                if hold_values[side] != before_shared.3[side] && self.explicit_display_setting_needs_write(242+side as u16, confirmed_hold[side], hold_values[side]) {
                    self.queue_display_setting_write(
                        format!("SM62 {} layout hold", if side == 0 { "left" } else { "right" }),
                        242 + side as u16, confirmed_hold[side], hold_values[side],
                    );
                }
            }
        }
        if ready && date_supported {
            for side in 0..2 {
                if date_values[side] != before_shared.4[side] && self.explicit_display_setting_needs_write(240+side as u16, confirmed_date[side] as u16, date_values[side] as u16) {
                    self.queue_display_setting_write(
                        format!("SM62 {} date", if side == 0 { "left" } else { "right" }),
                        240 + side as u16, confirmed_date[side] as u16, date_values[side] as u16,
                    );
                }
            }
        }
        for index in changed {
            let qsid = 230 + index as u16;
            self.queue_display_setting_write(
                format!("SM62 {} {}", if index < 5 { "left" } else { "right" }, qsid),
                qsid, confirmed[index] as u16, values[index] as u16,
            );
        }
    }
}
