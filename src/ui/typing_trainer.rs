use super::*;

const TYPING_TRAINER_VISIBLE_LINES: usize = 4;

impl EntropyApp {
    fn typing_trainer_focus_mode_active(&self) -> bool {
        self.typing_trainer.ui_hidden
            && !self.typing_trainer.is_finished()
            && !self.typing_trainer_history_open
    }

    pub(super) fn typing_trainer_chrome_opacity(&self, ctx: &egui::Context) -> f32 {
        let typing_trainer_page = self.main_menu_tab == MainMenuTab::Advanced
            && self.settings_tab == SettingsTab::TypingTrainer;
        let visible = !typing_trainer_page
            || !self.typing_trainer.ui_hidden
            || self.typing_trainer.is_finished()
            || self.typing_trainer_history_open;
        ctx.animate_bool_with_time(
            egui::Id::new("typing_trainer_chrome_visible"),
            visible,
            0.18,
        )
    }

    pub(super) fn pause_typing_trainer_if_inactive(&mut self, now: std::time::Instant) {
        if self.main_menu_tab != MainMenuTab::Advanced
            || self.settings_tab != SettingsTab::TypingTrainer
        {
            self.typing_trainer.pause_if_running(now);
            self.flush_typing_trainer_symbol_stats();
        }
    }

    /// Persists adaptive statistics recorded since the last save. Called when
    /// the trainer page is left and on exit, so an abandoned session still
    /// teaches the trainer which characters are difficult.
    pub(super) fn flush_typing_trainer_symbol_stats(&mut self) {
        if !self.typing_trainer.symbol_stats_unsaved() {
            return;
        }
        save_typing_trainer_symbol_stats(&self.typing_trainer.symbol_stats);
        self.typing_trainer.mark_symbol_stats_saved();
    }

    /// Rebuilds the symbol pool only when the keymap or the selected trainer
    /// language changed — deriving it walks every layer of the layout, which is
    /// far too much work to repeat on every frame.
    fn refresh_typing_trainer_symbol_pool(&mut self, layout: &KeyboardLayout) {
        let key_output_layout = crate::keycode::KeyOutputLayout::from(self.typing_trainer.language);
        let cached = self.typing_trainer_symbol_pool_source.as_ref().is_some_and(
            |(layers, cached_layout)| {
                *cached_layout == key_output_layout && *layers == layout.layers
            },
        );
        if cached {
            return;
        }
        let symbols = crate::app::typing_trainer_symbols::printable_symbols_from_layout(
            layout,
            key_output_layout,
        );
        self.typing_trainer_symbol_pool_source = Some((layout.layers.clone(), key_output_layout));
        self.typing_trainer.set_symbol_pool(symbols);
    }

