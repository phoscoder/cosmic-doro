//! Desktop notifications via the freedesktop notification spec, which COSMIC's
//! notification daemon (cosmic-notifications) implements.

use crate::config::Settings;
use crate::engine::{self, Completion, Phase, TimerState};
use std::sync::atomic::{AtomicU32, Ordering};
use notify_rust::{Hint, Notification, Timeout, Urgency};

pub fn phase_complete(c: &Completion, settings: &Settings) {
    let today = crate::history::today_focus_count();
    let goal = settings.daily_goal;
    let next_min = c.next.minutes(settings);

    let (summary, body, action) = match c.finished {
        Phase::Focus => {
            let kind = if c.next == Phase::LongBreak { "long break" } else { "short break" };
            let goal_line = if today >= goal as usize && goal > 0 {
                format!("🎯 Daily goal reached: {today}/{goal} sessions!")
            } else {
                format!("{today}/{goal} sessions today.")
            };
            (
                "🍅 Focus session complete".to_string(),
                format!("Nice work. Time for a {kind} ({next_min} min).\n{goal_line}"),
                "Start break",
            )
        }
        _ => (
            "🚀 Break's over".to_string(),
            format!(
                "Ready for round {} of {}? ({next_min} min focus)",
                c.state.round,
                settings.rounds.max(1)
            ),
            "Start focus",
        ),
    };

    let auto = c.auto_started;
    std::thread::spawn(move || {
        let mut n = Notification::new();
        n.appname(crate::APP_NAME)
            .summary(&summary)
            .body(&body)
            .icon(crate::APP_ID)
            .hint(Hint::DesktopEntry(crate::APP_ID.to_string()))
            .hint(Hint::Category("im.received".to_string()))
            .urgency(Urgency::Normal)
            .timeout(Timeout::Milliseconds(10_000));
        // We play the chime ourselves (see `play`), so keep the daemon quiet.
        n.hint(Hint::SuppressSound(true));
        if !auto {
            n.action("start", action);
        }
        let Ok(handle) = n.show() else { return };
        if !auto {
            handle.wait_for_action(|a| {
                if a == "start" && !engine::TimerState::load().is_running() {
                    engine::apply(engine::Command::Toggle);
                }
            });
        }
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerEvent {
    Started,
    Paused,
    Resumed,
}

/// Id of the last start/pause/resume notification, so each one replaces the
/// previous instead of stacking up.
static LAST_EVENT_ID: AtomicU32 = AtomicU32::new(0);

/// Tone + short notification for a start, pause or resume click.
pub fn timer_event(event: TimerEvent, state: &TimerState, settings: &Settings) {
    if settings.notification_sound {
        play(match event {
            TimerEvent::Paused => Tone::Pause,
            _ => Tone::Start,
        });
    }
    if !settings.notifications {
        return;
    }

    let now = engine::now_ms();
    let left = crate::format_clock(state.remaining_ms(settings, now));
    let ends_at = chrono::DateTime::from_timestamp_millis(now + state.remaining_ms(settings, now))
        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default();
    let phase = state.phase.title();
    let round = match state.phase {
        Phase::Focus => format!(" · round {} of {}", state.round, settings.rounds.max(1)),
        _ => String::new(),
    };
    let (summary, body, action) = match event {
        TimerEvent::Started => (
            format!("▶ {phase} started"),
            format!("{left}{round} · ends at {ends_at}"),
            Some(("pause", "Pause")),
        ),
        TimerEvent::Resumed => (
            format!("▶ {phase} resumed"),
            format!("{left} left{round} · ends at {ends_at}"),
            Some(("pause", "Pause")),
        ),
        TimerEvent::Paused => (
            format!("⏸ {phase} paused"),
            format!("{left} left{round}"),
            Some(("resume", "Resume")),
        ),
    };

    std::thread::spawn(move || {
        let mut n = Notification::new();
        n.appname(crate::APP_NAME)
            .summary(&summary)
            .body(&body)
            .icon(crate::APP_ID)
            .hint(Hint::DesktopEntry(crate::APP_ID.to_string()))
            .hint(Hint::Transient(true))
            .hint(Hint::SuppressSound(true))
            .urgency(Urgency::Low)
            .timeout(Timeout::Milliseconds(4_000));
        let last = LAST_EVENT_ID.load(Ordering::Relaxed);
        if last != 0 {
            n.id(last);
        }
        if let Some((key, label)) = action {
            n.action(key, label);
        }
        let Ok(handle) = n.show() else { return };
        LAST_EVENT_ID.store(handle.id(), Ordering::Relaxed);
        handle.wait_for_action(|a| {
            let st = TimerState::load();
            let wants = match a {
                "pause" => st.is_running(),
                "resume" => matches!(st.status, engine::Status::Paused { .. }),
                _ => false,
            };
            if wants {
                engine::apply(engine::Command::Toggle);
            }
        });
    });
}

/// Cosmic Doro's own tones, bundled in the binary (see `res/sounds/generate.sh`).
/// They're mastered much louder than the desktop's subtle UI sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Start,
    Pause,
    FocusEnd,
    BreakEnd,
}

impl Tone {
    pub fn for_finished(phase: Phase) -> Self {
        if phase == Phase::Focus { Tone::FocusEnd } else { Tone::BreakEnd }
    }

    fn file(self) -> (&'static str, &'static [u8]) {
        match self {
            Tone::Start => ("start.oga", include_bytes!("../res/sounds/start.oga")),
            Tone::Pause => ("pause.oga", include_bytes!("../res/sounds/pause.oga")),
            Tone::FocusEnd => ("focus-end.oga", include_bytes!("../res/sounds/focus-end.oga")),
            Tone::BreakEnd => ("break-end.oga", include_bytes!("../res/sounds/break-end.oga")),
        }
    }

    /// Writes the tone to the cache dir (once) so external players can read it.
    fn path(self) -> Option<std::path::PathBuf> {
        let (name, bytes) = self.file();
        let dir = std::env::var_os("XDG_CACHE_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache"))
            .join("cosmic-doro")
            .join(format!("sounds-{}", env!("CARGO_PKG_VERSION")));
        let path = dir.join(name);
        let fresh = std::fs::metadata(&path).is_ok_and(|m| m.len() == bytes.len() as u64);
        if !fresh {
            crate::paths::write_atomic(&path, bytes).ok()?;
        }
        Some(path)
    }
}

/// Plays a tone at the configured volume through PipeWire/PulseAudio without
/// blocking. Returns false if no player could be started.
pub fn play(tone: Tone) -> bool {
    let Some(path) = tone.path() else { return false };
    let volume = Settings::load().sound_volume.min(100) as f32 / 100.0;
    let players: [(&str, Vec<String>); 3] = [
        ("pw-play", vec![format!("--volume={volume:.2}")]),
        ("paplay", vec![format!("--volume={}", (volume * 65536.0) as u32)]),
        ("canberra-gtk-play", vec!["-f".into()]),
    ];
    for (bin, args) in players {
        if let Ok(mut child) = std::process::Command::new(bin)
            .args(&args)
            .arg(&path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            std::thread::spawn(move || child.wait());
            return true;
        }
    }
    false
}
