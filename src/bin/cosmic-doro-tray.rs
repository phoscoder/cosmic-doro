// Cosmic Doro — system tray icon (StatusNotifierItem, shown by COSMIC's
// Status Area applet). Also keeps the timer and notifications running while
// the main window is closed.

use doro::config::Settings;
use doro::engine::{self, Command, Phase, Status, TimerState};
use doro::{format_clock, todo};
use ksni::blocking::TrayMethods;
use ksni::menu::{StandardItem, SubMenu};
use ksni::{Icon, MenuItem, ToolTip};
use std::time::Duration;

struct DoroTray {
    timer: TimerState,
    settings: Settings,
    now: i64,
    tasks: Vec<todo::Todo>,
    quit: bool,
}

fn open_window(page: Option<&str>) {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("cosmic-doro")))
        .filter(|p| p.exists())
        .unwrap_or_else(|| "cosmic-doro".into());
    let mut cmd = std::process::Command::new(exe);
    if let Some(p) = page {
        cmd.arg(p);
    }
    let _ = cmd.spawn();
}

fn rgb(phase: Phase) -> [u8; 3] {
    match phase {
        Phase::Focus => [0xf2, 0x49, 0x3b],
        Phase::ShortBreak => [0x3c, 0xc8, 0x7a],
        Phase::LongBreak => [0xa7, 0x7b, 0xf3],
    }
}

/// Renders the progress ring as ARGB32 (network byte order) pixels.
fn ring_icon(size: i32, phase: Phase, remaining: f32, status: Status) -> Icon {
    let s = size as f32;
    let c = s / 2.0;
    let r_out = s * 0.47;
    let r_in = s * 0.30;
    let [pr, pg, pb] = rgb(phase);
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let d = (dx * dx + dy * dy).sqrt();
            // Angle clockwise from 12 o'clock, 0..1.
            let a = (dx.atan2(-dy) / std::f32::consts::TAU).rem_euclid(1.0);
            let ring_cov = ((r_out - d + 0.5).clamp(0.0, 1.0)) * ((d - r_in + 0.5).clamp(0.0, 1.0));
            let mut px = [0u8, 0, 0, 0];
            if ring_cov > 0.0 {
                // Remaining arc counts down from 12 o'clock clockwise.
                let lit = a >= 1.0 - remaining || status == Status::Idle;
                let (alpha, col) = if lit { (1.0, [pr, pg, pb]) } else { (0.35, [0x9a, 0x9a, 0xa8]) };
                px = [(255.0 * alpha * ring_cov) as u8, col[0], col[1], col[2]];
            }
            // Center glyph: dot (idle/running) or pause bars.
            let center = match status {
                Status::Paused { .. } => {
                    let bar = |bx: f32| (dx - bx).abs() < s * 0.05 && dy.abs() < s * 0.13;
                    if bar(-s * 0.08) || bar(s * 0.08) { 1.0 } else { 0.0 }
                }
                _ => (s * 0.12 - d + 0.5).clamp(0.0, 1.0),
            };
            if center > 0.0 {
                px = [(255.0 * center) as u8, pr, pg, pb];
            }
            data.extend_from_slice(&px);
        }
    }
    Icon { width: size, height: size, data }
}

impl DoroTray {
    fn refresh(&mut self) {
        let (timer, _) = engine::tick();
        self.timer = timer;
        self.settings = Settings::load();
        self.now = engine::now_ms();
        self.tasks = todo::load().into_iter().filter(|t| !t.done).collect();
    }

    fn remaining(&self) -> String {
        format_clock(self.timer.remaining_ms(&self.settings, self.now))
    }

    fn act(&mut self, cmd: Command) {
        self.timer = engine::apply(cmd);
        self.now = engine::now_ms();
    }
}

fn item(label: impl Into<String>, icon: &str, f: impl Fn(&mut DoroTray) + Send + 'static) -> MenuItem<DoroTray> {
    StandardItem { label: label.into(), icon_name: icon.into(), activate: Box::new(f), ..Default::default() }.into()
}

impl ksni::Tray for DoroTray {
    fn id(&self) -> String {
        "cosmic-doro".into()
    }

    fn title(&self) -> String {
        doro::APP_NAME.into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        let remaining = 1.0 - self.timer.progress(&self.settings, self.now);
        [22, 32, 48, 64]
            .into_iter()
            .map(|s| ring_icon(s, self.timer.phase, remaining, self.timer.status))
            .collect()
    }

