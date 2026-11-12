use super::*;

pub(crate) fn responsive_settings_editor_scale(ctx: &egui::Context) -> f32 {
    crate::ui_style::ResponsiveMetrics::from_ctx(ctx).scale
}

pub(crate) fn responsive_settings_visible_rows(
    ctx: &egui::Context,
    available_height: f32,
    total_rows: usize,
    bottom_reserve: f32,
) -> usize {
    let row_height = crate::ui_style::ResponsiveMetrics::from_ctx(ctx).settings_row_height();
    visible_settings_rows_for_height(available_height, total_rows, bottom_reserve, row_height)
}

fn visible_settings_rows_for_height(
    available_height: f32,
    total_rows: usize,
    bottom_reserve: f32,
    row_height: f32,
) -> usize {
    if total_rows == 0 {
        return 1;
    }

    let usable_height = (available_height - bottom_reserve).max(row_height);
    let rows_that_fit = (usable_height / row_height).floor().max(1.0) as usize;
    rows_that_fit.min(total_rows)
}

pub(crate) struct AdaptiveSettingsListViewport {
    pub(crate) viewport: egui::Rect,
    pub(crate) content_rect: egui::Rect,
    pub(crate) track_rect: egui::Rect,
    pub(crate) handle_height: f32,
    pub(crate) scroll_ratio: f32,
    pub(crate) track_hovered: bool,
    pub(crate) suppress_tooltips: bool,
    pub(crate) first_visible_row: usize,
    pub(crate) last_visible_row: usize,
    pub(crate) row_content_width: f32,
    pub(crate) row_height: f32,
    pub(crate) has_scrollbar: bool,
    pub(crate) scroll_offset: f32,
    pub(crate) content_height: f32,
}

/// Returns a footer rectangle anchored to the settings viewport, not to its
/// scrolling content. Page-level actions rendered here therefore stay centered
/// while the list scrolls or its scrollbar appears.
pub(crate) fn fixed_settings_action_bar_rect(
    viewport: egui::Rect,
    metrics: crate::ui_style::ResponsiveMetrics,
    button_size: egui::Vec2,
    button_count: usize,
    gap: f32,
) -> egui::Rect {
    let width = button_size.x * button_count as f32 + gap * button_count.saturating_sub(1) as f32;
    egui::Rect::from_center_size(
        egui::pos2(viewport.center().x, viewport.bottom() + metrics.value(26.0)),
        egui::vec2(width, button_size.y),
    )
}

pub(crate) fn allocate_adaptive_settings_list_viewport(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    metrics: crate::ui_style::ResponsiveMetrics,
    total_rows: usize,
    bottom_reserve: f32,
) -> AdaptiveSettingsListViewport {
    allocate_adaptive_settings_list_viewport_with_row_height(
        ui,
        id_salt,
        metrics,
        total_rows,
        bottom_reserve,
        metrics.settings_row_height(),
    )
}

pub(crate) fn allocate_adaptive_settings_list_viewport_with_row_height(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    metrics: crate::ui_style::ResponsiveMetrics,
    total_rows: usize,
    bottom_reserve: f32,
    row_height: f32,
) -> AdaptiveSettingsListViewport {
    allocate_adaptive_settings_list_viewport_capped(
        ui,
        id_salt,
        metrics,
        total_rows,
        bottom_reserve,
        row_height,
        usize::MAX,
    )
}

pub(crate) fn allocate_adaptive_settings_list_viewport_capped(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    metrics: crate::ui_style::ResponsiveMetrics,
    total_rows: usize,
    bottom_reserve: f32,
    row_height: f32,
    max_visible_rows: usize,
) -> AdaptiveSettingsListViewport {
    allocate_settings_list_viewport(
        ui,
        id_salt,
        metrics,
        total_rows,
        bottom_reserve,
        row_height,
        max_visible_rows,
        metrics.settings_content_width(),
    )
}

