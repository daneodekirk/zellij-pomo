//! Pure timer logic, kept free of Zellij API calls so it can be unit
//! tested natively with `cargo test --lib`.

#[derive(Default, PartialEq, Debug, Clone, Copy)]
pub enum Phase {
    #[default]
    Work,
    Break,
    Finished,
}

/// A countdown anchored to a wall-clock deadline. Remaining time is
/// recomputed from `now` on every tick, so duplicate or missed Timer
/// events can't change the pace.
#[derive(Default)]
pub struct Countdown {
    pub seconds_remaining: usize,
    pub running: bool,
    deadline: f64,
}

impl Countdown {
    pub fn begin(&mut self, secs: usize, now: f64) {
        self.seconds_remaining = secs;
        self.deadline = now + secs as f64;
        self.running = true;
    }

    pub fn pause(&mut self) {
        self.running = false;
    }

    /// Recompute the remaining seconds; returns true when the countdown
    /// just reached zero.
    pub fn tick(&mut self, now: f64) -> bool {
        if !self.running {
            return false;
        }
        self.seconds_remaining = (self.deadline - now).ceil().max(0.0) as usize;
        if self.seconds_remaining == 0 {
            self.running = false;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum PipeCommand {
    Start {
        work: Option<usize>,
        brk: Option<usize>,
    },
    Stop,
    Hide,
    Show,
}

impl PipeCommand {
    pub fn parse(payload: &str) -> Option<Self> {
        let parts: Vec<&str> = payload.trim().split_whitespace().collect();
        match parts.first().copied() {
            Some("start") => Some(PipeCommand::Start {
                work: parts.get(1).and_then(|s| s.parse().ok()),
                brk: parts.get(2).and_then(|s| s.parse().ok()),
            }),
            Some("stop") => Some(PipeCommand::Stop),
            Some("hide") => Some(PipeCommand::Hide),
            Some("show") => Some(PipeCommand::Show),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdown_ticks_down_in_real_seconds() {
        let mut c = Countdown::default();
        c.begin(10, 100.0);
        assert_eq!(c.seconds_remaining, 10);
        assert!(!c.tick(101.0));
        assert_eq!(c.seconds_remaining, 9);
        assert!(!c.tick(105.0));
        assert_eq!(c.seconds_remaining, 5);
    }

    #[test]
    fn duplicate_ticks_do_not_speed_up_the_countdown() {
        // Regression: config-loaded plugins can receive Timer events more
        // than once per second, which used to make the timer count double.
        let mut c = Countdown::default();
        c.begin(10, 100.0);
        c.tick(101.0);
        c.tick(101.0);
        c.tick(101.0);
        assert_eq!(c.seconds_remaining, 9);
    }

    #[test]
    fn sub_second_early_tick_does_not_drop_a_second() {
        let mut c = Countdown::default();
        c.begin(10, 100.0);
        c.tick(100.5);
        assert_eq!(c.seconds_remaining, 10);
    }

    #[test]
    fn countdown_finishes_once_at_zero() {
        let mut c = Countdown::default();
        c.begin(3, 100.0);
        assert!(!c.tick(102.0));
        assert!(c.tick(103.0));
        assert_eq!(c.seconds_remaining, 0);
        assert!(!c.running);
        assert!(!c.tick(104.0));
    }

    #[test]
    fn late_tick_clamps_to_zero() {
        let mut c = Countdown::default();
        c.begin(3, 100.0);
        assert!(c.tick(500.0));
        assert_eq!(c.seconds_remaining, 0);
    }

    #[test]
    fn pause_freezes_and_resume_reanchors() {
        let mut c = Countdown::default();
        c.begin(10, 100.0);
        c.tick(104.0);
        c.pause();
        assert_eq!(c.seconds_remaining, 6);
        assert!(!c.tick(150.0));
        assert_eq!(c.seconds_remaining, 6);
        c.begin(c.seconds_remaining, 200.0);
        c.tick(203.0);
        assert_eq!(c.seconds_remaining, 3);
    }

    #[test]
    fn parse_start_with_and_without_overrides() {
        assert_eq!(
            PipeCommand::parse("start"),
            Some(PipeCommand::Start { work: None, brk: None })
        );
        assert_eq!(
            PipeCommand::parse("start 1500 300"),
            Some(PipeCommand::Start { work: Some(1500), brk: Some(300) })
        );
        assert_eq!(
            PipeCommand::parse("  start 10  "),
            Some(PipeCommand::Start { work: Some(10), brk: None })
        );
        assert_eq!(
            PipeCommand::parse("start abc 5"),
            Some(PipeCommand::Start { work: None, brk: Some(5) })
        );
    }

    #[test]
    fn parse_simple_commands_and_garbage() {
        assert_eq!(PipeCommand::parse("stop"), Some(PipeCommand::Stop));
        assert_eq!(PipeCommand::parse("hide"), Some(PipeCommand::Hide));
        assert_eq!(PipeCommand::parse("show"), Some(PipeCommand::Show));
        assert_eq!(PipeCommand::parse("banana"), None);
        assert_eq!(PipeCommand::parse(""), None);
    }
}
