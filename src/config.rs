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
    pub tray_icon: bool,
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
            tray_icon: true,
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

    /// Spawns the tray process (a no-op if it's disabled or already running).
    pub fn spawn_tray() {
        let exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("cosmic-doro-tray")))
            .filter(|p| p.exists())
            .unwrap_or_else(|| "cosmic-doro-tray".into());
        if let Ok(mut child) = std::process::Command::new(exe).spawn() {
            // Reap it so a tray that exits (already running / disabled) leaves no zombie.
            std::thread::spawn(move || child.wait());
        }
    }

    fn autostart_path() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"))
            .join("autostart")
            .join(format!("{}.Tray.desktop", crate::APP_ID))
    }

    pub fn tray_autostart() -> bool {
        Self::autostart_path().exists()
    }

    /// Adds or removes the login autostart entry for the tray.
    pub fn set_tray_autostart(on: bool) {
        let path = Self::autostart_path();
        if !on {
            let _ = std::fs::remove_file(path);
            return;
        }
        let exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("cosmic-doro-tray")))
            .filter(|p| p.exists())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "cosmic-doro-tray".into());
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=Cosmic Doro (tray)\nExec={exe}\nIcon={}\nNoDisplay=true\nX-GNOME-Autostart-enabled=true\n",
            crate::APP_ID
        );
        let _ = crate::paths::write_atomic(&path, entry.as_bytes());
    }

    pub fn save(&self) {
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let _ = crate::paths::write_atomic(&Self::path(), &bytes);
        }
    }
}
