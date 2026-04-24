use std::process::Output;
use std::sync::mpsc::Sender;
use std::thread;

use crate::adb::{self, DeviceHandle};
use crate::dispatch::Event;

const MAX_PREVIEW_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataEntryKind {
    Directory,
    File,
    Other,
}

#[derive(Debug, Clone)]
pub struct DataEntry {
    pub name: String,
    pub path: String,
    pub kind: DataEntryKind,
    pub size_bytes: Option<u64>,
    pub meta: String,
}

#[derive(Debug, Clone)]
pub struct DataPreview {
    pub path: String,
    pub content: String,
    pub truncated: bool,
    pub binary: bool,
}

#[derive(Debug, Clone)]
pub enum AppDataEvent {
    Listed {
        package: String,
        path: String,
        entries: Vec<DataEntry>,
    },
    Previewed {
        package: String,
        preview: DataPreview,
    },
    Error {
        package: String,
        path: String,
        message: String,
    },
}

pub struct AppDataState {
    pub path: String,
    pub entries: Vec<DataEntry>,
    pub selected: usize,
    pub loading: bool,
    pub last_error: Option<String>,
    pub preview: Option<DataPreview>,
    pub preview_focused: bool,
    pub preview_scroll: usize,
}

impl Default for AppDataState {
    fn default() -> Self {
        Self {
            path: ".".to_string(),
            entries: Vec::new(),
            selected: 0,
            loading: false,
            last_error: None,
            preview: None,
            preview_focused: false,
            preview_scroll: 0,
        }
    }
}

impl AppDataState {
    pub fn reset_for_package(&mut self) {
        self.path = ".".to_string();
        self.entries.clear();
        self.selected = 0;
        self.loading = false;
        self.last_error = None;
        self.preview = None;
        self.preview_focused = false;
        self.preview_scroll = 0;
    }

    pub fn apply(&mut self, event: AppDataEvent) {
        self.loading = false;
        match event {
            AppDataEvent::Listed { path, entries, .. } => {
                self.path = path;
                self.entries = entries;
                self.selected = self.selected.min(self.entries.len().saturating_sub(1));
                self.last_error = None;
                self.preview = None;
                self.preview_focused = false;
                self.preview_scroll = 0;
            }
            AppDataEvent::Previewed { preview, .. } => {
                self.preview = Some(preview);
                self.preview_focused = true;
                self.preview_scroll = 0;
                self.last_error = None;
            }
            AppDataEvent::Error { message, .. } => {
                self.last_error = Some(message);
            }
        }
    }

    pub fn move_down(&mut self) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + 1).min(self.entries.len() - 1);
        }
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn selected_entry(&self) -> Option<&DataEntry> {
        self.entries.get(self.selected)
    }

    pub fn close_preview(&mut self) {
        self.preview = None;
        self.preview_focused = false;
        self.preview_scroll = 0;
    }

    pub fn parent_path(&self) -> Option<String> {
        parent_path(&self.path)
    }
}

pub fn spawn_list(handle: DeviceHandle, package: String, path: String, tx: Sender<Event>) {
    thread::spawn(move || {
        let event = match list_path(&handle, &package, &path) {
            Ok(entries) => AppDataEvent::Listed {
                package,
                path,
                entries,
            },
            Err(message) => AppDataEvent::Error {
                package,
                path,
                message,
            },
        };
        let _ = tx.send(Event::AppData(event));
    });
}

pub fn spawn_preview(handle: DeviceHandle, package: String, path: String, tx: Sender<Event>) {
    thread::spawn(move || {
        let event = match preview_path(&handle, &package, &path) {
            Ok(preview) => AppDataEvent::Previewed { package, preview },
            Err(message) => AppDataEvent::Error {
                package,
                path,
                message,
            },
        };
        let _ = tx.send(Event::AppData(event));
    });
}

fn list_path(handle: &DeviceHandle, package: &str, path: &str) -> Result<Vec<DataEntry>, String> {
    validate_package(package)?;
    let script = format!("ls -la {}", shell_quote(path));
    let output = run_as(handle, package, &script)?;
    if !output.status.success() {
        return Err(output_text(output));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_ls(path, &text))
}

fn preview_path(handle: &DeviceHandle, package: &str, path: &str) -> Result<DataPreview, String> {
    validate_package(package)?;
    let script = format!("head -c {} {}", MAX_PREVIEW_BYTES, shell_quote(path));
    let output = run_as(handle, package, &script)?;
    if !output.status.success() {
        return Err(output_text(output));
    }
    let bytes = output.stdout;
    let binary = bytes.contains(&0);
    let truncated = bytes.len() >= MAX_PREVIEW_BYTES;
    Ok(DataPreview {
        path: path.to_string(),
        content: String::from_utf8_lossy(&bytes).into_owned(),
        truncated,
        binary,
    })
}

fn run_as(handle: &DeviceHandle, package: &str, script: &str) -> Result<Output, String> {
    adb::command(handle)
        .args(["shell", "run-as", package, "sh", "-c", script])
        .output()
        .map_err(|e| e.to_string())
}

fn parse_ls(parent: &str, text: &str) -> Vec<DataEntry> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("total ") {
            continue;
        }
        let cols: Vec<&str> = trimmed.split_whitespace().collect();
        if cols.len() < 8 {
            continue;
        }
        let mode = cols[0];
        let mut name = cols[7..].join(" ");
        if name == "." || name == ".." {
            continue;
        }
        if let Some((link_name, _)) = name.split_once(" -> ") {
            name = link_name.to_string();
        }
        let kind = match mode.chars().next() {
            Some('d') => DataEntryKind::Directory,
            Some('-') => DataEntryKind::File,
            _ => DataEntryKind::Other,
        };
        let size_bytes = cols.get(4).and_then(|s| s.parse().ok());
        let date = cols.get(5).copied().unwrap_or("");
        let time = cols.get(6).copied().unwrap_or("");
        entries.push(DataEntry {
            path: join_path(parent, &name),
            name,
            kind,
            size_bytes,
            meta: format!("{mode} {date} {time}"),
        });
    }
    entries.sort_by(|left, right| {
        entry_rank(&left.kind)
            .cmp(&entry_rank(&right.kind))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    entries
}

fn entry_rank(kind: &DataEntryKind) -> u8 {
    match kind {
        DataEntryKind::Directory => 0,
        DataEntryKind::File => 1,
        DataEntryKind::Other => 2,
    }
}

fn join_path(parent: &str, name: &str) -> String {
    if parent == "." || parent.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", parent.trim_end_matches('/'), name)
    }
}

fn parent_path(path: &str) -> Option<String> {
    if path == "." || path.is_empty() {
        return None;
    }
    let trimmed = path.trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some((parent, _)) if !parent.is_empty() => Some(parent.to_string()),
        _ => Some(".".to_string()),
    }
}

fn output_text(output: Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let msg = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    if msg.is_empty() {
        "adb/run-as command failed".to_string()
    } else {
        msg.to_string()
    }
}

fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn validate_package(package: &str) -> Result<(), String> {
    if package.trim().is_empty() {
        return Err("target package is empty".to_string());
    }
    let valid = package
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.'));
    if valid {
        Ok(())
    } else {
        Err("target package contains unsupported characters".to_string())
    }
}