fn allocate_settings_list_viewport(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    metrics: crate::ui_style::ResponsiveMetrics,
    total_rows: usize,
    bottom_reserve: f32,
    row_height: f32,
    max_visible_rows: usize,
    viewport_width: f32,
) -> AdaptiveSettingsListViewport {
    let row_content_width =
        viewport_width - (metrics.settings_content_width() - metrics.settings_row_content_width());
    let visible_rows = visible_settings_rows_for_height(
        ui.available_height(),
        total_rows,
        bottom_reserve,
        row_height,
    )
    .min(max_visible_rows.max(1));
    let list_height = row_height * visible_rows as f32;
    let content_height = row_height * total_rows as f32;
    let max_offset = (content_height - list_height).max(0.0);
    let offset_id = ui.scope_id().with((id_salt, "smooth_offset"));
    let target_id = ui.scope_id().with((id_salt, "smooth_target"));
    let mut scroll_offset = ui
        .ctx()
        .data_mut(|d| d.get_persisted::<f32>(offset_id).unwrap_or(0.0))
        .clamp(0.0, max_offset);
    let mut target_offset = ui
        .ctx()
        .data_mut(|d| d.get_persisted::<f32>(target_id).unwrap_or(scroll_offset))
        .clamp(0.0, max_offset);
    let (viewport, _) =
        ui.allocate_exact_size(egui::vec2(viewport_width, list_height), Sense::hover());

    let track_width = metrics.value(6.0);
    let track_rect = egui::Rect::from_min_max(
        egui::pos2(viewport.right() - track_width, viewport.top()),
        egui::pos2(viewport.right(), viewport.bottom()),
    );
    let scrollbar_resp = if max_offset > 0.0 {
        Some(ui.interact(
            track_rect.expand2(egui::vec2(metrics.value(5.0), 0.0)),
            ui.scope_id().with((id_salt, "scrollbar")),
            Sense::click_and_drag(),
        ))
    } else {
        None
    };

    let mut scroll_active = false;
    let popup_open = egui::Popup::is_any_open(ui.ctx());
    let viewport_hovered = !popup_open
        && ui.input(|i| {
            i.pointer
                .hover_pos()
                .is_some_and(|pos| viewport.contains(pos))
        });
    let scroll_delta = if viewport_hovered {
        ui.input(|i| i.smooth_scroll_delta.y)
    } else {
        0.0
    };
    if scroll_delta.abs() > 0.0 && max_offset > 0.0 {
        scroll_active = true;
        target_offset = (target_offset - scroll_delta * 0.72).clamp(0.0, max_offset);
    }

    let handle_height = if max_offset > 0.0 {
        (list_height / content_height * viewport.height())
            .clamp(metrics.value(42.0), viewport.height())
    } else {
        viewport.height()
    };
    if let Some(resp) = &scrollbar_resp {
        if (resp.dragged() || resp.clicked()) && max_offset > 0.0 {
            if let Some(pointer_pos) = ui.input(|i| i.pointer.interact_pos()) {
                scroll_active = true;
                let travel = (track_rect.height() - handle_height).max(1.0);
                let t = ((pointer_pos.y - track_rect.top() - handle_height / 2.0) / travel)
                    .clamp(0.0, 1.0);
                target_offset = t * max_offset;
                scroll_offset = target_offset;
            }
        }
    }

    if (scroll_offset - target_offset).abs() > 0.35 {
        scroll_offset += (target_offset - scroll_offset) * 0.42;
        scroll_active = true;
        ui.ctx().request_repaint();
    } else {
        scroll_offset = target_offset;
    }
    scroll_offset = scroll_offset.clamp(0.0, max_offset);
    target_offset = target_offset.clamp(0.0, max_offset);
    ui.ctx().data_mut(|d| {
        d.insert_persisted(offset_id, scroll_offset);
        d.insert_persisted(target_id, target_offset);
    });

    let first_visible_row = (scroll_offset / row_height).floor() as usize;
    let row_y_offset = scroll_offset - first_visible_row as f32 * row_height;
    let last_visible_row = (first_visible_row + visible_rows + 1).min(total_rows);
    let visible_row_count = last_visible_row.saturating_sub(first_visible_row);
    let content_rect = egui::Rect::from_min_size(
        egui::pos2(viewport.left(), viewport.top() - row_y_offset),
        egui::vec2(row_content_width, row_height * visible_row_count as f32),
    );
    let track_hovered = scrollbar_resp
        .as_ref()
        .map(|resp| resp.hovered() || resp.dragged())
        .unwrap_or(false);

    AdaptiveSettingsListViewport {
        viewport,
        content_rect,
        track_rect,
        handle_height,
        scroll_ratio: if max_offset > 0.0 {
            scroll_offset / max_offset
        } else {
            0.0
        },
        track_hovered,
        suppress_tooltips: scroll_active || ui.input(|i| i.pointer.primary_down()),
        first_visible_row,
        last_visible_row,
        row_content_width,
        row_height,
        has_scrollbar: max_offset > 0.0,
        scroll_offset,
        content_height,
    }
}

