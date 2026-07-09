use super::*;

const TYPING_TRAINER_VISIBLE_LINES: usize = 4;

impl EntropyApp {
    pub(super) fn pause_typing_trainer_if_inactive(&mut self, now: std::time::Instant) {
        if self.main_menu_tab != MainMenuTab::Advanced
            || self.settings_tab != SettingsTab::TypingTrainer
        {
            self.typing_trainer.pause_if_running(now);
        }
    }

    pub(super) fn draw_typing_trainer_page(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let now = std::time::Instant::now();
        let remaining_secs = self.typing_trainer.remaining_secs_at(now);
        if self.typing_trainer.is_finished() {
            self.typing_trainer.ui_hidden = false;
        }
        let ui_hidden = self.typing_trainer.ui_hidden;
        if self.typing_trainer.started_at.is_some()
            && !self.typing_trainer.is_paused()
            && !self.typing_trainer.is_finished()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                if ui_hidden {
                    self.reserve_typing_trainer_chrome_space(ui, metrics);
                } else {
                    ui.add_space(metrics.value(18.0));
                    ui.label(
                        RichText::new(crate::i18n::tr_catalog(lang, "typing_trainer.title"))
                            .size(metrics.value(18.0))
                            .strong(),
                    );
                    ui.add_space(metrics.value(6.0));
                    ui.label(
                        RichText::new(crate::i18n::tr_catalog(lang, "typing_trainer.description"))
                            .size(metrics.value(13.0))
                            .color(app_muted_text(dark)),
                    );
                    ui.add_space(metrics.value(18.0));

                    self.draw_typing_trainer_controls(ui, metrics, lang);
                    ui.add_space(metrics.value(18.0));
                    self.draw_typing_trainer_stats(ui, metrics, lang, now, remaining_secs);
                    ui.add_space(metrics.value(24.0));
                }
                self.draw_typing_trainer_text(ui, metrics, dark);
                if self.typing_trainer.is_finished() {
                    ui.add_space(metrics.value(10.0));
                    self.draw_typing_trainer_finished_status(ui, metrics, lang);
                }
            });
        });
    }

    pub(super) fn handle_typing_trainer_input(&mut self, ctx: &egui::Context) {
        if self.main_menu_tab != MainMenuTab::Advanced
            || self.settings_tab != SettingsTab::TypingTrainer
            || self.keycode_picker.open
            || self.unlock_open
            || ctx.memory(|m| m.any_popup_open())
        {
            return;
        }

        let now = std::time::Instant::now();
        let command_modifier_down = ctx
            .input(|input| input.modifiers.command || input.modifiers.ctrl || input.modifiers.alt);
        let events = ctx.input(|input| input.events.clone());
        let mut typed_this_frame = false;
        let mut pointer_moved_this_frame = false;
        for event in events {
            match event {
                egui::Event::Text(text) if !command_modifier_down => {
                    for ch in text.chars() {
                        if typing_trainer_accepts_char(ch) && !self.typing_trainer.is_finished() {
                            typed_this_frame = true;
                        }
                        self.typing_trainer.type_char(ch, now);
                    }
                }
                egui::Event::Key {
                    key: egui::Key::Backspace,
                    pressed: true,
                    ..
                } => {
                    if !self.typing_trainer.is_finished() {
                        typed_this_frame = true;
                    }
                    self.typing_trainer.backspace();
                }
                egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    ..
                } => {
                    self.typing_trainer.reset();
                }
                egui::Event::PointerMoved(_) => {
                    pointer_moved_this_frame = true;
                }
                _ => {}
            }
        }
        if pointer_moved_this_frame || self.typing_trainer.is_finished() {
            self.typing_trainer.ui_hidden = false;
        } else if typed_this_frame {
            self.typing_trainer.ui_hidden = true;
        }
    }

    fn reserve_typing_trainer_chrome_space(
        &self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
    ) {
        let segment_size = metrics.size(244.0, 32.0);
        let restart_size = metrics.size(96.0, 32.0);
        let controls_width = segment_size.x + metrics.value(12.0) + restart_size.x;
        let stats_width = metrics.value(92.0) * 4.0;

        ui.add_space(metrics.value(18.0));
        ui.allocate_exact_size(
            egui::vec2(metrics.value(1.0), metrics.value(22.0)),
            Sense::hover(),
        );
        ui.add_space(metrics.value(6.0));
        ui.allocate_exact_size(
            egui::vec2(metrics.value(1.0), metrics.value(16.0)),
            Sense::hover(),
        );
        ui.add_space(metrics.value(18.0));
        ui.allocate_exact_size(egui::vec2(controls_width, segment_size.y), Sense::hover());
        ui.add_space(metrics.value(18.0));
        ui.allocate_exact_size(egui::vec2(stats_width, metrics.value(42.0)), Sense::hover());
        ui.add_space(metrics.value(24.0));
    }

    fn draw_typing_trainer_controls(
        &mut self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
    ) {
        let labels = TYPING_TRAINER_DURATIONS
            .iter()
            .map(|duration| duration.to_string())
            .collect::<Vec<_>>();
        let selected_duration = TYPING_TRAINER_DURATIONS
            .iter()
            .position(|duration| *duration == self.typing_trainer.duration_secs)
            .unwrap_or(1);
        let segment_size = metrics.size(244.0, 32.0);
        let restart_size = metrics.size(96.0, 32.0);
        let gap = metrics.value(12.0);
        let controls_width = segment_size.x + gap + restart_size.x;

        ui.allocate_ui_with_layout(
            egui::vec2(controls_width, segment_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                if let Some(picked) = crate::ui_style::settings_segmented_control(
                    ui,
                    "typing_trainer_duration",
                    &labels,
                    selected_duration,
                    segment_size,
                ) {
                    self.typing_trainer
                        .set_duration(TYPING_TRAINER_DURATIONS[picked]);
                }
                ui.add_space(gap);
                if crate::ui_style::modern_button(
                    ui,
                    crate::i18n::tr_catalog(lang, "typing_trainer.restart"),
                    restart_size,
                    true,
                )
                .clicked()
                {
                    self.typing_trainer.reset();
                }
            },
        );
    }

    fn draw_typing_trainer_stats(
        &self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        now: std::time::Instant,
        remaining_secs: u32,
    ) {
        let stats = self.typing_trainer.stats_at(now);
        let timer_secs = if self.typing_trainer.started_at.is_some() {
            remaining_secs
        } else {
            self.typing_trainer.duration_secs
        };
        let labels = [
            (
                crate::i18n::tr_catalog(lang, "typing_trainer.time"),
                timer_secs.to_string(),
            ),
            (
                crate::i18n::tr_catalog(lang, "typing_trainer.wpm"),
                stats.wpm.to_string(),
            ),
            (
                crate::i18n::tr_catalog(lang, "typing_trainer.accuracy"),
                format!("{:.0}%", stats.accuracy),
            ),
            (
                crate::i18n::tr_catalog(lang, "typing_trainer.errors"),
                stats.errors.to_string(),
            ),
        ];
        let dark = ui.visuals().dark_mode;
        let item_width = metrics.value(92.0);
        let stat_height = metrics.value(42.0);
        let total_width = item_width * labels.len() as f32;

        ui.allocate_ui_with_layout(
            egui::vec2(total_width, stat_height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                for (label, value) in labels {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(item_width, stat_height), Sense::hover());
                    let value_color = if self.typing_trainer.is_finished() {
                        app_accent()
                    } else {
                        ui.visuals().text_color()
                    };
                    ui.painter().text(
                        egui::pos2(rect.center().x, rect.top() + metrics.value(10.0)),
                        egui::Align2::CENTER_CENTER,
                        label,
                        FontId::proportional(metrics.value(11.0)),
                        app_muted_text(dark),
                    );
                    ui.painter().text(
                        egui::pos2(rect.center().x, rect.bottom() - metrics.value(11.0)),
                        egui::Align2::CENTER_CENTER,
                        value,
                        FontId::proportional(metrics.value(17.0)),
                        value_color,
                    );
                }
            },
        );
    }

    fn draw_typing_trainer_text(
        &mut self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        dark: bool,
    ) {
        let width = ui.available_width().min(metrics.value(860.0));
        let height = metrics.value(214.0);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }

        let text_rect = rect.shrink2(egui::vec2(metrics.value(8.0), 0.0));
        let font_id = FontId::new(metrics.value(27.0), egui::FontFamily::Monospace);
        let char_width = ui
            .painter()
            .layout_no_wrap("m".to_owned(), font_id.clone(), ui.visuals().text_color())
            .size()
            .x
            .max(metrics.value(11.0));
        let line_height = metrics.value(42.0);
        let top_padding = metrics.value(18.0);
        let max_line_chars = (text_rect.width() / char_width).floor().max(1.0) as usize;
        let max_visible_lines =
            (((text_rect.height() - top_padding).max(0.0) / line_height).floor() as usize + 1)
                .clamp(1, TYPING_TRAINER_VISIBLE_LINES);
        if !self.typing_trainer.is_finished() {
            self.ensure_typing_trainer_visible_text(max_line_chars, max_visible_lines);
        }

        let target_chars = self.typing_trainer.target_text.chars().collect::<Vec<_>>();
        let typed_chars = &self.typing_trainer.typed_chars;
        let mut x = text_rect.left();
        let mut y = text_rect.top() + top_padding;
        let caret_idx = typed_chars.len().min(target_chars.len());
        let mut idx = typing_trainer_visible_start_index(
            &target_chars,
            caret_idx,
            max_line_chars,
            max_visible_lines,
        );
        let mut caret_pos = None;
        let target_len = target_chars.len();
        let mut visible_lines = 1;

        while idx < target_len && visible_lines <= max_visible_lines && y <= text_rect.bottom() {
            let word_end = target_chars[idx..]
                .iter()
                .position(|ch| *ch == ' ')
                .map(|offset| idx + offset)
                .unwrap_or(target_len);
            let visible_word_len = word_end.saturating_sub(idx).max(1);
            let word_width = visible_word_len as f32 * char_width;
            if x > text_rect.left() && x + word_width > text_rect.right() {
                x = text_rect.left();
                y += line_height;
                visible_lines += 1;
                if visible_lines > max_visible_lines || y > text_rect.bottom() {
                    break;
                }
            }

            let draw_end = if word_end < target_len {
                word_end + 1
            } else {
                word_end
            };
            while idx < draw_end && y <= text_rect.bottom() {
                if idx == caret_idx {
                    caret_pos = Some(egui::pos2(x, y));
                }
                let target = target_chars[idx];
                let typed = typed_chars.get(idx).copied();
                let color = typing_trainer_char_color(ui, dark, target, typed);
                let glyph = if target == ' ' && typed.is_some_and(|ch| ch != ' ') {
                    "·".to_owned()
                } else {
                    target.to_string()
                };
                ui.painter().text(
                    egui::pos2(x, y),
                    egui::Align2::LEFT_CENTER,
                    glyph,
                    font_id.clone(),
                    color,
                );
                x += char_width;
                idx += 1;
            }
        }

        if caret_idx == target_len {
            caret_pos = Some(egui::pos2(x, y));
        }
        if !self.typing_trainer.is_finished()
            && ui.input(|input| ((input.time * 2.0) as i64) % 2 == 0)
        {
            if let Some(pos) = caret_pos {
                ui.painter().line_segment(
                    [
                        egui::pos2(pos.x, pos.y - metrics.value(16.0)),
                        egui::pos2(pos.x, pos.y + metrics.value(16.0)),
                    ],
                    Stroke::new(metrics.value(1.6), app_accent()),
                );
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
    }

    fn ensure_typing_trainer_visible_text(
        &mut self,
        max_line_chars: usize,
        max_visible_lines: usize,
    ) {
        for _ in 0..4 {
            let target_chars = self.typing_trainer.target_text.chars().collect::<Vec<_>>();
            let caret_idx = self
                .typing_trainer
                .typed_chars
                .len()
                .min(target_chars.len());
            let line_starts = typing_trainer_line_starts(&target_chars, max_line_chars);
            let first_visible_line =
                typing_trainer_first_visible_line(&line_starts, caret_idx, max_visible_lines);
            if line_starts.len() >= first_visible_line + max_visible_lines.max(1) {
                break;
            }
            self.typing_trainer.extend_target_text();
        }
    }

    fn draw_typing_trainer_finished_status(
        &self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
    ) {
        let size = metrics.size(120.0, 32.0);
        ui.allocate_ui_with_layout(
            size,
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                let _ = crate::ui_style::modern_button(
                    ui,
                    crate::i18n::tr_catalog(lang, "typing_trainer.finished"),
                    size,
                    false,
                );
            },
        );
    }
}

