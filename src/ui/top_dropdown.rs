use super::*;

pub(super) fn device_dropdown_open_id() -> egui::Id {
    egui::Id::new("device_dropdown_open")
}

pub(super) fn device_layer_operations_submenu_open_id() -> egui::Id {
    egui::Id::new("device_layer_operations_submenu_open")
}

pub(super) fn advanced_dropdown_open_id() -> egui::Id {
    egui::Id::new("advanced_dropdown_open")
}

pub(super) fn settings_dropdown_open_id() -> egui::Id {
    egui::Id::new("settings_dropdown_open")
}

pub(super) fn top_dropdown_frame(dark: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(app_surface_fill(dark))
        .stroke(crate::ui_style::modal_outline_stroke(dark))
        .corner_radius(12.0)
        .inner_margin(egui::Margin::symmetric(8, 6))
}

/// Shows a top dropdown popup at `pos`: a foreground area with the shared
/// frame and rows laid out flush. Frame margin and stroke, row height and
/// divider height are the whole geometry, so `top_dropdown_height`
/// predicts the popup exactly.
pub(super) fn show_top_dropdown<R>(
    ctx: &egui::Context,
    id: egui::Id,
    pos: egui::Pos2,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            top_dropdown_frame(ui.visuals().dark_mode)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    // The popup Ui persists across frames; begin with a fresh block.
                    ui.ctx().data_mut(|data| {
                        data.remove::<TopMenuGroup>(top_menu_block_id(ui));
                    });
                    add_contents(ui)
                })
                .inner
        })
}

/// Outer height of a popup shown by `show_top_dropdown` with the given
/// rows and dividers. Hover bridges rely on it matching the rendered popup.
pub(super) fn top_dropdown_height(rows: usize, dividers: usize) -> f32 {
    top_dropdown_frame(false).total_margin().sum().y
        + rows as f32 * TOP_DROPDOWN_ITEM_HEIGHT
        + dividers as f32 * TOP_DROPDOWN_DIVIDER_HEIGHT
}

/// Height of a row inside a top dropdown.
pub(super) const TOP_DROPDOWN_ITEM_HEIGHT: f32 = 30.0;
/// Height of a group divider row inside a top dropdown.
pub(super) const TOP_DROPDOWN_DIVIDER_HEIGHT: f32 = 9.0;
/// Extra row width taken by the icon column.
pub(super) const TOP_DROPDOWN_ICON_COLUMN: f32 = 24.0;
/// Left inset of the label in a row with an icon.
pub(super) const TOP_DROPDOWN_ICON_TEXT_LEFT: f32 = 10.0 + TOP_DROPDOWN_ICON_COLUMN;

// Named family that resolves symbols from Noto Emoji first.
// The proportional family would pick some glyphs from egui's bundled
// emoji fonts, which are drawn in a different style.
const TOP_MENU_ICON_FAMILY: &str = "emoji_preview";

/// Decides which menu groups get a divider after them, given the number
/// of visible rows per group. A group with a single row is too small to
/// stand alone: it joins the block before it, or the next block when it
/// opens the menu.
pub(super) fn top_menu_dividers<const N: usize>(group_sizes: [usize; N]) -> [bool; N] {
    // Each block is (index of its last group, visible rows).
    let mut blocks: Vec<(usize, usize)> = Vec::new();
    for (index, size) in group_sizes.into_iter().enumerate() {
        if size == 0 {
            continue;
        }
        match blocks.last_mut() {
            Some((last_group, rows)) if size < 2 || *rows < 2 => {
                *last_group = index;
                *rows += size;
            }
            _ => blocks.push((index, size)),
        }
    }
    let mut divider_after = [false; N];
    for (last_group, _) in blocks.iter().take(blocks.len().saturating_sub(1)) {
        divider_after[*last_group] = true;
    }
    divider_after
}

// Paint-only cursor: a divider, not a catalog group change, starts a new
// visible block. The Ui id keeps submenu tints independent of their parent.
fn top_menu_block_id(ui: &egui::Ui) -> egui::Id {
    ui.id().with("top_menu_block_group")
}

