use super::*;

impl EntropyApp {
    pub(super) fn start_onboarding_tour(&mut self, ctx: &egui::Context) {
        self.close_top_dropdowns(ctx);
        self.keycode_picker.open = false;
        if self.layout.is_some() {
            self.main_menu_tab = MainMenuTab::Keyboard;
        }
        self.tour_state.active = true;
        self.tour_state.step = 0;
        ctx.request_repaint();
    }

    fn complete_onboarding_tour(&mut self) {
        self.tour_state.active = false;
        self.tour_state.step = 0;
        self.app_settings.onboarding_tour_seen_version = ONBOARDING_TOUR_VERSION;
        save_app_settings(&self.app_settings);
    }

    pub(super) fn maybe_start_onboarding_tour(&mut self, ctx: &egui::Context) {
        if self.tour_state.active
            || self.app_settings.onboarding_tour_seen_version >= ONBOARDING_TOUR_VERSION
            || self.layout.is_none()
            || self.unlock_open
            || self.vial_unlock_polling
            || self.keycode_picker.open
        {
            return;
        }
        self.start_onboarding_tour(ctx);
    }

    pub(super) fn register_tour_target(&mut self, target: TourTarget, rect: egui::Rect) {
        if rect.is_positive() {
            self.tour_target_rects.retain(|(t, _)| *t != target);
            self.tour_target_rects.push((target, rect));
        }
    }

    fn tour_target_rect(&self, target: TourTarget) -> Option<egui::Rect> {
        self.tour_target_rects
            .iter()
            .find_map(|(registered, rect)| (*registered == target).then_some(*rect))
    }

    pub(super) fn draw_onboarding_tour(&mut self, ctx: &egui::Context) {
        if !self.tour_state.active || self.unlock_open || self.vial_unlock_polling {
            return;
        }
        if self.keycode_picker.open {
            return;
        }

        let step_count = ONBOARDING_TOUR_STEPS.len();
        if self.tour_state.step >= step_count {
            self.complete_onboarding_tour();
            return;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.complete_onboarding_tour();
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::Enter)) {
            if self.tour_state.step + 1 >= step_count {
                self.complete_onboarding_tour();
            } else {
                self.tour_state.step += 1;
            }
            ctx.request_repaint();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) && self.tour_state.step > 0 {
            self.tour_state.step -= 1;
            ctx.request_repaint();
        }

        let step = ONBOARDING_TOUR_STEPS[self.tour_state.step];
        let target_rect = step.target.and_then(|target| self.tour_target_rect(target));
        let lang = self.app_settings.language;
        let title = crate::i18n::tr_catalog(lang, step.title_key).to_owned();
        let body = crate::i18n::tr_catalog(lang, step.body_key).to_owned();
        let progress = crate::i18n::tr_catalog_format(
            lang,
            "onboarding_tour.progress",
            &[
                ("current", &(self.tour_state.step + 1).to_string()),
                ("total", &step_count.to_string()),
            ],
        );
        let prev_label = crate::i18n::tr_catalog(lang, "onboarding_tour.previous").to_owned();
        let next_label = if self.tour_state.step + 1 >= step_count {
            crate::i18n::tr_catalog(lang, "onboarding_tour.done").to_owned()
        } else {
            crate::i18n::tr_catalog(lang, "onboarding_tour.next").to_owned()
        };
        let skip_label = crate::i18n::tr_catalog(lang, "onboarding_tour.skip").to_owned();
        let sample_hint = crate::i18n::tr_catalog(lang, "onboarding_tour.sample_hint").to_owned();

        egui::Area::new(egui::Id::new("onboarding_tour_overlay"))
            .order(egui::Order::Tooltip)
            .fixed_pos(ctx.content_rect().min)
            .show(ctx, |ui| {
                let screen = ctx.content_rect();
                let local_screen = egui::Rect::from_min_size(egui::Pos2::ZERO, screen.size());
                ui.set_min_size(screen.size());
                let _block = ui.interact(
                    local_screen,
                    ui.make_persistent_id("onboarding_tour_blocker"),
                    Sense::click(),
                );
                let dark = ui.visuals().dark_mode;
                let painter = ui.painter();
                painter.rect_filled(
                    local_screen,
                    0.0,
                    Color32::from_black_alpha(if dark { 176 } else { 128 }),
                );

                let local_target = target_rect.map(|rect| rect.translate(-screen.min.to_vec2()));
                if let Some(rect) = local_target {
                    let highlight = rect.expand(8.0);
                    painter.rect(
                        highlight,
                        15.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 22),
                        Stroke::new(2.0_f32, app_accent()),
                        egui::StrokeKind::Inside,
                    );
                    if matches!(step.target, Some(TourTarget::BottomHints)) {
                        painter.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            sample_hint.as_str(),
                            FontId::proportional(12.0),
                            Color32::from_rgb(245, 245, 245),
                        );
                    }
                }