fn typing_trainer_visible_start_index(
    target_chars: &[char],
    caret_idx: usize,
    max_line_chars: usize,
    max_visible_lines: usize,
) -> usize {
    let line_starts = typing_trainer_line_starts(target_chars, max_line_chars);
    let first_visible_line =
        typing_trainer_first_visible_line(&line_starts, caret_idx, max_visible_lines);
    line_starts.get(first_visible_line).copied().unwrap_or(0)
}

fn typing_trainer_first_visible_line(
    line_starts: &[usize],
    caret_idx: usize,
    max_visible_lines: usize,
) -> usize {
    let caret_line = line_starts
        .partition_point(|start| *start <= caret_idx)
        .saturating_sub(1);
    let max_visible_lines = max_visible_lines.max(1);
    caret_line / max_visible_lines * max_visible_lines
}

fn typing_trainer_line_starts(target_chars: &[char], max_line_chars: usize) -> Vec<usize> {
    let target_len = target_chars.len();
    if target_len == 0 {
        return vec![0];
    }

    let max_line_chars = max_line_chars.max(1);
    let mut line_starts = vec![0];
    let mut line_chars = 0;
    let mut idx = 0;
    while idx < target_len {
        let word_end = target_chars[idx..]
            .iter()
            .position(|ch| *ch == ' ')
            .map(|offset| idx + offset)
            .unwrap_or(target_len);
        let visible_word_len = word_end.saturating_sub(idx).max(1);
        if line_chars > 0 && line_chars + visible_word_len > max_line_chars {
            line_starts.push(idx);
            line_chars = 0;
        }

        let draw_end = if word_end < target_len {
            word_end + 1
        } else {
            word_end
        };
        line_chars += draw_end.saturating_sub(idx);
        idx = draw_end;
    }
    line_starts
}