fn top_menu_block_tint(ui: &egui::Ui, icon: TopMenuIcon, dark: bool) -> Color32 {
    let group = ui.ctx().data_mut(|data| {
        let id = top_menu_block_id(ui);
        data.get_temp::<TopMenuGroup>(id).unwrap_or_else(|| {
            let group = icon.group();
            data.insert_temp(id, group);
            group
        })
    });
    group.tint(dark)
}

pub(super) fn top_dropdown_divider(ui: &mut egui::Ui, width: f32) {
    ui.ctx().data_mut(|data| {
        data.remove::<TopMenuGroup>(top_menu_block_id(ui));
    });
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, TOP_DROPDOWN_DIVIDER_HEIGHT),
        Sense::hover(),
    );
    if ui.is_rect_visible(rect) {
        ui.painter().hline(
            egui::Rangef::new(rect.left() + 10.0, rect.right() - 10.0),
            rect.center().y,
            crate::ui_style::modal_outline_stroke(ui.visuals().dark_mode),
        );
    }
}

pub(super) fn top_dropdown_item(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    enabled: bool,
    selected: bool,
) -> egui::Response {
    top_dropdown_item_with_accessory(
        ui,
        width,
        label,
        enabled,
        selected,
        TopDropdownItemAccessory::None,
        None,
    )
}

pub(super) fn top_dropdown_icon_item(
    ui: &mut egui::Ui,
    width: f32,
    icon: TopMenuIcon,
    label: &str,
    enabled: bool,
    selected: bool,
) -> egui::Response {
    top_dropdown_item_with_accessory(
        ui,
        width,
        label,
        enabled,
        selected,
        TopDropdownItemAccessory::None,
        Some(icon),
    )
}

pub(super) fn top_dropdown_icon_item_with_indicator(
    ui: &mut egui::Ui,
    width: f32,
    icon: TopMenuIcon,
    label: &str,
    enabled: bool,
    selected: bool,
    show_indicator: bool,
) -> egui::Response {
    top_dropdown_item_with_accessory(
        ui,
        width,
        label,
        enabled,
        selected,
        if show_indicator {
            TopDropdownItemAccessory::Indicator
        } else {
            TopDropdownItemAccessory::None
        },
        Some(icon),
    )
}

pub(super) fn top_dropdown_icon_submenu_item(
    ui: &mut egui::Ui,
    width: f32,
    icon: TopMenuIcon,
    label: &str,
    enabled: bool,
    submenu_open: bool,
) -> egui::Response {
    top_dropdown_item_with_accessory(
        ui,
        width,
        label,
        enabled,
        submenu_open,
        TopDropdownItemAccessory::Submenu,
        Some(icon),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TopDropdownItemAccessory {
    None,
    Indicator,
    Submenu,
}

fn top_dropdown_item_with_accessory(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    enabled: bool,
    selected: bool,
    accessory: TopDropdownItemAccessory,
    icon: Option<TopMenuIcon>,
) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, TOP_DROPDOWN_ITEM_HEIGHT), sense);
    // Even a selected or disabled first row determines this block's tint.
    let block_tint = icon.map(|icon| top_menu_block_tint(ui, icon, dark));
    // Rows are painted by hand, so assistive technology learns the role,
    // label, enabled state and selection from here.
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, label)
    });
    let hovered = resp.hovered() && enabled;
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if ui.is_rect_visible(rect) {
        if selected || hovered {
            let fill = app_hover_fill(dark);
            ui.painter().rect_filled(rect, 8.0, fill);
        }

        let text_color = if !enabled {
            app_muted_text(dark)
        } else if selected {
            app_accent()
        } else {
            ui.visuals().text_color()
        };
        let reserve_right = selected || accessory == TopDropdownItemAccessory::Submenu;
        let text_clip = if reserve_right {
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.right() - 24.0, rect.bottom()))
        } else {
            rect
        };
        let text_left = rect.left()
            + if icon.is_some() {
                TOP_DROPDOWN_ICON_TEXT_LEFT
            } else {
                10.0
            };
        if let Some(icon) = icon {
            let icon_color = if !enabled {
                app_muted_text(dark)
            } else if selected {
                app_accent()
            } else {
                block_tint.expect("icon row has a block tint")
            };
            paint_top_menu_icon(
                ui,
                egui::pos2(
                    rect.left() + 8.0 + TOP_DROPDOWN_ICON_COLUMN * 0.5,
                    rect.center().y,
                ),
                icon,
                icon_color,
            );
        }
        ui.painter().with_clip_rect(text_clip).text(
            egui::pos2(text_left, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(13.0),
            text_color,
        );

        if accessory == TopDropdownItemAccessory::Indicator {
            let label_width = top_menu_text_width(ui, label, 13.0);
            let max_dot_x = rect.right() - if selected { 28.0 } else { 10.0 };
            let dot_x = (text_left + label_width + 8.0).min(max_dot_x);
            ui.painter()
                .circle_filled(egui::pos2(dot_x, rect.center().y), 2.5, app_accent());
        }

        if accessory == TopDropdownItemAccessory::Submenu {
            ui.painter().text(
                egui::pos2(rect.right() - 10.0, rect.center().y - 1.0),
                egui::Align2::RIGHT_CENTER,
                "›",
                egui::FontId::proportional(18.0),
                if !enabled {
                    app_muted_text(dark)
                } else if selected || hovered {
                    app_accent()
                } else {
                    ui.visuals().text_color()
                },
            );
        } else if selected {
            ui.painter().circle_filled(
                egui::pos2(rect.right() - 12.0, rect.center().y),
                2.5,
                app_accent(),
            );
        }
    }

    resp
}

