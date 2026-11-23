// Cosmic Doro — main window.

use chrono::{Datelike, Local};
use cosmic::app::{Core, Settings as AppSettings, Task};
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::{Alignment, Length, Size, Subscription};
use cosmic::widget::{self, canvas, icon, nav_bar, settings};
use cosmic::{Application, ApplicationExt, Element, executor, theme};
use doro::config::Settings;
use doro::engine::{self, Command, Phase, Status, TimerState};
use doro::history::{self, Report};
use doro::ui::{self, Bars, Heatmap, Ring};
use doro::{format_clock, format_minutes};
use std::time::{Duration, SystemTime};

fn main() -> cosmic::iced::Result {
    let settings = AppSettings::default()
        .size(Size::new(1040.0, 780.0))
        .size_limits(cosmic::iced::Limits::NONE.min_width(420.0).min_height(560.0));
    let page = std::env::args().nth(1).and_then(|a| a.strip_prefix("--").map(str::to_string));
    // If Cosmic Doro is already running, this hands the request to that
    // instance over D-Bus (raising its window) and exits instead.
    cosmic::app::run_single_instance::<App>(settings, Flags { page })
}

/// Launch flags: an optional page name (`timer`, `reports`, `settings`).
#[derive(Clone, Debug, Default)]
struct Flags {
    page: Option<String>,
}

impl cosmic::app::CosmicFlags for Flags {
    type SubCommand = String;
    type Args = Vec<String>;

    fn action(&self) -> Option<&String> {
        self.page.as_ref()
    }
}