fn typing_trainer_char_color(
    ui: &egui::Ui,
    dark: bool,
    target: char,
    typed: Option<char>,
) -> Color32 {
    match typed {
        Some(ch) if ch == target => ui.visuals().text_color(),
        Some(_) => {
            if dark {
                Color32::from_rgb(214, 106, 120)
            } else {
                Color32::from_rgb(184, 62, 76)
            }
        }
        None => app_muted_text(dark).gamma_multiply(0.72),
    }
}

#[cfg(test)]
mod typing_trainer_ui_tests {
    use super::*;

    #[test]
    fn typing_trainer_visible_start_stays_at_first_line_near_start() {
        let chars = "one two three four five six".chars().collect::<Vec<_>>();

        assert_eq!(typing_trainer_visible_start_index(&chars, 3, 9, 2), 0);
    }

    #[test]
    fn typing_trainer_visible_start_keeps_current_page_until_it_is_done() {
        let chars = "one two three four five six seven eight"
            .chars()
            .collect::<Vec<_>>();
        let three_idx = "one two ".chars().count();

        assert_eq!(
            typing_trainer_visible_start_index(&chars, three_idx, 9, 2),
            0
        );
    }

    #[test]
    fn typing_trainer_visible_start_jumps_by_full_pages() {
        let chars = "one two three four five six seven eight"
            .chars()
            .collect::<Vec<_>>();
        let seven_idx = "one two three four five six ".chars().count();

        assert_eq!(
            typing_trainer_visible_start_index(&chars, seven_idx, 9, 2),
            "one two three ".chars().count()
        );
    }
}
