//! Canvas drawing shared by the window and the applet. Every color is pulled
//! from the active COSMIC theme so the timer follows accent color and
//! light/dark changes live.

use crate::engine::Phase;
use cosmic::iced::alignment::Vertical;
use cosmic::iced::mouse::Cursor;
use cosmic::iced::widget::canvas::{self, Frame, Geometry, LineCap, Path, Stroke, Text};
use cosmic::iced::widget::text::Alignment as TextAlign;
use cosmic::iced::{Color, Pixels, Point, Radians, Rectangle, Size, Vector};
use cosmic::{Renderer, Theme};
use std::f32::consts::{FRAC_PI_2, TAU};

pub fn color(c: cosmic::cosmic_theme::palette::Srgba) -> Color {
    Color::from_rgba(c.red, c.green, c.blue, c.alpha)
}

pub fn alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::from_rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

pub fn phase_color(theme: &Theme, phase: Phase) -> Color {
    let t = theme.cosmic();
    match phase {
        Phase::Focus => color(t.accent_color()),
        Phase::ShortBreak => color(t.success_color()),
        Phase::LongBreak => color(t.palette.accent_purple),
    }
}

/// The phase color drifts toward a warm tone as a focus session nears its end.
pub fn ring_color(theme: &Theme, phase: Phase, progress: f32) -> Color {
    let base = phase_color(theme, phase);
    if phase == Phase::Focus {
        let warm = color(theme.cosmic().palette.accent_orange);
        mix(base, warm, ((progress - 0.6) / 0.4).max(0.0) * 0.85)
    } else {
        base
    }
}

fn fg(theme: &Theme) -> Color {
    color(theme.cosmic().on_bg_color())
}

fn arc(center: Point, radius: f32, from: f32, to: f32) -> Path {
    Path::new(|b| {
        b.arc(canvas::path::Arc {
            center,
            radius,
            start_angle: Radians(from),
            end_angle: Radians(to),
        })
    })
}

fn text(content: impl Into<String>, pos: Point, size: f32, color: Color) -> Text {
    Text {
        content: content.into(),
        position: pos,
        color,
        size: Pixels(size),
        align_x: TextAlign::Center,
        align_y: Vertical::Center,
        ..Text::default()
    }
}

// ---------------------------------------------------------------- Ring ----

/// The big stopwatch ring on the timer page (and a tiny one in the panel).
pub struct Ring {
    pub phase: Phase,
    pub progress: f32,
    pub running: bool,
    pub time: String,
    pub caption: String,
    pub round: u32,
    pub rounds: u32,
    /// Compact mode for the panel: no text, no ticks.
    pub compact: bool,
    /// Override for the ring's neutral color (panel text color).
    pub on_color: Option<Color>,
}

