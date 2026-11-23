use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub focus_minutes: u32,
    pub short_break_minutes: u32,
    pub long_break_minutes: u32,
    /// Focus rounds before a long break.
    pub rounds: u32,
    pub daily_goal: u32,
    pub auto_start_breaks: bool,
    pub auto_start_focus: bool,
    pub notifications: bool,
    pub notification_sound: bool,
    pub applet_show_time: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            focus_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            rounds: 4,
            daily_goal: 8,
            auto_start_breaks: true,
            auto_start_focus: false,
            notifications: true,
            notification_sound: true,
            applet_show_time: true,
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        crate::paths::config_dir().join("settings.json")
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
}