                let card_w = 382.0_f32.min(local_screen.width() - 32.0).max(280.0);
                let card_h = 244.0_f32;
                let margin = 18.0;
                let card_x = local_target
                    .map(|rect| rect.center().x - card_w / 2.0)
                    .unwrap_or_else(|| local_screen.center().x - card_w / 2.0)
                    .clamp(
                        local_screen.left() + margin,
                        local_screen.right() - card_w - margin,
                    );
                let card_y = if let Some(rect) = local_target {
                    let below = rect.bottom() + 18.0;
                    let above = rect.top() - card_h - 18.0;
                    if below + card_h <= local_screen.bottom() - margin {
                        below
                    } else if above >= local_screen.top() + margin {
                        above
                    } else {
                        local_screen.center().y - card_h / 2.0
                    }
                } else {
                    local_screen.center().y - card_h / 2.0
                };
                let card_rect = egui::Rect::from_min_size(
                    egui::pos2(card_x, card_y),
                    Vec2::new(card_w, card_h),
                );

                painter.rect(
                    card_rect,
                    18.0,
                    app_window_fill(dark),
                    Stroke::new(1.0_f32, app_border_color(dark)),
                    egui::StrokeKind::Inside,
                );
                crate::ui_style::allocate_ui_at_rect(
                    ui,
                    card_rect.shrink2(Vec2::new(18.0, 16.0)),
                    |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(progress.as_str())
                                        .size(11.5)
                                        .color(app_muted_text(dark)),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let close = ui.add(
                                            egui::Label::new(
                                                RichText::new("×")
                                                    .size(18.0)
                                                    .color(app_muted_text(dark)),
                                            )
                                            .selectable(false)
                                            .sense(Sense::click()),
                                        );
                                        if close.hovered() {
                                            ui.ctx()
                                                .set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                        if close.clicked() {
                                            self.complete_onboarding_tour();
                                        }
                                    },
                                );
                            });
                            ui.add_space(10.0);
                            ui.label(RichText::new(title.as_str()).size(20.0).strong());
                            ui.add_space(8.0);
                            ui.add(
                                egui::Label::new(RichText::new(body.as_str()).size(13.5).color(
                                    if dark {
                                        Color32::from_gray(205)
                                    } else {
                                        Color32::from_gray(66)
                                    },
                                ))
                                .wrap(),
                            );
                            ui.add_space(10.0);
                            let button_row_height = 32.0;
                            let spacer_height =
                                (ui.available_height() - button_row_height).max(0.0);
                            ui.add_space(spacer_height);
                            ui.horizontal(|ui| {
                                if crate::ui_style::modern_button(
                                    ui,
                                    skip_label.as_str(),
                                    Vec2::new(96.0, button_row_height),
                                    true,
                                )
                                .clicked()
                                {
                                    self.complete_onboarding_tour();
                                }

                                let trailing_width = 88.0 + 94.0 + ui.spacing().item_spacing.x;
                                ui.add_space((ui.available_width() - trailing_width).max(0.0));

                                let prev_enabled = self.tour_state.step > 0;
                                if crate::ui_style::modern_button(
                                    ui,
                                    prev_label.as_str(),
                                    Vec2::new(88.0, button_row_height),
                                    prev_enabled,
                                )
                                .clicked()
                                    && prev_enabled
                                {
                                    self.tour_state.step -= 1;
                                    ctx.request_repaint();
                                }
                                if crate::ui_style::modern_button(
                                    ui,
                                    next_label.as_str(),
                                    Vec2::new(94.0, button_row_height),
                                    true,
                                )
                                .clicked()
                                {
                                    if self.tour_state.step + 1 >= step_count {
                                        self.complete_onboarding_tour();
                                    } else {
                                        self.tour_state.step += 1;
                                        ctx.request_repaint();
                                    }
                                }
                            });
                        });
                    },
                );
            });
    }
}
