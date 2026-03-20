use std::collections::BTreeMap;
use zellij_tile::prelude::*;

const DIGIT_PATTERNS: [[&str; 5]; 11] = [
    ["111", "101", "101", "101", "111"], // 0
    ["010", "110", "010", "010", "010"], // 1
    ["111", "001", "111", "100", "111"], // 2
    ["111", "001", "111", "001", "111"], // 3
    ["101", "101", "111", "001", "001"], // 4
    ["111", "100", "111", "001", "111"], // 5
    ["111", "100", "111", "101", "111"], // 6
    ["111", "001", "001", "001", "001"], // 7
    ["111", "101", "111", "101", "111"], // 8
    ["111", "101", "111", "001", "111"], // 9
    ["000", "010", "000", "010", "000"], // : (colon)
];

const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

#[derive(Default, PartialEq)]
enum Phase {
    #[default]
    Work,
    Break,
    BreakDone,
    Finished,
}

#[derive(Default)]
struct Pomo {
    seconds_remaining: usize,
    running: bool,
    work_duration: usize,
    break_duration: usize,
    phase: Phase,
    plugin_id: u32,
    is_fullscreen: bool,
    spin_idx: usize,
}

register_plugin!(Pomo);

const DEFAULT_WORK_SECS: usize = 1500;
const DEFAULT_BREAK_SECS: usize = 300;