fn page_index(name: &str) -> Option<u16> {
    match name {
        "timer" => Some(0),
        "reports" => Some(1),
        "settings" => Some(2),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Timer,
    Reports,
    Settings,
}

#[derive(Clone, Debug)]
enum Field {
    Focus(u32),
    Short(u32),
    Long(u32),
    Rounds(u32),
    Goal(u32),
    AutoBreaks(bool),
    AutoFocus(bool),
    Notifications(bool),
    Sound(bool),
    PanelTime(bool),
    Tray(bool),
    TrayAutostart(bool),
}

#[derive(Clone, Debug)]
enum Message {
    Tick,
    Timer(Command),
    Set(Field),
    TestSound,
    ClearHistory,
    ConfirmClear(bool),
}

struct App {
    core: Core,
    nav: nav_bar::Model,
    settings: Settings,
    timer: TimerState,
    now: i64,
    report: Report,
    history_mtime: Option<SystemTime>,
    settings_mtime: Option<SystemTime>,
    confirm_clear: bool,
    width: f32,
}

impl App {
    fn refresh_report(&mut self) {
        self.history_mtime = history::file_mtime();
        self.report = Report::build(&history::load_all(), self.settings.daily_goal);
    }

    fn page(&self) -> Page {
        self.nav.active_data::<Page>().copied().unwrap_or(Page::Timer)
    }

    fn sync_title(&mut self) -> Task<Message> {
        let rem = format_clock(self.timer.remaining_ms(&self.settings, self.now));
        let title = match self.timer.status {
            Status::Running { .. } => format!("{rem} · {} — {}", self.timer.phase.title(), doro::APP_NAME),
            Status::Paused { .. } => format!("{rem} (paused) — {}", doro::APP_NAME),
            Status::Idle => doro::APP_NAME.to_string(),
        };
        self.set_header_title(doro::APP_NAME.into());
        match self.core.main_window_id() {
            Some(id) => self.set_window_title(title, id),
            None => Task::none(),
        }
    }
}

impl Application for App {
    type Executor = executor::Default;
    type Flags = Flags;
    type Message = Message;
    const APP_ID: &'static str = doro::APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, flags: Flags) -> (Self, Task<Message>) {
        let mut nav = nav_bar::Model::default();
        nav.insert().text("Timer").icon(icon::from_name("accessories-clock-symbolic")).data(Page::Timer);
        nav.insert()
            .text("Reports")
            .icon(icon::from_name("utilities-system-monitor-symbolic"))
            .data(Page::Reports);
        nav.insert()
            .text("Settings")
            .icon(icon::from_name("preferences-system-symbolic"))
            .data(Page::Settings);
        nav.activate_position(flags.page.as_deref().and_then(page_index).unwrap_or(0));

        let (timer, _) = engine::tick();
        let settings = Settings::load();
        let mut app = App {
            core,
            nav,
            timer,
            settings,
            now: engine::now_ms(),
            report: Report::default(),
            history_mtime: None,
            settings_mtime: doro::paths::mtime(&Settings::path()),
            confirm_clear: false,
            width: 1040.0,
        };
        app.refresh_report();
        if app.settings.tray_icon {
            Settings::spawn_tray();
        }
        let t = app.sync_title();
        (app, t)
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav)
    }

    /// Another launch (tray, applet, app library) asked us to show up.
    fn dbus_activation(&mut self, msg: cosmic::dbus_activation::Message) -> Task<Message> {
        if let cosmic::dbus_activation::Details::ActivateAction { action, .. } = &msg.msg {
            if let Some(n) = page_index(action) {
                self.nav.activate_position(n);
                if self.page() == Page::Reports {
                    self.refresh_report();
                }
            }
        }
        match msg.activation_token {
            Some(token) => cosmic::task::message(cosmic::Action::Cosmic(cosmic::app::Action::Activate(token))),
            None => Task::none(),
        }
    }

    fn on_window_resize(&mut self, id: cosmic::iced::window::Id, width: f32, _height: f32) {
        if Some(id) == self.core.main_window_id() {
            self.width = width;
        }
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Message> {
        self.nav.activate(id);
        if self.page() == Page::Reports {
            self.refresh_report();
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        cosmic::iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                let (timer, done) = engine::tick();
                self.timer = timer;
                self.now = engine::now_ms();
                let m = doro::paths::mtime(&Settings::path());
                if m != self.settings_mtime {
                    self.settings_mtime = m;
                    self.settings = Settings::load();
                }
                if done.is_some() || history::file_mtime() != self.history_mtime {
                    self.refresh_report();
                }
            }
            Message::Timer(cmd) => {
                self.timer = engine::apply(cmd);
                self.now = engine::now_ms();
            }
            Message::Set(field) => {
                let s = &mut self.settings;
                match field {
                    Field::Focus(v) => s.focus_minutes = v,
                    Field::Short(v) => s.short_break_minutes = v,
                    Field::Long(v) => s.long_break_minutes = v,
                    Field::Rounds(v) => s.rounds = v,
                    Field::Goal(v) => s.daily_goal = v,
                    Field::AutoBreaks(v) => s.auto_start_breaks = v,
                    Field::AutoFocus(v) => s.auto_start_focus = v,
                    Field::Notifications(v) => s.notifications = v,
                    Field::Sound(v) => s.notification_sound = v,
                    Field::PanelTime(v) => s.applet_show_time = v,
                    Field::Tray(v) => s.tray_icon = v,
                    Field::TrayAutostart(v) => Settings::set_tray_autostart(v),
                }
                s.save();
                if s.tray_icon {
                    Settings::spawn_tray();
                }
                self.settings_mtime = doro::paths::mtime(&Settings::path());
                self.report.goal = self.settings.daily_goal;
            }
            Message::TestSound => {
                doro::notify::play_sound(Phase::Focus);
            }
            Message::ClearHistory => self.confirm_clear = true,
            Message::ConfirmClear(yes) => {
                if yes {
                    history::clear();
                    self.refresh_report();
                }
                self.confirm_clear = false;
            }
        }
        self.sync_title()
    }

    fn dialog(&self) -> Option<Element<'_, Message>> {
        self.confirm_clear.then(|| {
            widget::dialog()
                .title("Clear all history?")
                .body("This permanently deletes every recorded session. Reports will start from zero.")
                .icon(icon::from_name("dialog-warning-symbolic").size(64))
                .primary_action(widget::button::destructive("Clear history").on_press(Message::ConfirmClear(true)))
                .secondary_action(widget::button::standard("Cancel").on_press(Message::ConfirmClear(false)))
                .into()
        })
    }

    fn view(&self) -> Element<'_, Message> {
        let content = match self.page() {
            Page::Timer => self.timer_page(),
            Page::Reports => self.reports_page(),
            Page::Settings => self.settings_page(),
        };
        widget::scrollable(
            widget::container(content)
                .max_width(1000)
                .padding([16, 24, 32, 24])
                .center_x(Length::Fill),
        )
        .into()
    }
}

// ------------------------------------------------------------------ helpers

fn card<'a>(content: impl Into<Element<'a, Message>>) -> widget::Container<'a, Message, cosmic::Theme> {
    widget::container(content)
        .padding(18)
        .width(Length::Fill)
        .class(theme::Container::Card)
}

