// Cosmic Doro — COSMIC panel applet.

use cosmic::app::{Core, Task};
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Length, Rectangle, Subscription};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::{self, canvas, icon};
use cosmic::{Element, theme};
use doro::config::Settings;
use doro::engine::{self, Command, Phase, Status, TimerState};
use doro::ui::{self, Ring};
use doro::{format_clock, format_minutes};
use std::time::{Duration, SystemTime};

fn main() -> cosmic::iced::Result {
    cosmic::applet::run::<Applet>(())
}

#[derive(Clone, Debug)]
enum Message {
    Tick,
    Timer(Command),
    OpenApp,
    Surface(cosmic::surface::Action<Message>),
    PopupClosed(Id),
}

struct Applet {
    core: Core,
    popup: Option<Id>,
    settings: Settings,
    settings_mtime: Option<SystemTime>,
    timer: TimerState,
    now: i64,
    today: (usize, i64),
    history_mtime: Option<SystemTime>,
}

impl Applet {
    fn refresh_today(&mut self) {
        self.history_mtime = doro::history::file_mtime();
        let today = chrono::Local::now().date_naive();
        let s = doro::history::load_all();
        let focus = s.iter().filter(|s| s.phase == Phase::Focus && s.day() == today);
        self.today = focus.fold((0, 0), |(n, m), s| (n + 1, m + s.minutes()));
    }

    fn popup_view(&self) -> Element<'_, Message> {
        let s = &self.settings;
        let t = &self.timer;
        let progress = t.progress(s, self.now);
        let (play_icon, play_text) = match t.status {
            Status::Running { .. } => ("media-playback-pause-symbolic", "Pause"),
            Status::Paused { .. } => ("media-playback-start-symbolic", "Resume"),
            Status::Idle => ("media-playback-start-symbolic", "Start"),
        };
        let phase = t.phase;
        let status = match t.status {
            Status::Running { .. } => "Running",
            Status::Paused { .. } => "Paused",
            Status::Idle => "Ready",
        };

        let header = widget::Row::new()
            .align_y(Alignment::Center)
            .spacing(8)
            .push(
                widget::text::heading(t.display_label().to_string())
                    .width(Length::Fill)
                    .class(theme::Text::Color(ui::phase_color(&theme::active(), phase))),
            )
            .push(widget::text::caption(format!("{status} · round {}/{}", t.round, s.rounds.max(1))));

        let ring = canvas(Ring {
            phase,
            progress,
            running: t.is_running(),
            time: format_clock(t.remaining_ms(s, self.now)),
            caption: phase.title().to_string(),
            round: t.round,
            rounds: s.rounds,
            compact: false,
            on_color: None,
        })
        .width(Length::Fixed(200.0))
        .height(Length::Fixed(200.0));

        let controls = widget::Row::new()
            .spacing(12)
            .align_y(Alignment::Center)
            .push(widget::button::icon(icon::from_name("edit-undo-symbolic")).on_press(Message::Timer(Command::Reset)))
            .push(
                widget::button::suggested(play_text)
                    .leading_icon(icon::from_name(play_icon))
                    .on_press(Message::Timer(Command::Toggle)),
            )
            .push(
                widget::button::icon(icon::from_name("media-skip-forward-symbolic"))
                    .on_press(Message::Timer(Command::Skip)),
            );

        let goal = s.daily_goal.max(1);
        let content = widget::Column::new()
            .padding([12, 16])
            .spacing(12)
            .align_x(Alignment::Center)
            .push(header)
            .push(ring)
            .push(controls)
            .push(widget::divider::horizontal::default())
            .push(
                widget::Row::new()
                    .push(widget::text::body("Today").width(Length::Fill))
                    .push(widget::text::caption(format!(
                        "🍅 {}/{goal} · {}",
                        self.today.0,
                        format_minutes(self.today.1)
                    ))),
            )
            .push(
                widget::Row::new()
                    .spacing(8)
                    .push(
                        widget::button::standard("Reset")
                            .leading_icon(icon::from_name("view-refresh-symbolic"))
                            .width(Length::Fill)
                            .on_press(Message::Timer(Command::ResetCycle)),
                    )
                    .push(
                        widget::button::standard("Open reports")
                            .leading_icon(icon::from_name("utilities-system-monitor-symbolic"))
                            .width(Length::Fill)
                            .on_press(Message::OpenApp),
                    ),
            );

