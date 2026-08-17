use pomo::{Countdown, Phase, PipeCommand};
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

#[derive(Default)]
struct Pomo {
    countdown: Countdown,
    work_duration: usize,
    break_duration: usize,
    phase: Phase,
    spin_idx: usize,
    timer_pending: bool,
}

fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

impl Pomo {
    // set_timeout chains stack: every call spawns another once-per-second
    // chain that re-arms itself, making the countdown tick N times faster.
    fn arm_timer(&mut self) {
        if !self.timer_pending {
            self.timer_pending = true;
            set_timeout(1.0);
        }
    }

    fn begin(&mut self, secs: usize) {
        self.countdown.begin(secs, now_secs());
        self.arm_timer();
    }
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
        subscribe(&[
            EventType::Timer,
            EventType::Key,
            EventType::PermissionRequestResult,
        ]);
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::ReadCliPipes,
        ]);

        // Auto-start the work timer. Hiding waits for PermissionRequestResult —
        // hide_self() fails silently before permissions are granted.
        self.begin(self.work_duration);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(PermissionStatus::Granted) => {
                // Now that hide_self() can succeed, stay hidden during work —
                // pane size can't be controlled from the plugin, so we only
                // show for the break overlay.
                if self.phase == Phase::Work {
                    hide_self();
                }
                true
            }
            Event::Timer(_) => {
                self.timer_pending = false;
                if self.countdown.running {
                    self.spin_idx = (self.spin_idx + 1) % SPINNER.len();
                    if self.countdown.tick(now_secs()) {
                        match self.phase {
                            Phase::Work => self.start_break(),
                            Phase::Break => self.start_work(),
                            _ => {}
                        }
                    } else {
                        self.arm_timer();
                    }
                }
                true
            }
            Event::Key(key) => {
                match self.phase {
                    Phase::Work => match key.bare_key {
                        BareKey::Char(' ') => {
                            if self.countdown.running {
                                self.countdown.pause();
                            } else {
                                // Re-anchor the deadline to the paused remainder
                                self.begin(self.countdown.seconds_remaining);
                            }
                        }
                        BareKey::Char('r') => {
                            self.countdown.pause();
                            self.countdown.seconds_remaining = self.work_duration;
                        }
                        BareKey::Char('h') => {
                            hide_self();
                        }
                        _ => return false,
                    },
                    Phase::Break => return false,
                    Phase::Finished => match key.bare_key {
                        BareKey::Char('r') => {
                            self.phase = Phase::Work;
                            self.countdown.pause();
                            self.countdown.seconds_remaining = self.work_duration;
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
        // CLI pipes block the sending terminal until explicitly unblocked.
        if let PipeSource::Cli(pipe_id) = &pipe_message.source {
            unblock_cli_pipe_input(pipe_id);
        }
        match pipe_message.name.as_str() {
            "pomo" => {
                if let Some(payload) = &pipe_message.payload {
                    match PipeCommand::parse(payload) {
                        Some(PipeCommand::Start { work, brk }) => {
                            if let Some(work) = work {
                                self.work_duration = work;
                            }
                            if let Some(brk) = brk {
                                self.break_duration = brk;
                            }
                            self.phase = Phase::Work;
                            self.begin(self.work_duration);
                        }
                        Some(PipeCommand::Stop) => {
                            self.countdown.pause();
                            self.phase = Phase::Finished;
                        }
                        Some(PipeCommand::Hide) => {
                            hide_self();
                        }
                        Some(PipeCommand::Show) => {
                            show_self(false);
                        }
                        None => {}
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
            Phase::Finished => self.render_finished(cols),
        }
    }
}

impl Pomo {
    fn start_break(&mut self) {
        self.phase = Phase::Break;
        self.spin_idx = 0;
        show_self(true);
        self.begin(self.break_duration);
    }

    fn start_work(&mut self) {
        self.phase = Phase::Work;
        hide_self();
        self.begin(self.work_duration);
    }


    // -- rendering --

    fn render_work(&self, cols: usize) {
        let mins = self.countdown.seconds_remaining / 60;
        let secs = self.countdown.seconds_remaining % 60;
        let icon = if self.countdown.running { "🍅" } else { "⏸" };
        let time = format!("{icon} {mins:02}:{secs:02}");
        let help = "SPC:start/pause r:reset";
        let padding = cols.saturating_sub(time.len() + help.len() + 1);
        let line = format!("{time}{:>width$}", help, width = padding + help.len());
        print_text_with_coordinates(
            Text::new(&line).color_range(0, 0..time.len()),
            1, 0, Some(cols.saturating_sub(1)), None,
        );
    }

    fn render_finished(&self, cols: usize) {
        let msg = "✅ Done!";
        let help = "r:new session";
        let padding = cols.saturating_sub(msg.len() + help.len() + 1);
        let line = format!("{msg}{:>width$}", help, width = padding + help.len());
        print_text_with_coordinates(Text::new(&line), 1, 0, Some(cols.saturating_sub(1)), None);
    }

    fn render_break(&self, rows: usize, cols: usize) {
        if rows < 3 || cols < 20 {
            let mins = self.countdown.seconds_remaining / 60;
            let secs = self.countdown.seconds_remaining % 60;
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


    fn render_big_time(&self, rows: usize, cols: usize) {
        let mins = self.countdown.seconds_remaining / 60;
        let secs = self.countdown.seconds_remaining % 60;
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
        let elapsed = total.saturating_sub(self.countdown.seconds_remaining);
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
