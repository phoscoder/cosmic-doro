//! Desktop notifications via the freedesktop notification spec, which COSMIC's
//! notification daemon (cosmic-notifications) implements.

use crate::config::Settings;
use crate::engine::{self, Completion, Phase};
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
    let Some(file) = sound_candidates(phase_finished)
        .into_iter()
        .find(|f| std::path::Path::new(f).exists())
    else {
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