    pub(super) fn draw_typing_trainer_page(
        &mut self,
        ui: &mut egui::Ui,
        layout: &KeyboardLayout,
        ctx: &egui::Context,
        content_rect: egui::Rect,
    ) {
        self.refresh_typing_trainer_symbol_pool(layout);
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let now = std::time::Instant::now();
        let remaining_secs = self.typing_trainer.remaining_secs_at(now);
        self.record_finished_typing_trainer_run(now);
        if self.typing_trainer.is_finished() {
            self.typing_trainer.ui_hidden = false;
        }
        let chrome_opacity = self.typing_trainer_chrome_opacity(ctx);
        if self.typing_trainer.started_at.is_some()
            && !self.typing_trainer.is_paused()
            && !self.typing_trainer.is_finished()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if chrome_opacity > 0.0 && chrome_opacity < 1.0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                let mut focus_timer_y = None;
                ui.scope(|ui| {
                    ui.set_opacity(chrome_opacity);
                    if chrome_opacity <= 0.96 {
                        ui.disable();
                    }
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
                    focus_timer_y = Some(self.reserve_typing_trainer_stats_slot(ui, metrics));
                });
                self.draw_typing_trainer_focus_timer(
                    ui,
                    metrics,
                    remaining_secs,
                    chrome_opacity,
                    focus_timer_y,
                );
                self.draw_typing_trainer_text(ui, metrics, lang, now, dark);
                ui.add_space(metrics.value(10.0));
                self.draw_typing_trainer_actions(ui, metrics, lang, chrome_opacity);
            });
        });

        self.draw_typing_trainer_history_modal(ctx, metrics, lang, dark);

        if self.typing_trainer_focus_mode_active() {
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }
    }

    pub(super) fn handle_typing_trainer_input(&mut self, ctx: &egui::Context) {
        if self.main_menu_tab != MainMenuTab::Advanced
            || self.settings_tab != SettingsTab::TypingTrainer
            || self.keycode_picker.open
            || self.unlock_open
            || self.typing_trainer_history_open
            || egui::Popup::is_any_open(ctx)
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
                        if self.typing_trainer.is_symbol_training() {
                            if let Some(expected) = self.typing_trainer.expected_char() {
                                self.typing_trainer
                                    .record_symbol_attempt(expected, expected != ch);
                            }
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
                    self.typing_trainer.finish(now);
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

    fn draw_typing_trainer_controls(
        &mut self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
    ) {
        let language_labels = TYPING_TRAINER_LANGUAGES
            .iter()
            .map(|language| language.label().to_owned())
            .collect::<Vec<_>>();
        let selected_language = TYPING_TRAINER_LANGUAGES
            .iter()
            .position(|language| *language == self.typing_trainer.language)
            .unwrap_or(0);
        let symbol_training = self.typing_trainer.is_symbol_training();
        let mode_labels = [
            crate::i18n::tr_catalog(lang, "typing_trainer.time").to_owned(),
            crate::i18n::tr_catalog(
                lang,
                if symbol_training {
                    "typing_trainer.count"
                } else {
                    "typing_trainer.words"
                },
            )
            .to_owned(),
        ];
        let selected_mode = match self.typing_trainer.mode {
            TypingTrainerMode::Time => 0,
            TypingTrainerMode::Words | TypingTrainerMode::Symbols => 1,
        };
        let value_labels = match self.typing_trainer.mode {
            TypingTrainerMode::Time => TYPING_TRAINER_DURATIONS
                .iter()
                .map(|duration| duration.to_string())
                .collect::<Vec<_>>(),
            TypingTrainerMode::Words | TypingTrainerMode::Symbols if symbol_training => {
                crate::app::typing_trainer_symbols::TYPING_TRAINER_SYMBOL_COUNTS
                    .iter()
                    .map(|symbol_count| symbol_count.to_string())
                    .collect::<Vec<_>>()
            }
            TypingTrainerMode::Words | TypingTrainerMode::Symbols => TYPING_TRAINER_WORD_COUNTS
                .iter()
                .map(|word_count| word_count.to_string())
                .collect::<Vec<_>>(),
        };
        let selected_value = match self.typing_trainer.mode {
            TypingTrainerMode::Time => TYPING_TRAINER_DURATIONS
                .iter()
                .position(|duration| *duration == self.typing_trainer.duration_secs)
                .unwrap_or(1),
            TypingTrainerMode::Words | TypingTrainerMode::Symbols if symbol_training => {
                crate::app::typing_trainer_symbols::TYPING_TRAINER_SYMBOL_COUNTS
                    .iter()
                    .position(|symbol_count| *symbol_count == self.typing_trainer.word_count)
                    .unwrap_or(1)
            }
            TypingTrainerMode::Words | TypingTrainerMode::Symbols => TYPING_TRAINER_WORD_COUNTS
                .iter()
                .position(|word_count| *word_count == self.typing_trainer.word_count)
                .unwrap_or(1),
        };
        let language_size = metrics.size(96.0, 32.0);
        let mode_size = metrics.size(116.0, 32.0);
        let value_size = metrics.size(88.0, 32.0);
        let punctuation_size = metrics.size(112.0, 32.0);
        let numbers_size = metrics.size(104.0, 32.0);
        let material_size = metrics.size(116.0, 32.0);
        let gap = metrics.value(10.0);
        // Every control the current material shows, the material toggle
        // included — a short total clips the trailing controls away.
        let shared_controls_width = language_size.x + gap + mode_size.x + gap + value_size.x;
        let text_options_width = gap + punctuation_size.x + gap + numbers_size.x;
        let total_size = egui::vec2(
            shared_controls_width
                + if symbol_training {
                    0.0
                } else {
                    text_options_width
                }
                + gap
                + material_size.x,
            mode_size.y,
        );
        let mut settings_changed = false;

        ui.allocate_ui_with_layout(
            total_size,
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                // The language also selects the input mapping the symbol pool is
                // derived from, so it stays available in symbol training too.
                let language_dropdown_id =
                    ui.make_persistent_id("typing_trainer_language_dropdown");
                let (_, picked_language) = crate::ui_style::modern_dropdown_select_sized(
                    ui,
                    language_dropdown_id,
                    &language_labels,
                    selected_language,
                    language_size.x,
                    language_size.y,
                    metrics.value(12.5),
                );
                if let Some(picked) = picked_language {
                    self.typing_trainer
                        .set_language(TYPING_TRAINER_LANGUAGES[picked]);
                    settings_changed = true;
                }

                ui.add_space(gap);

                let mode_dropdown_id = ui.make_persistent_id("typing_trainer_mode_dropdown");
                let (_, picked_mode) = crate::ui_style::modern_dropdown_select_sized(
                    ui,
                    mode_dropdown_id,
                    &mode_labels,
                    selected_mode,
                    mode_size.x,
                    mode_size.y,
                    metrics.value(12.5),
                );
                if let Some(picked) = picked_mode {
                    let mode = if picked == 0 {
                        TypingTrainerMode::Time
                    } else {
                        TypingTrainerMode::Words
                    };
                    self.typing_trainer.set_mode(mode);
                    settings_changed = true;
                }

                ui.add_space(gap);

                let value_dropdown_id = ui.make_persistent_id("typing_trainer_value_dropdown");
                let (_, picked_value) = crate::ui_style::modern_dropdown_select_sized(
                    ui,
                    value_dropdown_id,
                    &value_labels,
                    selected_value,
                    value_size.x,
                    value_size.y,
                    metrics.value(12.5),
                );
                if let Some(picked) = picked_value {
                    match self.typing_trainer.mode {
                        TypingTrainerMode::Time => self
                            .typing_trainer
                            .set_duration(TYPING_TRAINER_DURATIONS[picked]),
                        TypingTrainerMode::Words | TypingTrainerMode::Symbols
                            if symbol_training =>
                        {
                            self.typing_trainer.set_word_count(
                                crate::app::typing_trainer_symbols::TYPING_TRAINER_SYMBOL_COUNTS
                                    [picked],
                            )
                        }
                        TypingTrainerMode::Words | TypingTrainerMode::Symbols => self
                            .typing_trainer
                            .set_word_count(TYPING_TRAINER_WORD_COUNTS[picked]),
                    }
                    settings_changed = true;
                }

                if !symbol_training {
                    ui.add_space(gap);
                    let punctuation_label =
                        crate::i18n::tr_catalog(lang, "typing_trainer.punctuation");
                    let punctuation_short_label =
                        crate::i18n::tr_catalog(lang, "typing_trainer.punctuation_short");
                    if crate::ui_style::modern_toggle_pill(
                        ui,
                        ".,?",
                        punctuation_short_label,
                        punctuation_label,
                        punctuation_size,
                        self.typing_trainer.punctuation_enabled,
                    )
                    .clicked()
                    {
                        self.typing_trainer
                            .set_punctuation_enabled(!self.typing_trainer.punctuation_enabled);
                        settings_changed = true;
                    }
                    ui.add_space(gap);
                    let numbers_label = crate::i18n::tr_catalog(lang, "typing_trainer.numbers");
                    let numbers_short_label =
                        crate::i18n::tr_catalog(lang, "typing_trainer.numbers_short");
                    if crate::ui_style::modern_toggle_pill(
                        ui,
                        "123",
                        numbers_short_label,
                        numbers_label,
                        numbers_size,
                        self.typing_trainer.numbers_enabled,
                    )
                    .clicked()
                    {
                        self.typing_trainer
                            .set_numbers_enabled(!self.typing_trainer.numbers_enabled);
                        settings_changed = true;
                    }
                }
                ui.add_space(gap);
                if crate::ui_style::modern_toggle_pill(
                    ui,
                    "#?",
                    crate::i18n::tr_catalog(lang, "typing_trainer.symbols"),
                    crate::i18n::tr_catalog(lang, "typing_trainer.symbols_tooltip"),
                    material_size,
                    symbol_training,
                )
                .clicked()
                {
                    self.typing_trainer.set_symbols_enabled(!symbol_training);
                    settings_changed = true;
                }
            },
        );
        if settings_changed {
            self.save_typing_trainer_settings();
        }
    }

    fn reserve_typing_trainer_stats_slot(
        &self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
    ) -> f32 {
        let item_width = metrics.value(92.0);
        let stat_height = metrics.value(42.0);
        let total_width = item_width * 4.0;
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(total_width, stat_height), Sense::hover());

        rect.bottom() - metrics.value(11.0)
    }

    fn draw_typing_trainer_focus_timer(
        &self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        remaining_secs: u32,
        chrome_opacity: f32,
        focus_timer_y: Option<f32>,
    ) {
        let width = ui.available_width().min(metrics.value(860.0));
        let height = metrics.value(24.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), Sense::hover());
        let timer_opacity = if self.typing_trainer.ui_hidden && !self.typing_trainer.is_finished() {
            (1.0 - chrome_opacity).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if timer_opacity <= 0.01 {
            return;
        }

        ui.painter().text(
            egui::pos2(rect.center().x, focus_timer_y.unwrap_or(rect.center().y)),
            egui::Align2::CENTER_CENTER,
            self.typing_trainer_focus_status(remaining_secs),
            FontId::proportional(metrics.value(22.0)),
            typing_trainer_color_with_opacity(app_accent(), timer_opacity),
        );
    }

    fn typing_trainer_focus_status(&self, remaining_secs: u32) -> String {
        match self.typing_trainer.mode {
            TypingTrainerMode::Time => {
                let timer_secs = if self.typing_trainer.started_at.is_some() {
                    remaining_secs
                } else {
                    self.typing_trainer.duration_secs
                };
                timer_secs.to_string()
            }
            TypingTrainerMode::Words | TypingTrainerMode::Symbols
                if self.typing_trainer.is_symbol_training() =>
            {
                format!(
                    "{}/{}",
                    self.typing_trainer.typed_chars.len(),
                    self.typing_trainer.word_count
                )
            }
            TypingTrainerMode::Words => {
                let (completed_words, target_words) = self.typing_trainer.word_progress();
                format!("{completed_words}/{target_words}")
            }
            TypingTrainerMode::Symbols => String::new(),
        }
    }

    fn draw_typing_trainer_text(
        &mut self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        now: std::time::Instant,
        dark: bool,
    ) {
        let width = ui.available_width().min(metrics.value(860.0));
        let height = metrics.value(214.0);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
        if resp.hovered() && !self.typing_trainer_focus_mode_active() {
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
        if self.typing_trainer.is_finished() {
            self.draw_typing_trainer_results(ui, text_rect, metrics, lang, now, dark);
            return;
        }
        if self.typing_trainer.is_symbol_training() && self.typing_trainer.symbol_pool.is_empty() {
            ui.painter().text(
                text_rect.center(),
                egui::Align2::CENTER_CENTER,
                crate::i18n::tr_catalog(lang, "typing_trainer.symbols_unavailable"),
                FontId::proportional(metrics.value(16.0)),
                app_muted_text(dark),
            );
            return;
        }
        self.ensure_typing_trainer_visible_text(max_line_chars, max_visible_lines);

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
        let line_starts = typing_trainer_line_starts(&target_chars, max_line_chars);
        let mut line_idx = line_starts
            .partition_point(|start| *start <= idx)
            .saturating_sub(1);
        let mut caret_pos = None;
        let target_len = target_chars.len();
        let mut visible_lines = 1;

        while idx < target_len && visible_lines <= max_visible_lines && y <= text_rect.bottom() {
            let draw_end = line_starts.get(line_idx + 1).copied().unwrap_or(target_len);
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
            if idx < target_len {
                x = text_rect.left();
                y += line_height;
                visible_lines += 1;
                line_idx += 1;
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

    fn draw_typing_trainer_results(
        &self,
        ui: &egui::Ui,
        rect: egui::Rect,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        now: std::time::Instant,
        dark: bool,
    ) {
        let stats = self.typing_trainer.stats_at(now);
        let elapsed_secs = self.typing_trainer.elapsed_secs_at(now).ceil() as u32;
        let labels = [
            (
                crate::i18n::tr_catalog(lang, "typing_trainer.wpm"),
                stats.wpm.to_string(),
            ),
            (
                crate::i18n::tr_catalog(lang, "typing_trainer.accuracy"),
                format!("{:.0}%", stats.accuracy),
            ),
            (
                crate::i18n::tr_catalog(lang, "typing_trainer.characters"),
                stats.typed_chars.to_string(),
            ),
            (
                crate::i18n::tr_catalog(lang, "typing_trainer.errors"),
                stats.errors.to_string(),
            ),
            (
                crate::i18n::tr_catalog(lang, "typing_trainer.time"),
                elapsed_secs.to_string(),
            ),
        ];
        let item_width = metrics.value(92.0);
        let total_width = item_width * labels.len() as f32;
        let start_x = rect.center().x - total_width / 2.0;
        let result_center_y = rect.center().y - metrics.value(72.0);
        let label_y = result_center_y - metrics.value(20.0);
        let value_y = result_center_y + metrics.value(16.0);

        for (idx, (label, value)) in labels.into_iter().enumerate() {
            let center_x = start_x + item_width * (idx as f32 + 0.5);
            ui.painter().text(
                egui::pos2(center_x, label_y),
                egui::Align2::CENTER_CENTER,
                label,
                FontId::proportional(metrics.value(13.0)),
                app_muted_text(dark),
            );
            ui.painter().text(
                egui::pos2(center_x, value_y),
                egui::Align2::CENTER_CENTER,
                value,
                FontId::proportional(metrics.value(30.0)),
                app_accent(),
            );
        }
    }

    fn ensure_typing_trainer_visible_text(
        &mut self,
        max_line_chars: usize,
        max_visible_lines: usize,
    ) {
        if self.typing_trainer.mode == TypingTrainerMode::Words {
            return;
        }
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

    fn draw_typing_trainer_actions(
        &mut self,
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        chrome_opacity: f32,
    ) {
        let button_size = metrics.size(104.0, 32.0);
        let gap = metrics.value(10.0);
        let finished = self.typing_trainer.is_finished();
        let has_history = !self.app_settings.typing_trainer_history.is_empty();
        let action_count = match (finished, has_history) {
            (true, true) => 3.0,
            (true, false) => 2.0,
            (false, true) => 2.0,
            (false, false) => 1.0,
        };
        let total_size = egui::vec2(
            button_size.x * action_count + gap * (action_count - 1.0),
            button_size.y,
        );
        ui.scope(|ui| {
            ui.set_opacity(chrome_opacity);
            if chrome_opacity <= 0.96 {
                ui.disable();
            }
            ui.allocate_ui_with_layout(
                total_size,
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    if finished {
                        if crate::ui_style::modern_button(
                            ui,
                            crate::i18n::tr_catalog(lang, "typing_trainer.retry"),
                            button_size,
                            true,
                        )
                        .clicked()
                        {
                            self.typing_trainer.retry();
                        }
                        ui.add_space(gap);
                        if crate::ui_style::modern_button(
                            ui,
                            crate::i18n::tr_catalog(lang, "typing_trainer.next"),
                            button_size,
                            true,
                        )
                        .clicked()
                        {
                            self.typing_trainer.reset();
                        }
                    } else {
                        if crate::ui_style::modern_button(
                            ui,
                            crate::i18n::tr_catalog(lang, "typing_trainer.restart"),
                            button_size,
                            true,
                        )
                        .clicked()
                        {
                            self.typing_trainer.reset();
                        }
                    }

                    if has_history {
                        ui.add_space(gap);
                        if crate::ui_style::modern_button(
                            ui,
                            crate::i18n::tr_catalog(lang, "typing_trainer.history"),
                            button_size,
                            true,
                        )
                        .clicked()
                        {
                            self.typing_trainer_history_open = true;
                            self.typing_trainer.ui_hidden = false;
                        }
                    }
                },
            );
        });
    }

    fn draw_typing_trainer_history_modal(
        &mut self,
        ctx: &egui::Context,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        dark: bool,
    ) {
        if !self.typing_trainer_history_open {
            return;
        }

        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.typing_trainer_history_open = false;
            return;
        }

        let screen_rect = ctx.content_rect();
        egui::Area::new("typing_trainer_history_backdrop".into())
            .order(egui::Order::Foreground)
            .fixed_pos(screen_rect.min)
            .show(ctx, |ui| {
                let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, screen_rect.size());
                ui.interact(
                    rect,
                    egui::Id::new("typing_trainer_history_backdrop_blocker"),
                    egui::Sense::click_and_drag(),
                );
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    Color32::from_black_alpha(crate::ui_style::modal_backdrop_alpha(dark)),
                );
            });

        let entries = self.app_settings.typing_trainer_history.clone();
        let summary =
            typing_trainer_history_summary_for_settings(&entries, self.typing_trainer.settings());
        let mut open = self.typing_trainer_history_open;
        let mut clear_clicked = false;
        let mut close_clicked = false;

        crate::ui_style::centered_modal_window(
            ctx,
            crate::i18n::tr_catalog(lang, "typing_trainer.history"),
            egui::Id::new("typing_trainer_history_window"),
            &mut open,
            Vec2::new(660.0, 430.0),
        )
        .show(ctx, |ui| {
            ui.set_min_size(Vec2::new(640.0, 380.0));
            let rect = ui.max_rect();
            let content_rect = egui::Rect::from_min_max(
                egui::pos2(rect.left() + 30.0, rect.top() + 18.0),
                egui::pos2(rect.right() - 30.0, rect.bottom() - 74.0),
            );
            let button_size = crate::ui_style::modal_action_button_size();
            let button_gap = metrics.value(10.0);
            let buttons_width = button_size.x * 2.0 + button_gap;
            let button_rect = egui::Rect::from_center_size(
                egui::pos2(rect.center().x, rect.bottom() - 34.0),
                egui::vec2(buttons_width, button_size.y),
            );

            crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
                Self::draw_typing_trainer_history_content(
                    ui, metrics, lang, dark, &entries, summary,
                );
            });

            crate::ui_style::allocate_ui_at_rect(ui, button_rect, |ui| {
                ui.allocate_ui_with_layout(
                    button_rect.size(),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        if crate::ui_style::modern_button(
                            ui,
                            crate::i18n::tr_catalog(lang, "typing_trainer.clear_history"),
                            button_size,
                            !entries.is_empty(),
                        )
                        .clicked()
                        {
                            clear_clicked = true;
                        }
                        ui.add_space(button_gap);
                        if crate::ui_style::modern_button(
                            ui,
                            crate::i18n::tr_catalog(lang, "common.close"),
                            button_size,
                            true,
                        )
                        .clicked()
                        {
                            close_clicked = true;
                        }
                    },
                );
            });
        });

        if clear_clicked {
            self.app_settings.typing_trainer_history.clear();
            save_app_settings(&self.app_settings);
            self.typing_trainer_history_open = false;
        } else {
            self.typing_trainer_history_open = open && !close_clicked;
        }
    }

    fn draw_typing_trainer_history_content(
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        dark: bool,
        entries: &[TypingTrainerRunRecord],
        summary: TypingTrainerHistorySummary,
    ) {
        if entries.is_empty() {
            crate::ui_style::modal_empty_state(
                ui,
                crate::i18n::tr_catalog(lang, "typing_trainer.history_empty"),
                None,
            );
            return;
        }

        Self::draw_typing_trainer_history_summary(ui, metrics, lang, dark, summary);
        ui.add_space(metrics.value(10.0));
        Self::draw_typing_trainer_history_table(ui, metrics, lang, dark, entries);
    }

    fn draw_typing_trainer_history_summary(
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        dark: bool,
        summary: TypingTrainerHistorySummary,
    ) {
        let width = ui.available_width();
        let height = metrics.value(50.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), Sense::hover());
        let muted = app_muted_text(dark);
        let text = if dark {
            Color32::from_gray(228)
        } else {
            ui.visuals().text_color()
        };
        let separator = muted.gamma_multiply(if dark { 0.34 } else { 0.24 });
        let label_font = FontId::proportional(metrics.value(11.0));
        let value_font = FontId::proportional(metrics.value(16.0));
        let title_x = rect.left() + metrics.value(8.0);
        let title_y = rect.center().y;
        let columns_left = rect.left() + metrics.value(132.0);
        let column_width = ((rect.right() - columns_left).max(0.0)) / 4.0;
        let label_y = rect.top() + metrics.value(15.0);
        let value_y = rect.top() + metrics.value(34.0);
        let items = [
            (
                crate::i18n::tr_catalog(lang, "typing_trainer.runs"),
                summary.run_count.to_string(),
            ),
            (
                crate::i18n::tr_catalog(lang, "typing_trainer.best_wpm"),
                typing_trainer_optional_summary_value(summary.best_wpm, ""),
            ),
            (
                crate::i18n::tr_catalog(lang, "typing_trainer.avg_wpm"),
                typing_trainer_optional_summary_value(summary.average_wpm, ""),
            ),
            (
                crate::i18n::tr_catalog(lang, "typing_trainer.avg_accuracy"),
                typing_trainer_optional_summary_value(summary.average_accuracy_percent, "%"),
            ),
        ];
        let painter = ui.painter();

        painter.text(
            egui::pos2(title_x, title_y),
            egui::Align2::LEFT_CENTER,
            crate::i18n::tr_catalog(lang, "typing_trainer.current_set"),
            FontId::proportional(metrics.value(12.0)),
            muted,
        );

        for (idx, (label, value)) in items.into_iter().enumerate() {
            let center_x = columns_left + column_width * (idx as f32 + 0.5);
            painter.text(
                egui::pos2(center_x, label_y),
                egui::Align2::CENTER_CENTER,
                label,
                label_font.clone(),
                muted,
            );
            painter.text(
                egui::pos2(center_x, value_y),
                egui::Align2::CENTER_CENTER,
                value,
                value_font.clone(),
                text,
            );
        }

        painter.line_segment(
            [
                egui::pos2(rect.left(), rect.bottom()),
                egui::pos2(rect.right(), rect.bottom()),
            ],
            Stroke::new(metrics.value(1.0), separator),
        );
    }

    fn draw_typing_trainer_history_table(
        ui: &mut egui::Ui,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        dark: bool,
        entries: &[TypingTrainerRunRecord],
    ) {
        let width = ui.available_width();
        let scrollbar_gutter = metrics.value(16.0);
        let table_width = (width - scrollbar_gutter).max(0.0);
        let header_height = metrics.value(24.0);
        let row_height = metrics.value(30.0);
        let scroll_height = (ui.available_height() - header_height - metrics.value(4.0)).max(0.0);
        let (header_rect, _) =
            ui.allocate_exact_size(egui::vec2(table_width, header_height), Sense::hover());
        Self::paint_typing_trainer_history_header(ui, header_rect, metrics, lang, dark);
        ui.add_space(metrics.value(4.0));

        egui::ScrollArea::vertical()
            .max_height(scroll_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(table_width);
                for (idx, entry) in entries.iter().enumerate() {
                    let (row_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), row_height),
                        Sense::hover(),
                    );
                    Self::paint_typing_trainer_history_row(
                        ui,
                        row_rect,
                        metrics,
                        lang,
                        dark,
                        entry,
                        idx + 1 < entries.len(),
                    );
                }
            });
    }

    fn paint_typing_trainer_history_header(
        ui: &egui::Ui,
        rect: egui::Rect,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        dark: bool,
    ) {
        let muted = app_muted_text(dark);
        let separator = muted.gamma_multiply(if dark { 0.34 } else { 0.24 });
        let (date_x, run_x, wpm_x, accuracy_x, errors_x) =
            typing_trainer_history_columns(rect, metrics);
        let header_y = rect.center().y;
        let painter = ui.painter();

        painter.text(
            egui::pos2(date_x, header_y),
            egui::Align2::LEFT_CENTER,
            crate::i18n::tr_catalog(lang, "typing_trainer.date"),
            FontId::proportional(metrics.value(11.0)),
            muted,
        );
        painter.text(
            egui::pos2(run_x, header_y),
            egui::Align2::LEFT_CENTER,
            crate::i18n::tr_catalog(lang, "typing_trainer.run"),
            FontId::proportional(metrics.value(11.0)),
            muted,
        );
        painter.text(
            egui::pos2(wpm_x, header_y),
            egui::Align2::CENTER_CENTER,
            crate::i18n::tr_catalog(lang, "typing_trainer.wpm"),
            FontId::proportional(metrics.value(11.0)),
            muted,
        );
        painter.text(
            egui::pos2(accuracy_x, header_y),
            egui::Align2::CENTER_CENTER,
            crate::i18n::tr_catalog(lang, "typing_trainer.accuracy"),
            FontId::proportional(metrics.value(11.0)),
            muted,
        );
        painter.text(
            egui::pos2(errors_x, header_y),
            egui::Align2::CENTER_CENTER,
            crate::i18n::tr_catalog(lang, "typing_trainer.errors"),
            FontId::proportional(metrics.value(11.0)),
            muted,
        );
        painter.line_segment(
            [
                egui::pos2(rect.left(), rect.bottom()),
                egui::pos2(rect.right(), rect.bottom()),
            ],
            Stroke::new(metrics.value(1.0), separator),
        );
    }

    fn paint_typing_trainer_history_row(
        ui: &egui::Ui,
        rect: egui::Rect,
        metrics: crate::ui_style::ResponsiveMetrics,
        lang: crate::i18n::Language,
        dark: bool,
        entry: &TypingTrainerRunRecord,
        draw_separator: bool,
    ) {
        let muted = app_muted_text(dark);
        let separator = muted.gamma_multiply(if dark { 0.34 } else { 0.24 });
        let text = if dark {
            Color32::from_gray(228)
        } else {
            ui.visuals().text_color()
        };
        let (date_x, run_x, wpm_x, accuracy_x, errors_x) =
            typing_trainer_history_columns(rect, metrics);
        let row_center_y = rect.center().y;
        let painter = ui.painter();

        painter.text(
            egui::pos2(date_x, row_center_y),
            egui::Align2::LEFT_CENTER,
            typing_trainer_history_date_label(entry.finished_at_unix_secs),
            FontId::proportional(metrics.value(12.0)),
            muted,
        );
        painter.text(
            egui::pos2(run_x, row_center_y),
            egui::Align2::LEFT_CENTER,
            typing_trainer_history_run_label(entry, lang),
            FontId::proportional(metrics.value(12.0)),
            text,
        );
        painter.text(
            egui::pos2(wpm_x, row_center_y),
            egui::Align2::CENTER_CENTER,
            entry.wpm.to_string(),
            FontId::proportional(metrics.value(12.0)),
            text,
        );
        painter.text(
            egui::pos2(accuracy_x, row_center_y),
            egui::Align2::CENTER_CENTER,
            format!("{}%", entry.accuracy_percent),
            FontId::proportional(metrics.value(12.0)),
            text,
        );
        painter.text(
            egui::pos2(errors_x, row_center_y),
            egui::Align2::CENTER_CENTER,
            entry.errors.to_string(),
            FontId::proportional(metrics.value(12.0)),
            text,
        );
        if draw_separator {
            painter.line_segment(
                [
                    egui::pos2(rect.left(), rect.bottom()),
                    egui::pos2(rect.right(), rect.bottom()),
                ],
                Stroke::new(metrics.value(1.0), separator),
            );
        }
    }

    fn record_finished_typing_trainer_run(&mut self, now: std::time::Instant) {
        if !self.typing_trainer.history_record_pending() {
            return;
        }

        let finished_at_unix_secs = chrono::Local::now().timestamp();
        if let Some(record) =
            TypingTrainerRunRecord::from_state(&self.typing_trainer, now, finished_at_unix_secs)
        {
            push_typing_trainer_history(&mut self.app_settings.typing_trainer_history, record);
            save_typing_trainer_symbol_stats(&self.typing_trainer.symbol_stats);
            self.typing_trainer.mark_symbol_stats_saved();
            save_app_settings(&self.app_settings);
        }
        self.typing_trainer.mark_history_recorded();
    }

    fn save_typing_trainer_settings(&mut self) {
        let settings = self.typing_trainer.settings();
        if self.app_settings.typing_trainer != settings {
            self.app_settings.typing_trainer = settings;
            save_app_settings(&self.app_settings);
        }
    }
}