impl<M> canvas::Program<M, Theme, Renderer> for Ring {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, theme: &Theme, bounds: Rectangle, _: Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, bounds.size());
        let side = bounds.width.min(bounds.height);
        let c = f.center();
        let accent = ring_color(theme, self.phase, self.progress);
        let neutral = self.on_color.unwrap_or_else(|| fg(theme));

        if self.compact {
            let w = (side * 0.13).max(2.0);
            let r = side / 2.0 - w;
            let c = Point::new(c.x, c.y + w * 0.35);
            // Stopwatch crown.
            f.fill_rectangle(
                Point::new(c.x - w * 0.6, c.y - r - w * 1.6),
                Size::new(w * 1.2, w * 0.9),
                neutral,
            );
            f.stroke(&Path::circle(c, r), Stroke::default().with_width(w).with_color(alpha(neutral, 0.35)));
            if self.progress > 0.0 || self.running {
                let start = -FRAC_PI_2;
                let end = start + TAU * (1.0 - self.progress).max(0.001);
                f.stroke(
                    &arc(c, r, start, end),
                    Stroke { line_cap: LineCap::Round, ..Stroke::default().with_width(w).with_color(accent) },
                );
            } else {
                f.fill(&Path::circle(c, w * 0.8), neutral);
            }
            return vec![f.into_geometry()];
        }

        let width = side * 0.055;
        let r = side / 2.0 - width * 1.6;

        // Faint starfield inside the dial.
        let seeds: [(f32, f32, f32); 14] = [
            (0.12, 0.30, 1.0), (0.78, 0.22, 1.4), (0.66, 0.74, 1.0), (0.25, 0.70, 1.6),
            (0.45, 0.15, 0.9), (0.86, 0.55, 1.1), (0.15, 0.52, 0.8), (0.55, 0.86, 1.2),
            (0.35, 0.36, 0.7), (0.70, 0.40, 0.8), (0.30, 0.85, 0.9), (0.82, 0.78, 0.7),
            (0.50, 0.30, 0.6), (0.20, 0.18, 1.1),
        ];
        let tl = Point::new(c.x - r, c.y - r);
        for (x, y, s) in seeds {
            let p = Point::new(tl.x + x * 2.0 * r, tl.y + y * 2.0 * r);
            if p.distance(c) < r * 0.85 {
                f.fill(&Path::circle(p, s * side / 300.0), alpha(neutral, 0.18));
            }
        }

        // Minute ticks like a stopwatch bezel.
        for i in 0..60 {
            let a = i as f32 / 60.0 * TAU - FRAC_PI_2;
            let major = i % 5 == 0;
            let (r1, r2) = (r + width * 0.95, r + width * if major { 1.55 } else { 1.25 });
            let dir = Vector::new(a.cos(), a.sin());
            f.stroke(
                &Path::line(c + dir * r1, c + dir * r2),
                Stroke::default()
                    .with_width(if major { 2.0 } else { 1.0 })
                    .with_color(alpha(neutral, if major { 0.35 } else { 0.15 })),
            );
        }

        // Track.
        f.stroke(&Path::circle(c, r), Stroke::default().with_width(width).with_color(alpha(neutral, 0.08)));

        // Remaining arc (counts down clockwise from 12 o'clock), with a soft glow.
        let remaining = 1.0 - self.progress;
        if remaining > 0.0005 {
            let start = -FRAC_PI_2 + TAU * self.progress;
            let end = -FRAC_PI_2 + TAU;
            let path = arc(c, r, start, end);
            f.stroke(
                &path,
                Stroke { line_cap: LineCap::Round, ..Stroke::default().with_width(width * 2.2).with_color(alpha(accent, 0.12)) },
            );
            f.stroke(
                &path,
                Stroke { line_cap: LineCap::Round, ..Stroke::default().with_width(width).with_color(accent) },
            );
            // Orbiting "planet" at the leading edge.
            let dir = Vector::new(start.cos(), start.sin());
            f.fill(&Path::circle(c + dir * r, width * 0.95), accent);
            f.fill(&Path::circle(c + dir * r, width * 0.42), alpha(Color::WHITE, 0.9));
        }

        // Center text.
        f.fill_text(text(self.time.clone(), Point::new(c.x, c.y - side * 0.03), side * 0.2, neutral));
        f.fill_text(text(
            self.caption.to_uppercase(),
            Point::new(c.x, c.y + side * 0.12),
            side * 0.048,
            alpha(neutral, 0.65),
        ));

        // Round dots.
        let n = self.rounds.clamp(1, 12);
        let gap = side * 0.05;
        let x0 = c.x - gap * (n as f32 - 1.0) / 2.0;
        for i in 0..n {
            let p = Point::new(x0 + gap * i as f32, c.y + side * 0.21);
            let done = i + 1 < self.round || (i + 1 == self.round && self.phase.is_break());
            let current = i + 1 == self.round && self.phase == Phase::Focus;
            let col = if done || current { phase_color(theme, Phase::Focus) } else { alpha(neutral, 0.2) };
            f.fill(&Path::circle(p, side * if current { 0.016 } else { 0.012 }), col);
        }

        vec![f.into_geometry()]
    }
}

// -------------------------------------------------------------- Charts ----

/// Vertical bar chart with labels underneath, used for the week and hours.
pub struct Bars {
    pub values: Vec<i64>,
    pub labels: Vec<String>,
    pub highlight: Option<usize>,
    /// Optional dashed reference line (e.g. daily goal) in the same unit.
    pub reference: Option<i64>,
    pub show_values: bool,
    /// Only draw every n-th label.
    pub label_every: usize,
}