        self.core.applet.popup_container(content).into()
    }
}

impl cosmic::Application for Applet {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = doro::APPLET_ID;

    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _: ()) -> (Self, Task<Message>) {
        let (timer, _) = engine::tick();
        let mut a = Applet {
            core,
            popup: None,
            settings: Settings::load(),
            settings_mtime: doro::paths::mtime(&Settings::path()),
            timer,
            now: engine::now_ms(),
            today: (0, 0),
            history_mtime: None,
        };
        a.refresh_today();
        (a, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        // Tick faster while the popup is open so its ring animates smoothly.
        let ms = if self.popup.is_some() { 250 } else { 1000 };
        cosmic::iced::time::every(Duration::from_millis(ms)).map(|_| Message::Tick)
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
                if done.is_some() || doro::history::file_mtime() != self.history_mtime {
                    self.refresh_today();
                }
            }
            Message::Timer(cmd) => {
                self.timer = engine::apply(cmd);
                self.now = engine::now_ms();
            }
            Message::OpenApp => {
                let exe = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.join("cosmic-doro")))
                    .filter(|p| p.exists())
                    .unwrap_or_else(|| "cosmic-doro".into());
                if let Ok(mut child) = std::process::Command::new(exe).arg("--reports").spawn() {
                    std::thread::spawn(move || child.wait());
                }
                if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Surface(destroy_popup(id)));
                }
            }
            Message::Surface(a) => return cosmic::task::message(cosmic::Action::Surface(a)),
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let applet = &self.core.applet;
        let horizontal = applet.is_horizontal();
        let (size, _) = applet.suggested_size(true);
        let (major, minor) = applet.suggested_padding(true);
        let (hpad, vpad) = if horizontal { (major, minor) } else { (minor, major) };

        let t = &self.timer;
        let active = !matches!(t.status, Status::Idle);
        let ring = canvas(Ring {
            phase: t.phase,
            progress: t.progress(&self.settings, self.now),
            running: active,
            time: String::new(),
            caption: String::new(),
            round: t.round,
            rounds: self.settings.rounds,
            compact: true,
            on_color: None,
        })
        .width(Length::Fixed(f32::from(size)))
        .height(Length::Fixed(f32::from(size)));

        let mut content = widget::Row::new().spacing(6).align_y(Alignment::Center).push(ring);
        if horizontal && self.settings.applet_show_time && active {
            let mut label = format_clock(t.remaining_ms(&self.settings, self.now));
            if matches!(t.status, Status::Paused { .. }) {
                label.push_str(" ⏸");
            }
            content = content.push(applet.text(label));
        }

        let have_popup = self.popup;
        let button = widget::button::custom(
            widget::layer_container(content).center_y(Length::Fixed(f32::from(size + 2 * vpad))),
        )
        .padding([0, hpad])
        .class(theme::Button::AppletIcon)
        .on_press_with_rectangle(move |offset, bounds| {
            if let Some(id) = have_popup {
                Message::Surface(destroy_popup(id))
            } else {
                Message::Surface(app_popup::<Applet>(
                    // Default live settings: follow the user's COSMIC frosted/blur choice.
                    |_| Default::default(),
                    move |state: &mut Applet| {
                        let new_id = Id::unique();
                        state.popup = Some(new_id);
                        let mut ps = state.core.applet.get_popup_settings(
                            state.core.main_window_id().unwrap(),
                            new_id,
                            None,
                            None,
                            None,
                        );
                        ps.positioner.anchor_rect = Rectangle {
                            x: (bounds.x - offset.x) as i32,
                            y: (bounds.y - offset.y) as i32,
                            width: bounds.width as i32,
                            height: bounds.height as i32,
                        };
                        ps
                    },
                    Some(Box::new(|state: &Applet| state.popup_view().map(cosmic::Action::App))),
                ))
            }
        });

        let tip = match t.status {
            Status::Idle => format!("{} — ready", doro::APP_NAME),
            _ => format!(
                "{} — {} {}",
                doro::APP_NAME,
                t.display_label(),
                format_clock(t.remaining_ms(&self.settings, self.now))
            ),
        };
        applet
            .applet_tooltip::<Message>(button, tip, self.popup.is_some(), Message::Surface, None)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        "".into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}