fn typing_trainer_history_date_label(finished_at_unix_secs: i64) -> String {
    let Some(finished_at) =
        chrono::TimeZone::timestamp_opt(&chrono::Local, finished_at_unix_secs, 0).single()
    else {
        return "--".to_owned();
    };
    finished_at.format("%m-%d %H:%M").to_string()
}

fn typing_trainer_optional_summary_value(value: Option<u32>, suffix: &str) -> String {
    value
        .map(|value| format!("{value}{suffix}"))
        .unwrap_or_else(|| "--".to_owned())
}

fn typing_trainer_history_run_label(
    entry: &TypingTrainerRunRecord,
    lang: crate::i18n::Language,
) -> String {
    if entry.symbols_enabled {
        let pacing = if entry.mode == TypingTrainerMode::Time {
            entry.duration_secs.to_string()
        } else {
            entry.word_count.to_string()
        };
        return format!(
            "{} / {} {}",
            crate::i18n::tr_catalog(lang, "typing_trainer.symbols"),
            crate::i18n::tr_catalog(
                lang,
                if entry.mode == TypingTrainerMode::Time {
                    "typing_trainer.time"
                } else {
                    "typing_trainer.count"
                },
            ),
            pacing
        );
    }
    let mode = match entry.mode {
        TypingTrainerMode::Time => format!(
            "{} {}",
            crate::i18n::tr_catalog(lang, "typing_trainer.time"),
            entry.duration_secs
        ),
        TypingTrainerMode::Words => format!(
            "{} {}",
            crate::i18n::tr_catalog(lang, "typing_trainer.words"),
            entry.word_count
        ),
        TypingTrainerMode::Symbols => String::new(),
    };
    let mut modifiers = String::new();
    if entry.punctuation_enabled {
        modifiers.push_str(" @");
    }
    if entry.numbers_enabled {
        modifiers.push_str(" #");
    }
    format!("{} / {}{}", entry.language.label(), mode, modifiers)
}