fn titled_card<'a>(title: &'a str, subtitle: String, body: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    card(
        widget::Column::new()
            .spacing(12)
            .push(
                widget::Column::new()
                    .spacing(2)
                    .push(widget::text::heading(title))
                    .push(widget::text::caption(subtitle)),
            )
            .push(body),
    )
    .into()
}

fn stat<'a>(label: &'a str, value: String, detail: String, icon_name: &'static str) -> Element<'a, Message> {
    card(
        widget::Column::new()
            .spacing(6)
            .push(
                widget::Row::new()
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .push(icon::from_name(icon_name).size(16).icon().class(theme::Svg::Custom(std::rc::Rc::new(
                        |t: &cosmic::Theme| cosmic::iced::widget::svg::Style {
                            color: Some(ui::color(t.cosmic().accent_color())),
                        },
                    ))))
                    .push(widget::text::caption(label)),
            )
            .push(widget::text::title2(value))
            .push(widget::text::caption(detail)),
    )
    .into()
}

/// Simple themed meter: an accent fill over a muted track.
fn meter<'a>(fraction: f32, height: u16) -> Element<'a, Message> {
    let f = fraction.clamp(0.0, 1.0);
    let filled = (f * 1000.0).round() as u16;
    let radius = f32::from(height) / 2.0;
    let fill = widget::container(widget::Space::new())
        .width(Length::FillPortion(filled.max(1)))
        .height(height)
        .class(theme::Container::custom(move |t| cosmic::iced::widget::container::Style {
            background: Some(ui::color(t.cosmic().accent_color()).into()),
            border: cosmic::iced::Border { radius: radius.into(), ..Default::default() },
            ..Default::default()
        }));
    let mut track_row = widget::Row::new();
    if filled > 0 {
        track_row = track_row.push(fill);
    }
    if filled < 1000 {
        track_row = track_row.push(widget::Space::new().width(Length::FillPortion(1000 - filled)));
    }
    widget::container(track_row)
        .width(Length::Fill)
        .height(height)
        .class(theme::Container::custom(move |t| cosmic::iced::widget::container::Style {
            background: Some(ui::alpha(ui::color(t.cosmic().on_bg_color()), 0.08).into()),
            border: cosmic::iced::Border { radius: radius.into(), ..Default::default() },
            ..Default::default()
        }))
        .into()
}

fn pct_change(now: i64, before: i64) -> String {
    if before == 0 {
        if now == 0 { "No focus yet this week".into() } else { "First active week — great start".into() }
    } else {
        let p = (now - before) as f64 / before as f64 * 100.0;
        format!("{}{:.0}% vs previous 7 days", if p >= 0.0 { "+" } else { "" }, p)
    }
}

// -------------------------------------------------------------------- pages

impl App {
    fn timer_page(&self) -> Element<'_, Message> {
        let s = &self.settings;
        let t = &self.timer;
        let progress = t.progress(s, self.now);
        let content_w = if self.core.nav_bar_active() { self.width - 260.0 } else { self.width };
        let ring_size = (content_w - 56.0).clamp(240.0, 380.0);
        let caption = match t.phase {
            Phase::Focus => format!("Focus · round {}/{}", t.round, s.rounds.max(1)),
            p => format!("{} · {} min", p.title(), p.minutes(s)),
        };
        let ring = canvas(Ring {
            phase: t.phase,
            progress,
            running: t.is_running(),
            time: format_clock(t.remaining_ms(s, self.now)),
            caption,
            round: t.round,
            rounds: s.rounds,
            compact: false,
            on_color: None,
        })
        .width(Length::Fixed(ring_size))
        .height(Length::Fixed(ring_size));

        let (play_icon, play_text) = match t.status {
            Status::Running { .. } => ("media-playback-pause-symbolic", "Pause"),
            Status::Paused { .. } => ("media-playback-start-symbolic", "Resume"),
            Status::Idle => ("media-playback-start-symbolic", "Start"),
        };

        let controls = widget::Row::new()
            .spacing(16)
            .align_y(Alignment::Center)
            .push(
                widget::tooltip(
                    widget::button::icon(icon::from_name("edit-undo-symbolic"))
                        .medium()
                        .on_press(Message::Timer(Command::Reset)),
                    widget::text("Restart this phase"),
                    widget::tooltip::Position::Bottom,
                ),
            )
            .push(
                widget::button::suggested(play_text)
                    .leading_icon(icon::from_name(play_icon))
                    .on_press(Message::Timer(Command::Toggle))
                    .width(Length::Fixed(160.0)),
            )
            .push(
                widget::tooltip(
                    widget::button::icon(icon::from_name("media-skip-forward-symbolic"))
                        .medium()
                        .on_press(Message::Timer(Command::Skip)),
                    widget::text("Skip to next phase"),
                    widget::tooltip::Position::Bottom,
                ),
            );

