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
        // We play the chime ourselves (see `play_sound`), so keep the daemon quiet.
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

/// Sound theme files to try, in order: COSMIC's theme first, then freedesktop's.
fn sound_candidates(phase_finished: Phase) -> [&'static str; 4] {
    match phase_finished {
        // Focus done: a bright "complete" chime.
        Phase::Focus => [
            "/usr/share/sounds/COSMIC/stereo/notification/complete.oga",
            "/usr/share/sounds/freedesktop/stereo/complete.oga",
            "/usr/share/sounds/COSMIC/stereo/action/bell.oga",
            "/usr/share/sounds/freedesktop/stereo/bell.oga",
        ],
        // Break over: a bell to call you back.
        _ => [
            "/usr/share/sounds/COSMIC/stereo/action/bell.oga",
            "/usr/share/sounds/freedesktop/stereo/bell.oga",
            "/usr/share/sounds/COSMIC/stereo/notification/complete.oga",
            "/usr/share/sounds/freedesktop/stereo/complete.oga",
        ],
    }
}

/// Plays the end-of-phase chime through PipeWire/PulseAudio without blocking.
/// Returns false if no sound file or player could be found.
pub fn play_sound(phase_finished: Phase) -> bool {
    play_first(&sound_candidates(phase_finished))
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
        match event {
            TimerEvent::Paused => play_pause_sound(),
            _ => play_start_sound(),
        };
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

/// Short rising cue played when the timer starts or resumes.
pub fn play_start_sound() -> bool {
    play_first(&[
        "/usr/share/sounds/COSMIC/stereo/action/count-down.oga",
        "/usr/share/sounds/freedesktop/stereo/message.oga",
        "/usr/share/sounds/COSMIC/stereo/notification/message.oga",
    ])
}

/// A different, softer tone for pausing so the two are easy to tell apart.
pub fn play_pause_sound() -> bool {
    play_first(&[
        "/usr/share/sounds/COSMIC/stereo/action/camera-focus.oga",
        "/usr/share/sounds/COSMIC/stereo/action/audio-volume-change.oga",
        "/usr/share/sounds/freedesktop/stereo/audio-volume-change.oga",
    ])
}

fn play_first(candidates: &[&str]) -> bool {
    let Some(file) = candidates.iter().copied().find(|f| std::path::Path::new(f).exists()) else {
        return false;
    };
    let players: [(&str, &[&str]); 3] = [("pw-play", &[]), ("paplay", &[]), ("canberra-gtk-play", &["-f"])];
    for (bin, args) in players {
        if let Ok(mut child) = std::process::Command::new(bin)
            .args(args)
            .arg(file)
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
