//! Shared core for Cosmic Doro: settings, the timer engine, session history
//! and desktop notifications. Both the main window and the panel applet use
//! this crate, and they coordinate through small JSON files on disk so either
//! one can drive the timer on its own.

pub mod config;
pub mod engine;
pub mod history;
pub mod notify;
pub mod paths;
pub mod todo;
pub mod ui;

pub const APP_ID: &str = "io.github.phoscoder.CosmicDoro";
pub const APPLET_ID: &str = "io.github.phoscoder.CosmicDoro.Applet";
pub const APP_NAME: &str = "Cosmic Doro";

pub const LOGO_SVG: &[u8] = include_bytes!("../res/icons/io.github.phoscoder.CosmicDoro.svg");
pub const SYMBOLIC_SVG: &[u8] =
    include_bytes!("../res/icons/io.github.phoscoder.CosmicDoro-symbolic.svg");

/// Formats milliseconds as `MM:SS` (or `H:MM:SS` past an hour).
pub fn format_clock(ms: i64) -> String {
    let total = (ms.max(0) + 999) / 1000;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// Formats minutes as a compact human duration like `1h 25m`.
pub fn format_minutes(minutes: i64) -> String {
    let (h, m) = (minutes / 60, minutes % 60);
    match (h, m) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}