        let phases = widget::Row::new()
            .spacing(8)
            .push(phase_chip(Phase::Focus, t.phase))
            .push(phase_chip(Phase::ShortBreak, t.phase))
            .push(phase_chip(Phase::LongBreak, t.phase));

        let r = &self.report;
        let goal = s.daily_goal.max(1);
        let today = card(
            widget::Column::new()
                .spacing(10)
                .push(
                    widget::Row::new()
                        .align_y(Alignment::Center)
                        .push(widget::text::heading("Today").width(Length::Fill))
                        .push(widget::text::caption(format!(
                            "{} / {} sessions · {} focused · 🔥 {} day streak",
                            r.today_sessions,
                            goal,
                            format_minutes(r.today_minutes),
                            r.streak
                        ))),
                )
                .push(meter(r.today_sessions as f32 / goal as f32, 8)),
        )
        .max_width(560);

        widget::Column::new()
            .spacing(22)
            .align_x(Alignment::Center)
            .width(Length::Fill)
            .push(phases)
            .push(ring)
            .push(controls)
            .push(widget::tooltip(
                widget::button::standard("Reset")
                    .leading_icon(icon::from_name("view-refresh-symbolic"))
                    .on_press(Message::Timer(Command::ResetCycle)),
                widget::text("Stop the timer and go back to focus, round 1"),
                widget::tooltip::Position::Bottom,
            ))
            .push(today)
            .into()
    }

    fn reports_page(&self) -> Element<'_, Message> {
        let r = &self.report;
        let goal = r.goal.max(1);
        // Nav bar takes ~260px when shown; COSMIC collapses it on narrow windows.
        let content_w = if self.core.nav_bar_active() { self.width - 260.0 } else { self.width };
        let stat_cols = if content_w >= 860.0 { 4 } else if content_w >= 440.0 { 2 } else { 1 };
        let pair_cols = if content_w >= 760.0 { 2 } else { 1 };

        let stats = vec![
            stat(
                "Today",
                format!("{} / {}", r.today_sessions, goal),
                format!("{} focus · {} break", format_minutes(r.today_minutes), format_minutes(r.today_break_minutes)),
                "accessories-clock-symbolic",
            ),
            stat(
                "Streak",
                format!("{} {}", r.streak, if r.streak == 1 { "day" } else { "days" }),
                format!("Best: {} days", r.best_streak),
                "starred-symbolic",
            ),
            stat(
                "This week",
                format_minutes(r.week_minutes),
                pct_change(r.week_minutes, r.prev_week_minutes),
                "x-office-calendar-symbolic",
            ),
            stat(
                "All time",
                format!("{} sessions", r.total_sessions),
                format!("{} over {} days", format_minutes(r.total_minutes), r.active_days),
                "utilities-system-monitor-symbolic",
            ),
        ];

        let today = Local::now().date_naive();
        let week = canvas(Bars {
            values: r.week.iter().map(|d| d.minutes).collect(),
            labels: r
                .week
                .iter()
                .map(|d| if d.date == today { "Today".into() } else { d.date.format("%a").to_string() })
                .collect(),
            highlight: Some(6),
            reference: Some(i64::from(self.settings.daily_goal * self.settings.focus_minutes)),
            show_values: true,
            label_every: 1,
        })
        .width(Length::Fill)
        .height(Length::Fixed(200.0));

        let week_card = titled_card(
            "Last 7 days",
            format!(
                "{} focused · dashed line is your daily goal ({} sessions)",
                format_minutes(r.week_minutes),
                self.settings.daily_goal
            ),
            week,
        );

        // Heatmap layout.
        let first = r.heatmap.first().map(|d| d.date).unwrap_or(today);
        let mut months = Vec::new();
        let mut last_month = None;
        let cells = r
            .heatmap
            .iter()
            .map(|d| {
                let days = (d.date - first).num_days() as usize;
                let (col, row) = (days / 7, days % 7);
                if row == 0 && last_month != Some(d.date.month()) {
                    last_month = Some(d.date.month());
                    months.push((col, d.date.format("%b").to_string()));
                }
                (col, row, d.minutes)
            })
            .collect();
        let active = r.heatmap.iter().filter(|d| d.minutes > 0).count();
        let heat_card = titled_card(
            "Activity",
            format!("{active} active days in the last {} weeks", history::HEATMAP_WEEKS),
            canvas(Heatmap { cells, weeks: history::HEATMAP_WEEKS as usize, month_labels: months })
                .width(Length::Fill)
                .height(Length::Fixed(150.0)),
        );

        let hours_card = titled_card(
            "Focus by hour",
            match r.peak_hour() {
                Some(h) => format!("You focus best around {h:02}:00"),
                None => "Complete a session to find your rhythm".into(),
            },
            canvas(Bars {
                values: r.hours.to_vec(),
                labels: (0..24).map(|h| format!("{h:02}")).collect(),
                highlight: r.peak_hour(),
                reference: None,
                show_values: false,
                label_every: 3,
            })
            .width(Length::Fill)
            .height(Length::Fixed(150.0)),
        );

        let mut recent = widget::list_column();
        if r.recent.is_empty() {
            recent = recent.add(widget::text::caption("No sessions yet — start your first focus round!"));
        }
        for s in &r.recent {
            let when = s.local_start();
            let day = if when.date_naive() == today {
                "Today".to_string()
            } else {
                when.format("%a %d %b").to_string()
            };
            let name = if s.phase == Phase::Focus {
                if s.label.is_empty() { "Focus".to_string() } else { s.label.clone() }
            } else {
                s.phase.title().to_string()
            };
            recent = recent.add(
                widget::Row::new()
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .push(widget::text::body(if s.phase == Phase::Focus { "🍅" } else { "☕" }))
                    .push(widget::text::body(name).width(Length::Fill))
                    .push(widget::text::caption(format!("{day} · {}", when.format("%H:%M"))))
                    .push(
                        widget::text::body(format_minutes(s.minutes()))
                            .width(Length::Fixed(56.0))
                            .align_x(Horizontal::Right),
                    ),
            );
        }
        let recent_card = titled_card("Recent sessions", "Your latest completed rounds".into(), recent);

        let highlights = titled_card(
            "Highlights",
            "Personal bests".into(),
            widget::Column::new()
                .spacing(10)
                .push(kv(
                    "Best day",
                    r.best_day
                        .as_ref()
                        .map(|d| format!("{} · {}", d.date.format("%a %d %b %Y"), format_minutes(d.minutes)))
                        .unwrap_or_else(|| "—".into()),
                ))
                .push(kv("Average per active day", format_minutes(r.avg_daily_minutes())))
                .push(kv("Longest streak", format!("{} days", r.best_streak)))
                .push(kv("Total focus", format_minutes(r.total_minutes))),
        );

        widget::Column::new()
            .spacing(16)
            .push(widget::text::title1("Reports"))
            .push(grid(stats, stat_cols))
            .push(week_card)
            .push(grid(vec![heat_card, hours_card], pair_cols))
            .push(grid(vec![highlights, recent_card], pair_cols))
            .into()
    }

    fn settings_page(&self) -> Element<'_, Message> {
        let s = &self.settings;
        let spin = |label: &'static str, value: u32, min: u32, max: u32, unit: &'static str, f: fn(u32) -> Field| {
            settings::item(
                label,
                widget::spin_button(
                    format!("{value} {unit}"),
                    label,
                    value,
                    1,
                    min,
                    max,
                    move |v| Message::Set(f(v)),
                ),
            )
        };

        let logo = icon::from_svg_bytes(doro::LOGO_SVG).icon().size(72);
        let about = card(
            widget::Row::new()
                .spacing(18)
                .align_y(Alignment::Center)
                .push(logo)
                .push(
                    widget::Column::new()
                        .spacing(4)
                        .push(widget::text::title3(doro::APP_NAME))
                        .push(widget::text::caption(format!("Version {}", env!("CARGO_PKG_VERSION"))))
                        .push(widget::text::caption(
                            "A cosmic pomodoro timer for the COSMIC desktop. Add the Cosmic Doro applet \
                             from Settings → Desktop → Panel → Applets to keep the timer in your panel.",
                        )),
                ),
        );

        settings::view_column(vec![
            widget::text::title1("Settings").into(),
            settings::section()
                .title("Timer")
                .add(spin("Focus", s.focus_minutes, 1, 180, "min", Field::Focus))
                .add(spin("Short break", s.short_break_minutes, 1, 60, "min", Field::Short))
                .add(spin("Long break", s.long_break_minutes, 1, 90, "min", Field::Long))
                .add(spin("Rounds before long break", s.rounds, 1, 12, "rounds", Field::Rounds))
                .add(spin("Daily goal", s.daily_goal, 1, 24, "sessions", Field::Goal))
                .into(),
            settings::section()
                .title("Behavior")
                .add(
                    settings::item::builder("Auto-start breaks")
                        .description("Begin the break as soon as a focus round ends")
                        .toggler(s.auto_start_breaks, |v| Message::Set(Field::AutoBreaks(v))),
                )
                .add(
                    settings::item::builder("Auto-start focus")
                        .description("Begin the next focus round as soon as a break ends")
                        .toggler(s.auto_start_focus, |v| Message::Set(Field::AutoFocus(v))),
                )
                .into(),
            settings::section()
                .title("Notifications")
                .add(
                    settings::item::builder("Desktop notifications")
                        .description("Shown through COSMIC notifications when a phase ends")
                        .toggler(s.notifications, |v| Message::Set(Field::Notifications(v))),
                )
                .add(
                    settings::item::builder("Sound")
                        .description("Play a chime when a focus session or break ends")
                        .toggler(s.notification_sound, |v| Message::Set(Field::Sound(v))),
                )
                .add(settings::item(
                    "Test the sound",
                    widget::button::standard("Play")
                        .leading_icon(icon::from_name("audio-volume-high-symbolic"))
                        .on_press(Message::TestSound),
                ))
                .into(),
            settings::section()
                .title("Panel applet")
                .add(
                    settings::item::builder("Show remaining time in panel")
                        .description("Otherwise only the progress ring is shown")
                        .toggler(s.applet_show_time, |v| Message::Set(Field::PanelTime(v))),
                )
                .into(),
            settings::section()
                .title("Tray icon")
                .add(
                    settings::item::builder("Show tray icon")
                        .description("Live progress ring in the status area; keeps the timer running after you close this window")
                        .toggler(s.tray_icon, |v| Message::Set(Field::Tray(v))),
                )
                .add(
                    settings::item::builder("Start tray at login")
                        .description("Have Cosmic Doro ready in the tray every time you log in")
                        .toggler(Settings::tray_autostart(), |v| Message::Set(Field::TrayAutostart(v))),
                )
                .into(),
            settings::section()
                .title("Data")
                .add(settings::item(
                    "Session history",
                    widget::button::destructive("Clear history").on_press(Message::ClearHistory),
                ))
                .into(),
            about.into(),
        ])
        .into()
    }
}

