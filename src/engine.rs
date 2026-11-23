//! The timer engine. State lives in `$XDG_STATE_HOME/cosmic-doro/timer.json`
//! and is stored as absolute timestamps, so any process (window or applet)
//! can read it and show the same countdown. When a phase expires, the first
//! process to atomically create a claim file for that phase's sequence number
//! completes it: logs history, advances the cycle and sends the notification.

use crate::config::Settings;
use crate::history::{self, Session};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Focus,
    ShortBreak,
    LongBreak,
}

impl Phase {
    pub fn title(self) -> &'static str {
        match self {
            Phase::Focus => "Focus",
            Phase::ShortBreak => "Short break",
            Phase::LongBreak => "Long break",
        }
    }

    pub fn minutes(self, s: &Settings) -> u32 {
        match self {
            Phase::Focus => s.focus_minutes,
            Phase::ShortBreak => s.short_break_minutes,
            Phase::LongBreak => s.long_break_minutes,
        }
        .max(1)
    }

    pub fn is_break(self) -> bool {
        self != Phase::Focus
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Status {
    Idle,
    Running { ends_at: i64 },
    Paused { remaining_ms: i64 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct TimerState {
    pub phase: Phase,
    /// 1-based focus round inside the current cycle.
    pub round: u32,
    pub status: Status,
    /// Bumped on every phase transition; used to claim completions.
    pub seq: u64,
    /// Length of the phase as it was started (settings may change mid-phase).
    pub duration_ms: i64,
    pub started_at: Option<i64>,
    pub label: String,
    /// Task (from the Tasks page) this focus is attributed to.
    pub task_id: Option<u64>,
}

impl Default for TimerState {
    fn default() -> Self {
        Self {
            phase: Phase::Focus,
            round: 1,
            status: Status::Idle,
            seq: 0,
            duration_ms: 0,
            started_at: None,
            label: String::new(),
            task_id: None,
        }
    }
}

impl TimerState {
    fn path() -> PathBuf {
        crate::paths::state_dir().join("timer.json")
    }

    pub fn file_mtime() -> Option<std::time::SystemTime> {
        crate::paths::mtime(&Self::path())
    }

    pub fn load() -> Self {
        std::fs::read(Self::path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let _ = crate::paths::write_atomic(&Self::path(), &bytes);
        }
    }

    pub fn total_ms(&self, s: &Settings) -> i64 {
        match self.status {
            Status::Idle => i64::from(self.phase.minutes(s)) * 60_000,
            _ if self.duration_ms > 0 => self.duration_ms,
            _ => i64::from(self.phase.minutes(s)) * 60_000,
        }
    }

    pub fn remaining_ms(&self, s: &Settings, now: i64) -> i64 {
        match self.status {
            Status::Idle => self.total_ms(s),
            Status::Running { ends_at } => (ends_at - now).max(0),
            Status::Paused { remaining_ms } => remaining_ms.max(0),
        }
    }

    /// Elapsed fraction of the phase in `0.0..=1.0`.
    pub fn progress(&self, s: &Settings, now: i64) -> f32 {
        let total = self.total_ms(s).max(1) as f32;
        (1.0 - self.remaining_ms(s, now) as f32 / total).clamp(0.0, 1.0)
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status, Status::Running { .. })
    }

    pub fn display_label(&self) -> &str {
        if self.label.trim().is_empty() {
            self.phase.title()
        } else {
            self.label.trim()
        }
    }

    fn start(&mut self, s: &Settings, now: i64) {
        self.duration_ms = i64::from(self.phase.minutes(s)) * 60_000;
        self.status = Status::Running { ends_at: now + self.duration_ms };
        self.started_at = Some(now);
    }

    /// Moves to the next phase of the cycle (does not start it).
    fn advance(&mut self, s: &Settings) {
        match self.phase {
            Phase::Focus if self.round >= s.rounds.max(1) => self.phase = Phase::LongBreak,
            Phase::Focus => self.phase = Phase::ShortBreak,
            Phase::ShortBreak => {
                self.phase = Phase::Focus;
                self.round += 1;
            }
            Phase::LongBreak => {
                self.phase = Phase::Focus;
                self.round = 1;
            }
        }
        self.status = Status::Idle;
        self.started_at = None;
        self.duration_ms = 0;
        self.seq += 1;
    }
}

#[derive(Clone, Debug)]
pub enum Command {
    Toggle,
    Reset,
    Skip,
    ResetCycle,
    /// Focus on a task (or clear with `None`); the task title becomes the label.
    SetTask(Option<(u64, String)>),
}

/// Applies a user command to the shared state and returns the new state.
pub fn apply(cmd: Command) -> TimerState {
    let settings = Settings::load();
    let mut state = TimerState::load();
    let now = now_ms();
    match cmd {
        Command::Toggle => match state.status {
            Status::Idle => state.start(&settings, now),
            Status::Running { ends_at } => {
                state.status = Status::Paused { remaining_ms: (ends_at - now).max(0) }
            }
            Status::Paused { remaining_ms } => {
                state.status = Status::Running { ends_at: now + remaining_ms }
            }
        },
        Command::Reset => {
            state.status = Status::Idle;
            state.started_at = None;
            state.duration_ms = 0;
        }
        Command::Skip => {
            let was_running = state.is_running();
            state.advance(&settings);
            if was_running {
                state.start(&settings, now);
            }
        }
        Command::ResetCycle => {
            let label = std::mem::take(&mut state.label);
            let seq = state.seq + 1;
            state = TimerState { label, seq, task_id: state.task_id, ..TimerState::default() };
        }
        Command::SetTask(task) => {
            state.task_id = task.as_ref().map(|t| t.0);
            state.label = task.map(|t| t.1).unwrap_or_default();
        }
    }
    state.save();
    state
}

#[derive(Clone, Debug)]
pub struct Completion {
    pub finished: Phase,
    pub next: Phase,
    pub auto_started: bool,
    pub state: TimerState,
}

fn claim(seq: u64) -> bool {
    let dir = crate::paths::state_dir().join("claims");
    let _ = std::fs::create_dir_all(&dir);
    let ok = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(seq.to_string()))
        .is_ok();
    if ok {
        // Keep a handful of recent claims so slow readers can't re-claim.
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let old = e.file_name().to_str().and_then(|n| n.parse::<u64>().ok());
                if old.is_some_and(|n| n + 8 < seq) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
    ok
}

/// Reloads shared state and completes the current phase if it has expired.
/// Returns the fresh state and, if this process completed a phase, the details.
pub fn tick() -> (TimerState, Option<Completion>) {
    let state = TimerState::load();
    let now = now_ms();
    let Status::Running { ends_at } = state.status else {
        return (state, None);
    };
    if now < ends_at || !claim(state.seq) {
        return (state, None);
    }
    // Re-check after claiming: another process may have moved on already.
    let mut state = TimerState::load();
    if !matches!(state.status, Status::Running { ends_at: e } if e == ends_at) {
        return (state, None);
    }

    let settings = Settings::load();
    let finished = state.phase;
    let duration = state.total_ms(&settings);
    history::append(&Session {
        start: state.started_at.unwrap_or(ends_at - duration),
        end: ends_at,
        phase: finished,
        seconds: duration / 1000,
        label: state.label.trim().to_string(),
    });
    if finished == Phase::Focus {
        if let Some(id) = state.task_id {
            crate::todo::add_pomodoro(id);
        }
    }

    state.advance(&settings);
    let auto = if state.phase.is_break() {
        settings.auto_start_breaks
    } else {
        settings.auto_start_focus
    };
    if auto {
        state.start(&settings, now);
    }
    state.save();

    let completion = Completion { finished, next: state.phase, auto_started: auto, state: state.clone() };
    if settings.notifications {
        crate::notify::phase_complete(&completion, &settings);
    }
    (state, Some(completion))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_cycle_with_claims() {
        let dir = std::env::temp_dir().join(format!("cosmic-doro-test-{}", std::process::id()));
        // SAFETY: single test touching the environment in this binary.
        unsafe {
            std::env::set_var("XDG_STATE_HOME", dir.join("state"));
            std::env::set_var("XDG_DATA_HOME", dir.join("data"));
            std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
        }
        let s = Settings { rounds: 2, auto_start_breaks: false, notifications: false, ..Settings::default() };
        s.save();

        let expire = || {
            let mut st = TimerState::load();
            if let Status::Running { ends_at } = st.status {
                st.status = Status::Running { ends_at: now_ms() - 1 };
                st.started_at = Some(ends_at - st.duration_ms);
                st.save();
            }
        };

        let st = apply(Command::Toggle);
        assert!(st.is_running());
        assert!((24 * 60_000..=25 * 60_000).contains(&st.remaining_ms(&s, now_ms())));

        expire();
        let (st, done) = tick();
        let done = done.expect("phase should complete");
        assert_eq!(done.finished, Phase::Focus);
        assert_eq!(st.phase, Phase::ShortBreak);
        assert!(!st.is_running(), "breaks don't auto-start with this config");

        // A second process ticking now must not complete anything again.
        assert!(tick().1.is_none());
        assert!(!claim(st.seq - 1), "old claim must be kept");

        // Break -> round 2 focus -> long break -> round 1.
        apply(Command::Toggle);
        expire();
        let (st, _) = tick();
        assert_eq!((st.phase, st.round), (Phase::Focus, 2));
        let st = apply(Command::Skip);
        assert_eq!(st.phase, Phase::LongBreak);
        let st = apply(Command::Skip);
        assert_eq!((st.phase, st.round), (Phase::Focus, 1));

        // Pause keeps remaining time frozen.
        apply(Command::Toggle);
        let p = apply(Command::Toggle);
        assert!(matches!(p.status, Status::Paused { .. }));

        let log = crate::history::load_all();
        assert_eq!(log.len(), 2, "one focus + one break logged, skips are not");
        assert_eq!(log[0].phase, Phase::Focus);
        let _ = std::fs::remove_dir_all(dir);
    }
}