impl<M> canvas::Program<M, Theme, Renderer> for Bars {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, theme: &Theme, bounds: Rectangle, cursor: Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, bounds.size());
        let fgc = fg(theme);
        let accent = color(theme.cosmic().accent_color());
        let n = self.values.len().max(1);
        let label_h = 20.0;
        let top_pad = if self.show_values { 18.0 } else { 6.0 };
        let chart_h = (bounds.height - label_h - top_pad).max(1.0);
        let max = self
            .values
            .iter()
            .copied()
            .chain(self.reference)
            .max()
            .unwrap_or(0)
            .max(1) as f32;
        let slot = bounds.width / n as f32;
        let bar_w = (slot * 0.62).min(44.0);
        let hover = cursor.position_in(bounds).map(|p| (p.x / slot) as usize);

        // Baseline.
        let base_y = top_pad + chart_h;
        f.fill_rectangle(Point::new(0.0, base_y), Size::new(bounds.width, 1.0), alpha(fgc, 0.12));

        for (i, v) in self.values.iter().enumerate() {
            let h = (*v as f32 / max * chart_h).max(if *v > 0 { 3.0 } else { 0.0 });
            let x = slot * i as f32 + (slot - bar_w) / 2.0;
            let is_hi = self.highlight == Some(i);
            let hovered = hover == Some(i);
            let col = if is_hi || hovered { accent } else { alpha(accent, 0.45) };
            // Empty slot ghost so the rhythm of the chart stays readable.
            f.fill(
                &Path::rounded_rectangle(Point::new(x, top_pad), Size::new(bar_w, chart_h), (bar_w / 2.0).min(6.0).into()),
                alpha(fgc, 0.035),
            );
            if h > 0.0 {
                f.fill(
                    &Path::rounded_rectangle(Point::new(x, base_y - h), Size::new(bar_w, h), (bar_w / 2.0).min(6.0).into()),
                    col,
                );
            }
            if (self.show_values || hovered) && *v > 0 {
                f.fill_text(text(
                    crate::format_minutes(*v),
                    Point::new(x + bar_w / 2.0, (base_y - h - 9.0).max(8.0)),
                    11.0,
                    alpha(fgc, if hovered { 1.0 } else { 0.7 }),
                ));
            }
            if i % self.label_every.max(1) == 0 {
                if let Some(l) = self.labels.get(i) {
                    f.fill_text(text(
                        l.clone(),
                        Point::new(x + bar_w / 2.0, base_y + label_h / 2.0 + 2.0),
                        11.0,
                        alpha(fgc, if is_hi { 0.95 } else { 0.55 }),
                    ));
                }
            }
        }

        if let Some(r) = self.reference.filter(|r| *r > 0) {
            let y = base_y - r as f32 / max * chart_h;
            let mut x = 0.0;
            while x < bounds.width {
                f.fill_rectangle(Point::new(x, y), Size::new(5.0, 1.0), alpha(color(theme.cosmic().success_color()), 0.8));
                x += 9.0;
            }
        }
        vec![f.into_geometry()]
    }

    fn mouse_interaction(&self, _: &(), _: Rectangle, _: Cursor) -> cosmic::iced::mouse::Interaction {
        cosmic::iced::mouse::Interaction::default()
    }
}

/// GitHub-style activity grid: columns are weeks, rows are weekdays.
pub struct Heatmap {
    /// (week column, weekday row 0=Mon, minutes)
    pub cells: Vec<(usize, usize, i64)>,
    pub weeks: usize,
    pub month_labels: Vec<(usize, String)>,
}

impl<M> canvas::Program<M, Theme, Renderer> for Heatmap {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, theme: &Theme, bounds: Rectangle, cursor: Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, bounds.size());
        let fgc = fg(theme);
        let accent = color(theme.cosmic().accent_color());
        let left = 30.0;
        let top = 18.0;
        let gap = 3.0;
        let cell = ((bounds.width - left) / self.weeks.max(1) as f32 - gap)
            .min((bounds.height - top) / 7.0 - gap)
            .max(4.0);
        let max = self.cells.iter().map(|c| c.2).max().unwrap_or(0).max(1) as f32;

        for (row, name) in [(0, "Mon"), (2, "Wed"), (4, "Fri")] {
            f.fill_text(Text {
                align_x: TextAlign::Left,
                ..text(name, Point::new(0.0, top + row as f32 * (cell + gap) + cell / 2.0), 10.0, alpha(fgc, 0.5))
            });
        }
        for (col, name) in &self.month_labels {
            f.fill_text(Text {
                align_x: TextAlign::Left,
                ..text(name.clone(), Point::new(left + *col as f32 * (cell + gap), 7.0), 10.0, alpha(fgc, 0.5))
            });
        }

        let mut tip = None;
        for (col, row, mins) in &self.cells {
            let p = Point::new(left + *col as f32 * (cell + gap), top + *row as f32 * (cell + gap));
            let col_ = if *mins == 0 {
                alpha(fgc, 0.07)
            } else {
                alpha(accent, 0.3 + 0.7 * (*mins as f32 / max).sqrt())
            };
            f.fill(&Path::rounded_rectangle(p, Size::new(cell, cell), (cell * 0.25).into()), col_);
            if let Some(cp) = cursor.position_in(bounds) {
                if Rectangle::new(p, Size::new(cell, cell)).contains(cp) {
                    tip = Some((p, *mins));
                }
            }
        }
        if let Some((p, mins)) = tip {
            f.stroke(
                &Path::rounded_rectangle(p, Size::new(cell, cell), (cell * 0.25).into()),
                Stroke::default().with_width(1.5).with_color(fgc),
            );
            let label = if mins == 0 { "No focus".to_string() } else { crate::format_minutes(mins) };
            let x = (p.x + cell / 2.0).clamp(30.0, bounds.width - 30.0);
            f.fill_text(text(label, Point::new(x, (p.y - 8.0).max(6.0)), 11.0, fgc));
        }
        vec![f.into_geometry()]
    }
}
