use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::SystemTime;

use chrono::{DateTime, Local};

use crate::dispatch::Event;

#[derive(Debug, Clone)]
pub struct ProjectEntry {
    pub path: PathBuf,
    pub display: String,
    pub modified: SystemTime,
}

impl ProjectEntry {
    pub fn modified_label(&self) -> String {
        DateTime::<Local>::from(self.modified)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    }
}

pub struct ProjectPicker {
    pub root: PathBuf,
    pub entries: Vec<ProjectEntry>,
    pub selected: usize,
    pub loading: bool,
}

impl ProjectPicker {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            entries: Vec::new(),
            selected: 0,
            loading: true,
        }
    }
}

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".gradle",
    ".git",
    ".idea",
    ".vscode",
    "build",
    "Pods",
    "DerivedData",
    ".dart_tool",
    ".kotlin",
    "target",
    ".cxx",
    ".cache",
    "Library",
];

const MAX_DEPTH: usize = 5;
const MAX_ENTRIES: usize = 256;

pub fn default_root() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join("Documents"))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn spawn_scan(root: PathBuf, tx: Sender<Event>) {
    thread::spawn(move || {
        let mut out = Vec::new();
        walk(&root, 0, &mut out);
        out.sort_by(|a, b| b.modified.cmp(&a.modified));
        out.truncate(MAX_ENTRIES);
        let _ = tx.send(Event::Projects(out));
    });
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<ProjectEntry>) {
    if depth > MAX_DEPTH {
        return;
    }
    if dir.join("gradlew").is_file() {
        let modified = fs::metadata(dir)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        out.push(ProjectEntry {
            path: dir.to_path_buf(),
            display: display_path(dir),
            modified,
        });
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        if name_s.starts_with('.') {
            continue;
        }
        if SKIP_DIRS.iter().any(|s| *s == name_s.as_ref()) {
            continue;
        }
        walk(&entry.path(), depth + 1, out);
    }
}

fn display_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rel) = path.strip_prefix(&home) {
            return format!("~/{}", rel.display());
        }
    }
    path.display().to_string()
}