    fn tool_tip(&self) -> ToolTip {
        let t = &self.timer;
        let state = match t.status {
            Status::Running { .. } => "left",
            Status::Paused { .. } => "left · paused",
            Status::Idle => "· ready",
        };
        ToolTip {
            title: format!("{} {} {state}", t.phase.title(), self.remaining()),
            description: match t.task_id {
                Some(_) if !t.label.is_empty() => format!("Focusing on: {}", t.label),
                _ => format!("Round {} of {}", t.round, self.settings.rounds.max(1)),
            },
            icon_name: doro::APP_ID.into(),
            icon_pixmap: Vec::new(),
        }
    }

    /// Left click: open the main window.
    fn activate(&mut self, _x: i32, _y: i32) {
        open_window(None);
    }

    /// Middle click: start / pause.
    fn secondary_activate(&mut self, _x: i32, _y: i32) {
        self.act(Command::Toggle);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let t = &self.timer;
        let (play_label, play_icon) = match t.status {
            Status::Running { .. } => ("Pause", "media-playback-pause-symbolic"),
            Status::Paused { .. } => ("Resume", "media-playback-start-symbolic"),
            Status::Idle => ("Start", "media-playback-start-symbolic"),
        };
        let header = match t.status {
            Status::Idle => format!("{} · {} · ready", t.phase.title(), self.remaining()),
            Status::Paused { .. } => format!("{} · {} · paused", t.phase.title(), self.remaining()),
            Status::Running { .. } => format!("{} · {} left", t.phase.title(), self.remaining()),
        };

        let mut tasks: Vec<MenuItem<Self>> = self
            .tasks
            .iter()
            .take(10)
            .map(|task| {
                let (id, title) = (task.id, task.title.clone());
                let mark = if t.task_id == Some(id) { "● " } else { "" };
                item(
                    format!("{mark}{title}  ({}/{} 🍅)", task.pomodoros, task.estimate),
                    "",
                    move |this| this.act(Command::SetTask(Some((id, title.clone())))),
                )
            })
            .collect();
        if tasks.is_empty() {
            tasks.push(StandardItem { label: "No open tasks".into(), enabled: false, ..Default::default() }.into());
        }
        if t.task_id.is_some() {
            tasks.push(MenuItem::Separator);
            tasks.push(item("Stop focusing on task", "window-close-symbolic", |this| {
                this.act(Command::SetTask(None))
            }));
        }
        tasks.push(MenuItem::Separator);
        tasks.push(item("Manage tasks…", "checkbox-checked-symbolic", |_| open_window(Some("--tasks"))));

        let mut menu = vec![StandardItem { label: header, enabled: false, ..Default::default() }.into()];
        if let Some(_) = t.task_id.filter(|_| !t.label.is_empty()) {
            menu.push(StandardItem { label: format!("🎯 {}", t.label), enabled: false, ..Default::default() }.into());
        }
        menu.extend([
            MenuItem::Separator,
            item(play_label, play_icon, |this| this.act(Command::Toggle)),
            item("Skip to next phase", "media-skip-forward-symbolic", |this| this.act(Command::Skip)),
            item("Restart this phase", "edit-undo-symbolic", |this| this.act(Command::Reset)),
            item("Reset pomodoro", "view-refresh-symbolic", |this| this.act(Command::ResetCycle)),
            MenuItem::Separator,
            SubMenu { label: "Focus on task".into(), icon_name: "task-due-symbolic".into(), submenu: tasks, ..Default::default() }
                .into(),
            MenuItem::Separator,
            item("Open Cosmic Doro", "accessories-clock-symbolic", |_| open_window(None)),
            item("Reports", "utilities-system-monitor-symbolic", |_| open_window(Some("--reports"))),
            item("Settings", "preferences-system-symbolic", |_| open_window(Some("--settings"))),
            MenuItem::Separator,
            item("Hide tray icon", "window-close-symbolic", |this| {
                let mut s = Settings::load();
                s.tray_icon = false;
                s.save();
                this.quit = true;
            }),
        ]);
        menu
    }
}

fn main() {
    if !Settings::load().tray_icon {
        return;
    }
    // Single instance: hold an exclusive lock for the life of the process.
    let dir = doro::paths::state_dir();
    let _ = std::fs::create_dir_all(&dir);
    let Ok(lock) = std::fs::File::create(dir.join("tray.lock")) else { return };
    if lock.try_lock().is_err() {
        return;
    }

    let mut tray = DoroTray {
        timer: TimerState::default(),
        settings: Settings::default(),
        now: 0,
        tasks: Vec::new(),
        quit: false,
    };
    tray.refresh();
    let handle = match tray.spawn() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("cosmic-doro-tray: could not register tray icon: {e}");
            return;
        }
    };

    loop {
        std::thread::sleep(Duration::from_secs(1));
        let quit = handle.update(|t| {
            t.refresh();
            t.quit || !t.settings.tray_icon
        });
        if quit != Some(false) {
            handle.shutdown().wait();
            break;
        }
    }
    drop(lock);
}
