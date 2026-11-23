//! Completed sessions, stored one JSON object per line in
//! `$XDG_DATA_HOME/cosmic-doro/history.jsonl`, plus report aggregation.

use crate::engine::Phase;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Session {
    /// Epoch milliseconds.
    pub start: i64,
    pub end: i64,
    pub phase: Phase,
    pub seconds: i64,
    #[serde(default)]
    pub label: String,
}

impl Session {
    pub fn local_start(&self) -> DateTime<Local> {
        DateTime::from_timestamp_millis(self.start)
            .unwrap_or_default()
            .with_timezone(&Local)
    }
    pub fn day(&self) -> NaiveDate {
        self.local_start().date_naive()
    }
    pub fn minutes(&self) -> i64 {
        (self.seconds + 30) / 60
    }
}

pub fn path() -> PathBuf {
    crate::paths::data_dir().join("history.jsonl")
}

pub fn file_mtime() -> Option<std::time::SystemTime> {
    crate::paths::mtime(&path())
}

pub fn append(s: &Session) {
    let p = path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let (Ok(mut f), Ok(line)) = (
        std::fs::OpenOptions::new().create(true).append(true).open(&p),
        serde_json::to_string(s),
    ) {
        let _ = writeln!(f, "{line}");
    }
}

pub fn load_all() -> Vec<Session> {
    std::fs::read_to_string(path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

pub fn clear() {
    let _ = std::fs::remove_file(path());
}

pub fn today_focus_count() -> usize {
    let today = Local::now().date_naive();
    load_all()
        .iter()
        .filter(|s| s.phase == Phase::Focus && s.day() == today)
        .count()
}

#[derive(Clone, Debug, Default)]
pub struct DayStat {
    pub date: NaiveDate,
    pub minutes: i64,
    pub sessions: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub today_sessions: u32,
    pub today_minutes: i64,
    pub today_break_minutes: i64,
    pub goal: u32,
    pub streak: u32,
    pub best_streak: u32,
    pub total_sessions: u32,
    pub total_minutes: i64,
    pub active_days: u32,
    pub best_day: Option<DayStat>,
    /// Last 7 days, oldest first (today last).
    pub week: Vec<DayStat>,
    pub week_minutes: i64,
    pub prev_week_minutes: i64,
    /// Last 18 weeks, Monday-aligned, oldest first; days after today omitted.
    pub heatmap: Vec<DayStat>,
    /// Focus minutes by hour of day (all time).
    pub hours: [i64; 24],
    /// Most recent sessions, newest first.
    pub recent: Vec<Session>,
}

pub const HEATMAP_WEEKS: i64 = 18;

impl Report {
    pub fn build(sessions: &[Session], goal: u32) -> Self {
        Self::build_for(sessions, goal, Local::now().date_naive())
    }

    pub fn build_for(sessions: &[Session], goal: u32, today: NaiveDate) -> Self {
        let mut r = Report { goal, ..Default::default() };
        let mut by_day: HashMap<NaiveDate, DayStat> = HashMap::new();

        for s in sessions {
            let day = s.day();
            if s.phase != Phase::Focus {
                if day == today {
                    r.today_break_minutes += s.minutes();
                }
                continue;
            }
            let mins = s.minutes();
            r.total_sessions += 1;
            r.total_minutes += mins;
            let d = by_day.entry(day).or_insert(DayStat { date: day, ..Default::default() });
            d.minutes += mins;
            d.sessions += 1;
            r.hours[s.local_start().hour() as usize] += mins;
        }

        let get = |d: NaiveDate| by_day.get(&d).cloned().unwrap_or(DayStat { date: d, ..Default::default() });

        let t = get(today);
        r.today_sessions = t.sessions;
        r.today_minutes = t.minutes;
        r.active_days = by_day.len() as u32;
        r.best_day = by_day.values().max_by_key(|d| (d.minutes, d.date)).cloned();

        r.week = (0..7).rev().map(|i| get(today - Duration::days(i))).collect();
        r.week_minutes = r.week.iter().map(|d| d.minutes).sum();
        r.prev_week_minutes = (7..14).map(|i| get(today - Duration::days(i)).minutes).sum();

        let monday = today - Duration::days(i64::from(today.weekday().num_days_from_monday()));
        let first = monday - Duration::weeks(HEATMAP_WEEKS - 1);
        r.heatmap = (0..HEATMAP_WEEKS * 7)
            .map(|i| first + Duration::days(i))
            .take_while(|d| *d <= today)
            .map(get)
            .collect();

        // Current streak: today counts if active, otherwise start from yesterday.
        let mut day = if t.sessions > 0 { today } else { today - Duration::days(1) };
        while by_day.contains_key(&day) {
            r.streak += 1;
            day -= Duration::days(1);
        }
        let mut days: Vec<_> = by_day.keys().copied().collect();
        days.sort();
        let (mut run, mut prev) = (0u32, None::<NaiveDate>);
        for d in days {
            run = if prev.is_some_and(|p| p + Duration::days(1) == d) { run + 1 } else { 1 };
            r.best_streak = r.best_streak.max(run);
            prev = Some(d);
        }

        r.recent = sessions.iter().rev().take(8).cloned().collect();
        r
    }

    pub fn avg_daily_minutes(&self) -> i64 {
        if self.active_days == 0 { 0 } else { self.total_minutes / i64::from(self.active_days) }
    }

    pub fn peak_hour(&self) -> Option<usize> {
        let (h, m) = self.hours.iter().enumerate().max_by_key(|(_, m)| **m)?;
        (*m > 0).then_some(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn focus_on(d: NaiveDate, hour: u32, label: &str) -> Session {
        let start = Local.from_local_datetime(&d.and_hms_opt(hour, 0, 0).unwrap()).unwrap();
        Session {
            start: start.timestamp_millis(),
            end: start.timestamp_millis() + 25 * 60_000,
            phase: Phase::Focus,
            seconds: 25 * 60,
            label: label.into(),
        }
    }

    #[test]
    fn streaks_and_totals() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        let d = |n| today - Duration::days(n);
        let s = vec![
            focus_on(d(10), 9, "a"),
            focus_on(d(9), 9, "a"),
            focus_on(d(8), 9, "a"),
            focus_on(d(2), 14, "b"),
            focus_on(d(1), 14, "b"),
            focus_on(today, 14, ""),
            focus_on(today, 15, ""),
        ];
        let r = Report::build_for(&s, 8, today);
        assert_eq!(r.today_sessions, 2);
        assert_eq!(r.today_minutes, 50);
        assert_eq!(r.streak, 3);
        assert_eq!(r.best_streak, 3);
        assert_eq!(r.total_sessions, 7);
        assert_eq!(r.week.len(), 7);
        assert_eq!(r.week_minutes, 100);
        assert_eq!(r.prev_week_minutes, 75);
        assert_eq!(r.peak_hour(), Some(14));
        assert_eq!(r.heatmap.last().unwrap().date, today);
    }

    #[test]
    fn streak_survives_until_end_of_today() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        let s = vec![focus_on(today - Duration::days(1), 9, "")];
        assert_eq!(Report::build_for(&s, 8, today).streak, 1);
    }
}