/// Lays out elements in rows of `cols` equal-width columns.
fn grid<'a>(items: Vec<Element<'a, Message>>, cols: usize) -> Element<'a, Message> {
    let mut column = widget::Column::new().spacing(16);
    let mut row = widget::Row::new().spacing(16);
    let mut n = 0;
    for item in items {
        row = row.push(item);
        n += 1;
        if n == cols {
            column = column.push(row);
            row = widget::Row::new().spacing(16);
            n = 0;
        }
    }
    if n > 0 {
        for _ in n..cols {
            row = row.push(widget::Space::new().width(Length::Fill));
        }
        column = column.push(row);
    }
    column.into()
}

fn kv<'a>(k: &'a str, v: String) -> Element<'a, Message> {
    widget::Row::new()
        .push(widget::text::body(k).width(Length::Fill))
        .push(widget::text::body(v))
        .into()
}

fn phase_chip<'a>(phase: Phase, current: Phase) -> Element<'a, Message> {
    let active = phase == current;
    widget::container(widget::text::caption(phase.title()))
        .padding([4, 12])
        .class(theme::Container::custom(move |t| {
            let c = ui::phase_color(t, phase);
            cosmic::iced::widget::container::Style {
                background: Some(ui::alpha(c, if active { 0.22 } else { 0.0 }).into()),
                text_color: Some(if active { c } else { ui::alpha(ui::color(t.cosmic().on_bg_color()), 0.6) }),
                border: cosmic::iced::Border {
                    radius: 999.0.into(),
                    width: 1.0,
                    color: ui::alpha(c, if active { 0.6 } else { 0.0 }),
                },
                ..Default::default()
            }
        }))
        .into()
}
