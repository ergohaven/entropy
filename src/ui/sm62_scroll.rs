// Keep the existing preview beside a standard settings list. The same
// 18-point right gutter (12 gap + 6 track) is used by other settings viewports.
const SM62_DISPLAY_PREVIEW_WIDTH: f32 = 245.0;
const SM62_DISPLAY_PANEL_GAP: f32 = 20.0;
fn sm62_display_scroll<R>(
    ui: &mut egui::Ui,
    body: egui::Rect,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
    let gutter = metrics.settings_content_width() - metrics.settings_row_content_width();
    let width = body.width().min(
        SM62_DISPLAY_PREVIEW_WIDTH + SM62_DISPLAY_PANEL_GAP + metrics.settings_content_width(),
    );
    let column =
        egui::Rect::from_center_size(body.center(), egui::vec2(width, body.height().max(1.0)));
    let track = egui::Rect::from_min_max(
        egui::pos2(column.right() - metrics.value(6.0), column.top()),
        column.max,
    );
    let viewport = egui::Rect::from_min_max(
        column.min,
        egui::pos2(
            (column.right() - gutter).max(column.left() + 1.0),
            column.bottom(),
        ),
    );
    let pending_offset =
        ui.data_mut(|data| data.remove_temp::<f32>(sm62_display_scroll_offset_id()));
    let output = crate::ui_style::allocate_ui_at_rect(ui, viewport, |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            let area = egui::ScrollArea::vertical()
                .id_salt("sm62_display_settings")
                .auto_shrink([false, false])
                .max_height(viewport.height())
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
            match pending_offset {
                Some(offset) => area.vertical_scroll_offset(offset),
                None => area,
            }
            .show(ui, |ui| {
                ui.set_min_width(viewport.width());
                add(ui)
            })
        })
        .inner
    })
    .inner;
    let max_offset = (output.content_size.y - output.inner_rect.height()).max(0.0);
    let mut offset = output.state.clamped_offset().y;
    if max_offset > 0.5 {
        let height = (track.height() * output.inner_rect.height() / output.content_size.y)
            .clamp(24.0_f32.min(track.height()), track.height());
        let response = ui.interact(
            track,
            output.id.with("floating_track"),
            egui::Sense::click_and_drag(),
        );
        if response.clicked() || response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                offset = ((pos.y - track.top() - height / 2.0)
                    / (track.height() - height).max(1.0))
                .clamp(0.0, 1.0)
                    * max_offset;
                scroll_sm62_display_to(ui.ctx(), offset);
                ui.ctx().request_repaint();
            }
        } else if response.hovered() {
            let delta = ui.input(|input| input.smooth_scroll_delta.y);
            if delta != 0.0 {
                offset = (offset - delta).clamp(0.0, max_offset);
                scroll_sm62_display_to(ui.ctx(), offset);
                ui.ctx().request_repaint();
            }
        }
        crate::ui_style::paint_floating_scrollbar_handle(
            ui,
            track,
            height,
            offset / max_offset,
            response.hovered() || response.dragged(),
        );
    }
    #[cfg(test)]
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            egui::Id::unique("sm62_scroll_fixture"),
            (output.id, max_offset),
        )
    });
    output
}

fn sm62_display_scroll_offset_id() -> egui::Id {
    egui::Id::unique("sm62_display_settings_offset")
}

// egui keeps the scroll offset to itself and ScrollArea takes a new one as it
// shows, so the SM62 settings scroll there on their next pass.
fn scroll_sm62_display_to(ctx: &egui::Context, offset: f32) {
    ctx.data_mut(|data| data.insert_temp(sm62_display_scroll_offset_id(), offset));
}