/// A wide settings grid shares the list's vertical row virtualization, gutter,
/// scrollbar and tooltip policy. Horizontal scrolling stays inside the gutter.
pub(crate) fn show_adaptive_settings_grid(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    metrics: crate::ui_style::ResponsiveMetrics,
    total_rows: usize,
    minimum_content_width: f32,
    mut draw_row: impl FnMut(&mut egui::Ui, usize, f32, f32, bool),
) -> AdaptiveSettingsListViewport {
    let gutter = metrics.settings_content_width() - metrics.settings_row_content_width();
    let width = ui.available_width().min(minimum_content_width + gutter);
    let list = allocate_settings_list_viewport(
        ui,
        id_salt,
        metrics,
        total_rows,
        0.0,
        metrics.settings_row_height(),
        usize::MAX,
        width,
    );
    let content_viewport = egui::Rect::from_min_max(
        list.viewport.min,
        egui::pos2(
            list.viewport.left() + list.row_content_width,
            list.viewport.bottom(),
        ),
    );
    crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
        ui.set_clip_rect(content_viewport.intersect(ui.clip_rect()));
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::horizontal()
            .id_salt((id_salt, "columns"))
            .show(ui, |ui| {
                let width = minimum_content_width.max(list.row_content_width);
                ui.set_min_width(width);
                // Horizontal wheel gestures also suppress row tooltips.
                let suppress =
                    list.suppress_tooltips || ui.input(|i| i.smooth_scroll_delta.x.abs() > 0.0);
                for row in list.first_visible_row..list.last_visible_row {
                    draw_row(ui, row, width, list.row_height, suppress);
                }
            });
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
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_grid_retains_gutter_virtualization_and_tooltip_suppression() {
        let ctx = egui::Context::default();
        let mut first_row = 0;
        for frame in 0..12 {
            let scroll = frame > 0;
            let events = if scroll {
                vec![
                    egui::Event::PointerMoved(egui::pos2(200.0, 120.0)),
                    egui::Event::MouseWheel {
                        source: egui::MouseWheelSource::Unknown,
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -200.0),
                        modifiers: egui::Modifiers::NONE,
                        phase: egui::TouchPhase::Move,
                    },
                ]
            } else {
                vec![]
            };
            let mut rendered = Vec::new();
            let _ = crate::app::discard_texture_deltas(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 300.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                    let list = show_adaptive_settings_grid(
                        ui,
                        "grid_test",
                        metrics,
                        100,
                        1200.0,
                        |ui, row, width, height, suppress| {
                            rendered.push((row, suppress));
                            ui.allocate_space(egui::vec2(width, height));
                        },
                    );
                    assert!(list.has_scrollbar);
                    assert!(
                        (list.viewport.width() - list.row_content_width - metrics.value(18.0))
                            .abs()
                            < 0.01
                    );
                    assert!(rendered.len() < 10, "only visible rows should render");
                    if scroll {
                        assert!(list.scroll_offset > 0.0);
                        assert!(rendered.iter().all(|(_, suppressed)| *suppressed));
                    }
                    first_row = list.first_visible_row;
                },
            ));
        }
        assert!(first_row > 0, "scrolling must reach later rows");
    }

    #[test]
    fn settings_list_uses_all_rows_that_fit_large_windows() {
        let ctx = egui::Context::default();
        let row_height = crate::ui_style::ResponsiveMetrics::from_ctx(&ctx).settings_row_height();

        assert_eq!(
            responsive_settings_visible_rows(&ctx, row_height * 9.0, 8, row_height),
            8
        );
    }

    #[test]
    fn settings_list_still_scrolls_when_rows_do_not_fit() {
        let ctx = egui::Context::default();
        let row_height = crate::ui_style::ResponsiveMetrics::from_ctx(&ctx).settings_row_height();

        assert_eq!(
            responsive_settings_visible_rows(&ctx, row_height * 5.0, 8, row_height),
            4
        );
    }

    #[test]
    fn taller_keycap_rows_scroll_by_their_actual_height() {
        let ctx = egui::Context::default();
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(&ctx);
        let row_height = metrics.value(62.0);
        assert!(((row_height - metrics.value(54.0)) / 2.0 - metrics.value(4.0)).abs() < 0.001);
        assert_eq!(
            visible_settings_rows_for_height(row_height * 5.0 + 54.1, 7, 54.0, row_height),
            5
        );
        assert_eq!(
            visible_settings_rows_for_height(row_height * 7.0 + 54.1, 7, 54.0, row_height),
            7
        );
    }

    #[test]
    fn fixed_action_bar_stays_centered_on_the_viewport() {
        let ctx = egui::Context::default();
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(&ctx);
        let viewport = egui::Rect::from_min_size(egui::pos2(35.0, 80.0), egui::vec2(470.0, 216.0));
        let rect =
            fixed_settings_action_bar_rect(viewport, metrics, egui::vec2(110.0, 34.0), 2, 8.0);

        assert_eq!(rect.center().x, viewport.center().x);
        assert!(rect.top() > viewport.bottom());
    }
}