fn typing_trainer_history_columns(
    rect: egui::Rect,
    metrics: crate::ui_style::ResponsiveMetrics,
) -> (f32, f32, f32, f32, f32) {
    (
        rect.left() + metrics.value(8.0),
        rect.left() + metrics.value(128.0),
        rect.right() - metrics.value(176.0),
        rect.right() - metrics.value(104.0),
        rect.right() - metrics.value(34.0),
    )
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
    if !target_chars.iter().any(|ch| ch.is_whitespace()) {
        return (0..target_len).step_by(max_line_chars).collect();
    }
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
        Some(ch) if ch == target => {
            if dark {
                Color32::from_gray(238)
            } else {
                ui.visuals().text_color()
            }
        }
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

fn typing_trainer_color_with_opacity(color: Color32, opacity: f32) -> Color32 {
    let alpha = (color.a() as f32 * opacity.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

#[cfg(test)]
mod typing_trainer_ui_tests {
    use super::*;

    fn test_app() -> EntropyApp {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        EntropyApp::new(&creation_context)
    }

    #[test]
    fn time_based_symbol_training_focus_status_shows_remaining_seconds() {
        let mut app = test_app();
        app.typing_trainer.set_symbols_enabled(true);
        app.typing_trainer.set_mode(TypingTrainerMode::Time);
        app.typing_trainer.word_count = 25;
        app.typing_trainer.typed_chars = vec!['!', '?'];
        app.typing_trainer.started_at = Some(std::time::Instant::now());

        assert_eq!(app.typing_trainer_focus_status(42), "42");
    }

    #[test]
    fn fixed_count_symbol_training_focus_status_shows_character_progress() {
        let mut app = test_app();
        app.typing_trainer.set_symbols_enabled(true);
        app.typing_trainer.set_mode(TypingTrainerMode::Words);
        app.typing_trainer.word_count = 50;
        app.typing_trainer.typed_chars = vec!['!', '?', '/'];

        assert_eq!(app.typing_trainer_focus_status(42), "3/50");
    }

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

    #[test]
    fn typing_trainer_wraps_a_continuous_symbol_sequence() {
        let chars = "abcdefghijklmnopqrstuvwxyz".chars().collect::<Vec<_>>();

        assert_eq!(typing_trainer_line_starts(&chars, 10), vec![0, 10, 20]);
    }
}