pub(super) fn top_menu_text_width(ui: &egui::Ui, label: &str, font_size: f32) -> f32 {
    ui.fonts_mut(|f| {
        f.layout_no_wrap(
            label.to_owned(),
            egui::FontId::proportional(font_size),
            ui.visuals().widgets.inactive.fg_stroke.color,
        )
        .size()
        .x
    })
}

fn top_menu_icon_family(ui: &egui::Ui) -> egui::FontFamily {
    let family = egui::FontFamily::Name(TOP_MENU_ICON_FAMILY.into());
    // egui panics on an unregistered family, so fall back to the UI font.
    if ui.fonts(|fonts| fonts.definitions().families.contains_key(&family)) {
        family
    } else {
        egui::FontFamily::Proportional
    }
}

// Glyphs come from fonts with different metrics. Size and position follow
// the drawn shape, not the font line box, so every icon gets the same
// optical size and sits on the row center.
fn paint_top_menu_icon(ui: &egui::Ui, center: egui::Pos2, icon: TopMenuIcon, color: Color32) {
    const BASE_FONT_SIZE: f32 = 15.0;
    const TARGET_EXTENT: f32 = 16.0;

    let family = top_menu_icon_family(ui);
    let layout = |size: f32| {
        ui.painter().layout_no_wrap(
            icon.glyph().to_owned(),
            egui::FontId::new(size, family.clone()),
            color,
        )
    };
    let extent =
        |galley: &egui::Galley| galley.mesh_bounds.width().max(galley.mesh_bounds.height());

    let mut galley = layout(BASE_FONT_SIZE);
    let base_extent = extent(&galley);
    if base_extent > 0.0 {
        let scale = (TARGET_EXTENT / base_extent).clamp(0.8, 1.5);
        if (scale - 1.0).abs() > 0.05 {
            galley = layout(BASE_FONT_SIZE * scale);
        }
    }
    let ink_center = galley.mesh_bounds.center();
    ui.painter()
        .galley(center - ink_center.to_vec2(), galley, color);
}

pub(super) fn top_menu_divider_stroke(dark: bool) -> egui::Stroke {
    let color = if dark {
        Color32::from_gray(105)
    } else {
        Color32::from_gray(170)
    };
    egui::Stroke::new(1.5_f32, color)
}

pub(super) fn adaptive_top_dropdown_width<'a>(
    ui: &egui::Ui,
    labels: impl IntoIterator<Item = &'a str>,
    min_width: f32,
) -> f32 {
    let text_width = labels
        .into_iter()
        .filter(|label| !label.is_empty())
        .map(|label| top_menu_text_width(ui, label, 13.0))
        .fold(0.0, f32::max);

    // 16px frame margins + 10px left text inset + selected-dot reserve + breathing room.
    (text_width + 56.0).max(min_width).min(360.0)
}