impl ZellijPlugin for Pomo {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.work_duration = configuration
            .get("work_seconds")
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_WORK_SECS);
        self.break_duration = configuration
            .get("break_seconds")
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_BREAK_SECS);
        self.seconds_remaining = self.work_duration;
        self.plugin_id = get_plugin_ids().plugin_id;

        subscribe(&[EventType::Timer, EventType::Key]);
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
        ]);

        // Auto-start the work timer
        self.running = true;
        set_timeout(1.0);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::Timer(_) => {
                if self.running && self.seconds_remaining > 0 {
                    self.seconds_remaining -= 1;
                    self.spin_idx = (self.spin_idx + 1) % SPINNER.len();
                    if self.seconds_remaining == 0 {
                        self.running = false;
                        match self.phase {
                            Phase::Work => self.start_break(),
                            Phase::Break => self.phase = Phase::BreakDone,
                            _ => {}
                        }
                    } else {
                        set_timeout(1.0);
                    }
                }
                true
            }
            Event::Key(key) => {
                match self.phase {
                    Phase::Work => match key.bare_key {
                        BareKey::Char(' ') => {
                            self.running = !self.running;
                            if self.running {
                                set_timeout(1.0);
                            }
                        }
                        BareKey::Char('r') => {
                            self.seconds_remaining = self.work_duration;
                            self.running = false;
                        }
                        _ => return false,
                    },
                    Phase::Break => return false,
                    Phase::BreakDone => match key.bare_key {
                        BareKey::Char('y') | BareKey::Char('Y') | BareKey::Enter => {
                            self.start_work();
                        }
                        BareKey::Char('n') | BareKey::Char('N') => {
                            self.phase = Phase::Finished;
                            self.exit_fullscreen();
                        }
                        _ => return false,
                    },
                    Phase::Finished => match key.bare_key {
                        BareKey::Char('r') => {
                            self.phase = Phase::Work;
                            self.seconds_remaining = self.work_duration;
                            self.running = false;
                        }
                        _ => return false,
                    },
                }
                true
            }
            _ => false,
        }
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        match pipe_message.name.as_str() {
            "pomo" => {
                if let Some(payload) = &pipe_message.payload {
                    let parts: Vec<&str> = payload.trim().split_whitespace().collect();
                    match parts.first().copied() {
                        Some("start") => {
                            if let Some(work) = parts.get(1).and_then(|s| s.parse().ok()) {
                                self.work_duration = work;
                            }
                            if let Some(brk) = parts.get(2).and_then(|s| s.parse().ok()) {
                                self.break_duration = brk;
                            }
                            self.phase = Phase::Work;
                            self.seconds_remaining = self.work_duration;
                            self.running = true;
                            set_timeout(1.0);
                        }
                        Some("stop") => {
                            self.running = false;
                            self.phase = Phase::Finished;
                        }
                        _ => {}
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        match self.phase {
            Phase::Work => self.render_work(cols),
            Phase::Break => self.render_break(rows, cols),
            Phase::BreakDone => self.render_break_done(rows, cols),
            Phase::Finished => self.render_finished(cols),
        }
    }
}

impl Pomo {
    fn start_break(&mut self) {
        self.phase = Phase::Break;
        self.seconds_remaining = self.break_duration;
        self.running = true;
        self.spin_idx = 0;
        self.enter_fullscreen();
        set_timeout(1.0);
    }

    fn start_work(&mut self) {
        self.phase = Phase::Work;
        self.seconds_remaining = self.work_duration;
        self.running = true;
        self.exit_fullscreen();
        set_timeout(1.0);
    }

    fn enter_fullscreen(&mut self) {
        if !self.is_fullscreen {
            toggle_pane_id_fullscreen(PaneId::Plugin(self.plugin_id));
            self.is_fullscreen = true;
        }
    }

    fn exit_fullscreen(&mut self) {
        if self.is_fullscreen {
            toggle_pane_id_fullscreen(PaneId::Plugin(self.plugin_id));
            self.is_fullscreen = false;
        }
    }

    // -- rendering --

    fn render_work(&self, cols: usize) {
        let mins = self.seconds_remaining / 60;
        let secs = self.seconds_remaining % 60;
        let icon = if self.running { "🍅" } else { "⏸" };
        let time = format!("{icon} {mins:02}:{secs:02}");
        let help = "SPC:start/pause r:reset";
        let padding = cols.saturating_sub(time.len() + help.len() + 1);
        let line = format!("{time}{:>width$}", help, width = padding + help.len());
        print_text_with_coordinates(
            Text::new(&line).color_range(0, 0..time.len()),
            0, 0, Some(cols), None,
        );
    }

    fn render_finished(&self, cols: usize) {
        let msg = "✅ Done!";
        let help = "r:new session";
        let padding = cols.saturating_sub(msg.len() + help.len() + 1);
        let line = format!("{msg}{:>width$}", help, width = padding + help.len());
        print_text_with_coordinates(Text::new(&line), 0, 0, Some(cols), None);
    }

    fn render_break(&self, rows: usize, cols: usize) {
        if rows < 3 || cols < 20 {
            let mins = self.seconds_remaining / 60;
            let secs = self.seconds_remaining % 60;
            let line = format!("🍩 BREAK {mins:02}:{secs:02}");
            print_text_with_coordinates(
                Text::new(&line).color_range(0, 0..line.len()),
                0, 0, Some(cols), None,
            );
            return;
        }

        let title = "🍩 BREAK TIME";
        let title_x = cols.saturating_sub(title.len()) / 2;
        let title_y = (rows / 2).saturating_sub(6);
        print_text_with_coordinates(
            Text::new(title).color_range(0, 0..title.len()),
            title_x,
            title_y,
            None,
            None,
        );

        self.render_big_time(rows, cols);
        self.render_progress_bar(self.break_duration, rows, cols);
        self.render_spinner(rows, cols);
    }

    fn render_break_done(&self, rows: usize, cols: usize) {
        if rows < 3 || cols < 20 {
            let line = "BREAK OVER  Another? [Y/n]";
            print_text_with_coordinates(
                Text::new(line).color_range(0, 0..line.len()),
                0, 0, Some(cols), None,
            );
            return;
        }

        let title = "🍅 BREAK OVER";
        let title_x = cols.saturating_sub(title.len()) / 2;
        let title_y = (rows / 2).saturating_sub(3);
        print_text_with_coordinates(
            Text::new(title).color_range(0, 0..title.len()),
            title_x,
            title_y,
            None,
            None,
        );

        let bar_width = cols.saturating_sub(4);
        let bar: String = (0..bar_width).map(|_| '━').collect();
        print_text_with_coordinates(
            Text::new(&bar).color_range(0, 0..bar_width),
            2,
            (rows / 2).saturating_sub(1),
            None,
            None,
        );

        let prompt = "Another? [Y/n]";
        let prompt_x = cols.saturating_sub(prompt.len()) / 2;
        print_text_with_coordinates(
            Text::new(prompt).color_range(1, 0..prompt.len()),
            prompt_x,
            rows / 2 + 2,
            None,
            None,
        );
    }

    fn render_big_time(&self, rows: usize, cols: usize) {
        let mins = self.seconds_remaining / 60;
        let secs = self.seconds_remaining % 60;
        let digits = [mins / 10, mins % 10, 10, secs / 10, secs % 10];

        let total_width = 19;
        let start_x = cols.saturating_sub(total_width) / 2;
        let start_y = (rows / 2).saturating_sub(2);

        for row in 0..5 {
            let mut line = String::new();
            for (i, &d) in digits.iter().enumerate() {
                if i > 0 {
                    line.push(' ');
                }
                let pattern = DIGIT_PATTERNS[d][row];
                for ch in pattern.chars() {
                    line.push(if ch == '1' { '⣿' } else { ' ' });
                }
            }
            print_text_with_coordinates(
                Text::new(&line),
                start_x,
                start_y + row,
                None,
                None,
            );
        }
    }

    fn render_progress_bar(&self, total: usize, rows: usize, cols: usize) {
        let elapsed = total.saturating_sub(self.seconds_remaining);
        let bar_width = cols.saturating_sub(4);
        let filled = if total > 0 {
            ((elapsed as f64 / total as f64) * bar_width as f64).round() as usize
        } else {
            bar_width
        };
        let filled = filled.min(bar_width);

        let mut bar = String::with_capacity(bar_width);
        for i in 0..bar_width {
            bar.push(if i < filled { '━' } else { '─' });
        }

        let bar_y = rows / 2 + 4;
        print_text_with_coordinates(
            Text::new(&bar).color_range(0, 0..filled),
            2,
            bar_y,
            None,
            None,
        );
    }

    fn render_spinner(&self, rows: usize, cols: usize) {
        let ch = SPINNER[self.spin_idx % SPINNER.len()];
        print_text_with_coordinates(
            Text::new(ch.to_string()).color_range(1, 0..1),
            cols / 2,
            rows / 2 + 6,
            None,
            None,
        );
    }
}
