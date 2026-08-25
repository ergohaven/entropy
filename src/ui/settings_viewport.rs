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
    if total_rows == 0 {
        return 1;
    }

    let row_height = crate::ui_style::ResponsiveMetrics::from_ctx(ctx).settings_row_height();
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
    let viewport_width = metrics.settings_content_width();
    let row_content_width = metrics.settings_row_content_width();
    let row_height = metrics.settings_row_height();
    let visible_rows = responsive_settings_visible_rows(
        ui.ctx(),
        ui.available_height(),
        total_rows,
        bottom_reserve,
    );
    let list_height = row_height * visible_rows as f32;
    let content_height = row_height * total_rows as f32;
    let max_offset = (content_height - list_height).max(0.0);
    let offset_id = ui.id().with((id_salt, "smooth_offset"));
    let target_id = ui.id().with((id_salt, "smooth_target"));
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
            ui.id().with((id_salt, "scrollbar")),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