pub(super) fn adaptive_top_icon_dropdown_width<'a>(
    ui: &egui::Ui,
    labels: impl IntoIterator<Item = &'a str>,
    min_width: f32,
) -> f32 {
    adaptive_top_dropdown_width(ui, labels, min_width) + TOP_DROPDOWN_ICON_COLUMN
}

impl EntropyApp {
    pub(super) fn close_top_dropdowns(&self, ctx: &egui::Context) {
        ctx.data_mut(|d| {
            d.insert_temp(device_dropdown_open_id(), false);
            d.insert_temp(device_layer_operations_submenu_open_id(), false);
            d.insert_temp(advanced_dropdown_open_id(), false);
            d.insert_temp(settings_dropdown_open_id(), false);
        });
    }

    pub(super) fn top_dropdown_open(&self, ctx: &egui::Context) -> bool {
        ctx.data(|d| {
            d.get_temp::<bool>(device_dropdown_open_id())
                .unwrap_or(false)
                || d.get_temp::<bool>(advanced_dropdown_open_id())
                    .unwrap_or(false)
                || d.get_temp::<bool>(settings_dropdown_open_id())
                    .unwrap_or(false)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders a popup with `rows` rows, the first `dividers` of them
    /// followed by a divider, and returns its rect.
    fn rendered_popup_rect(ctx: &egui::Context, rows: usize, dividers: usize) -> egui::Rect {
        let mut rect = egui::Rect::NOTHING;
        // Two frames: an area sizes itself on its first one.
        for _ in 0..2 {
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                let popup = show_top_dropdown(
                    ui.ctx(),
                    egui::Id::new(("test_dropdown", rows, dividers)),
                    egui::Pos2::ZERO,
                    |ui| {
                        for row in 0..rows {
                            top_dropdown_icon_item(
                                ui,
                                160.0,
                                TopMenuIcon::Device,
                                "Row",
                                true,
                                false,
                            );
                            if row < dividers {
                                top_dropdown_divider(ui, 160.0);
                            }
                        }
                    },
                );
                rect = popup.response.rect;
            });
        }
        rect
    }

    #[test]
    fn dropdown_height_matches_the_rendered_popup() {
        let ctx = egui::Context::default();
        for (rows, dividers) in [(1, 0), (2, 1), (7, 1), (16, 4)] {
            let rendered = rendered_popup_rect(&ctx, rows, dividers).height();
            let predicted = top_dropdown_height(rows, dividers);
            assert!(
                (rendered - predicted).abs() < 0.01,
                "{rows} rows, {dividers} dividers: rendered {rendered}, predicted {predicted}"
            );
        }
    }

    #[test]
    fn dropdown_rows_expose_button_semantics() {
        use egui::accesskit::{Role, Toggled};

        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            top_dropdown_icon_item(ui, 160.0, TopMenuIcon::Rgb, "Lighting", true, true);
            top_dropdown_icon_item(
                ui,
                160.0,
                TopMenuIcon::Lock,
                "Unlock keyboard",
                false,
                false,
            );
        });
        let update = output
            .platform_output
            .accesskit_update
            .expect("accesskit tree is emitted");
        let node = |label: &str| {
            update
                .nodes
                .iter()
                .map(|(_, node)| node)
                .find(|node| node.label() == Some(label))
                .unwrap_or_else(|| panic!("no accessibility node labeled {label:?}"))
        };

        let selected = node("Lighting");
        assert_eq!(selected.role(), Role::Button);
        assert!(!selected.is_disabled());
        assert_eq!(selected.toggled(), Some(Toggled::True));

