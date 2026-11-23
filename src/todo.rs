//! Task list stored in `$XDG_DATA_HOME/cosmic-doro/todos.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Todo {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub done: bool,
    /// Estimated pomodoros.
    #[serde(default = "one")]
    pub estimate: u32,
    /// Completed focus sessions spent on this task.
    #[serde(default)]
    pub pomodoros: u32,
    pub created: i64,
    #[serde(default)]
    pub completed_at: Option<i64>,
}

fn one() -> u32 {
    1
}

pub fn path() -> PathBuf {
    crate::paths::data_dir().join("todos.json")
}

pub fn file_mtime() -> Option<std::time::SystemTime> {
    crate::paths::mtime(&path())
}

pub fn load() -> Vec<Todo> {
    std::fs::read(path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(todos: &[Todo]) {
    if let Ok(bytes) = serde_json::to_vec_pretty(todos) {
        let _ = crate::paths::write_atomic(&path(), &bytes);
    }
}

pub fn next_id(todos: &[Todo]) -> u64 {
    todos.iter().map(|t| t.id).max().unwrap_or(0) + 1
}

/// Records one finished focus session against a task.
pub fn add_pomodoro(id: u64) {
    let mut todos = load();
    if let Some(t) = todos.iter_mut().find(|t| t.id == id) {
        t.pomodoros += 1;
        save(&todos);
    }
}