        let disabled = node("Unlock keyboard");
        assert_eq!(disabled.role(), Role::Button);
        assert!(disabled.is_disabled());
        assert_eq!(disabled.toggled(), Some(Toggled::False));
    }

    #[test]
    fn coalesced_config_groups_share_one_tint_until_a_divider() {
        let dividers = top_menu_dividers([1, 2, 1, 2, 2]);
        assert_eq!(dividers, [false, false, true, true, false]);
        for dark in [false, true] {
            let ctx = egui::Context::default();
            ctx.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                // Selected/disabled opening rows still choose the block hue.
                top_dropdown_icon_item(ui, 160.0, TopMenuIcon::Rgb, "RGB", true, true);
                for icon in [
                    TopMenuIcon::Encoders,
                    TopMenuIcon::Touchpad,
                    TopMenuIcon::Magic,
                ] {
                    top_dropdown_icon_item(ui, 160.0, icon, icon.glyph(), true, false);
                    assert_eq!(
                        top_menu_block_tint(ui, icon, dark),
                        TopMenuGroup::Lighting.tint(dark)
                    );
                }
                top_dropdown_divider(ui, 160.0);
                top_dropdown_icon_item(
                    ui,
                    160.0,
                    TopMenuIcon::MatrixTester,
                    "Matrix",
                    false,
                    false,
                );
                assert_eq!(
                    top_menu_block_tint(ui, TopMenuIcon::Lock, dark),
                    TopMenuGroup::Service.tint(dark)
                );
                top_dropdown_divider(ui, 160.0);
                top_dropdown_icon_item(ui, 160.0, TopMenuIcon::AppSettings, "App", true, false);
                assert_eq!(
                    top_menu_block_tint(ui, TopMenuIcon::AboutEntropy, dark),
                    TopMenuGroup::Meta.tint(dark)
                );
            });
        }
    }

    #[test]
    fn leading_singleton_sets_the_joined_block_tint() {
        let ctx = egui::Context::default();
        assert_eq!(
            top_menu_dividers([0, 0, 1, 2, 2]),
            [false, false, false, true, false]
        );
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            top_dropdown_icon_item(ui, 160.0, TopMenuIcon::Magic, "Magic", true, false);
            top_dropdown_icon_item(ui, 160.0, TopMenuIcon::MatrixTester, "Matrix", true, false);
            assert_eq!(
                top_menu_block_tint(ui, TopMenuIcon::Lock, false),
                TopMenuGroup::KeyBehavior.tint(false)
            );
            top_dropdown_divider(ui, 160.0);
            assert_eq!(
                top_menu_block_tint(ui, TopMenuIcon::AppSettings, false),
                TopMenuGroup::Meta.tint(false)
            );
        });
    }

    #[test]
    fn popup_resets_tint_between_frames() {
        let ctx = egui::Context::default();
        for first in [TopMenuIcon::Rgb, TopMenuIcon::TextExpander] {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                show_top_dropdown(
                    ctx,
                    egui::Id::new("tint_popup"),
                    egui::pos2(8.0, 8.0),
                    |ui| {
                        top_dropdown_icon_item(ui, 160.0, first, "First", true, false);
                        assert_eq!(
                            top_menu_block_tint(ui, TopMenuIcon::Lock, false),
                            first.group().tint(false)
                        );
                    },
                );
            });
        }
    }

    #[test]
    fn full_menu_separates_every_group() {
        assert_eq!(
            top_menu_dividers([4, 5, 2, 2, 2]),
            [true, true, true, true, false]
        );
    }

    #[test]
    fn single_row_group_joins_the_previous_block() {
        // Layer LEDs | Modules, Bluetooth | Tap-Hold | Matrix, Unlock | App, About
        assert_eq!(
            top_menu_dividers([1, 2, 1, 2, 2]),
            [false, false, true, true, false]
        );
        assert_eq!(
            top_menu_dividers([2, 1, 0, 0, 2]),
            [false, true, false, false, false]
        );
    }

    #[test]
    fn leading_single_row_group_joins_the_next_block() {
        assert_eq!(
            top_menu_dividers([0, 0, 1, 2, 2]),
            [false, false, false, true, false]
        );
    }

    #[test]
    fn short_menu_has_no_dividers() {
        assert_eq!(top_menu_dividers([0, 0, 0, 0, 2]), [false; 5]);
        assert_eq!(top_menu_dividers([0, 0, 0, 1, 2]), [false; 5]);
        assert_eq!(top_menu_dividers([2, 0]), [false; 2]);
        assert_eq!(top_menu_dividers([2, 1]), [false; 2]);
    }

    #[test]
    fn shared_dropdown_state_is_visible_to_background_lifecycle() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let app = EntropyApp::new(&creation_context);

        ctx.data_mut(|d| d.insert_temp(device_dropdown_open_id(), true));
        assert!(app.top_dropdown_open(&ctx));

        app.close_top_dropdowns(&ctx);
        assert!(!app.top_dropdown_open(&ctx));
    }
}
