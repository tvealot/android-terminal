use std::process::Output;
use std::sync::mpsc::Sender;
use std::thread;

use crate::adb::{self, DeviceHandle};
use crate::dispatch::Event;

const MAX_PREVIEW_BYTES: usize = 64 * 1024;
const MAX_SQL_ROWS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppDataMode {
    Files,
    Databases,
    Preferences,
}

impl AppDataMode {
    pub fn label(self) -> &'static str {
        match self {
            AppDataMode::Files => "files",
            AppDataMode::Databases => "db",
            AppDataMode::Preferences => "prefs",
        }
    }
}

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
pub struct DatabaseEntry {
    pub name: String,
    pub path: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct DbTable {
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct DbTablePreview {
    pub database: String,
    pub table: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreferenceFileKind {
    SharedPreferences,
    DataStore,
}

#[derive(Debug, Clone)]
pub struct PreferenceFile {
    pub name: String,
    pub path: String,
    pub kind: PreferenceFileKind,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct PreferenceRow {
    pub key: String,
    pub value_type: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct PreferencePreview {
    pub file: PreferenceFile,
    pub rows: Vec<PreferenceRow>,
    pub message: Option<String>,
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
    DatabasesListed {
        package: String,
        databases: Vec<DatabaseEntry>,
    },
    TablesListed {
        package: String,
        database: String,
        tables: Vec<DbTable>,
    },
    TablePreviewed {
        package: String,
        preview: DbTablePreview,
    },
    PreferencesListed {
        package: String,
        files: Vec<PreferenceFile>,
    },
    PreferencePreviewed {
        package: String,
        preview: PreferencePreview,
    },
    Error {
        package: String,
        path: String,
        message: String,
    },
}

pub struct AppDataState {
    pub mode: AppDataMode,
    pub path: String,
    pub entries: Vec<DataEntry>,
    pub selected: usize,
    pub loading: bool,
    pub last_error: Option<String>,
    pub preview: Option<DataPreview>,
    pub preview_focused: bool,
    pub preview_scroll: usize,
    pub databases: Vec<DatabaseEntry>,
    pub db_selected: usize,
    pub current_database: Option<String>,
    pub tables: Vec<DbTable>,
    pub table_selected: usize,
    pub table_preview: Option<DbTablePreview>,
    pub preference_files: Vec<PreferenceFile>,
    pub pref_selected: usize,
    pub preference_preview: Option<PreferencePreview>,
}

impl Default for AppDataState {
    fn default() -> Self {
        Self {
            mode: AppDataMode::Files,
            path: ".".to_string(),
            entries: Vec::new(),
            selected: 0,
            loading: false,
            last_error: None,
            preview: None,
            preview_focused: false,
            preview_scroll: 0,
            databases: Vec::new(),
            db_selected: 0,
            current_database: None,
            tables: Vec::new(),
            table_selected: 0,
            table_preview: None,
            preference_files: Vec::new(),
            pref_selected: 0,
            preference_preview: None,
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
        self.databases.clear();
        self.db_selected = 0;
        self.current_database = None;
        self.tables.clear();
        self.table_selected = 0;
        self.table_preview = None;
        self.preference_files.clear();
        self.pref_selected = 0;
        self.preference_preview = None;
    }

    pub fn switch_mode(&mut self, mode: AppDataMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.last_error = None;
        self.preview = None;
        self.table_preview = None;
        self.preference_preview = None;
        self.preview_focused = false;
        self.preview_scroll = 0;
    }

    pub fn apply(&mut self, event: AppDataEvent) {
        self.loading = false;
        match event {
            AppDataEvent::Listed { path, entries, .. } => {
                self.mode = AppDataMode::Files;
                self.path = path;
                self.entries = entries;
                self.selected = self.selected.min(self.entries.len().saturating_sub(1));
                self.last_error = None;
                self.preview = None;
                self.table_preview = None;
                self.preference_preview = None;
                self.preview_focused = false;
                self.preview_scroll = 0;
            }
            AppDataEvent::Previewed { preview, .. } => {
                self.mode = AppDataMode::Files;
                self.preview = Some(preview);
                self.table_preview = None;
                self.preference_preview = None;
                self.preview_focused = true;
                self.preview_scroll = 0;
                self.last_error = None;
            }
            AppDataEvent::DatabasesListed { databases, .. } => {
                self.mode = AppDataMode::Databases;
                self.databases = databases;
                self.db_selected = self.db_selected.min(self.databases.len().saturating_sub(1));
                self.current_database = None;
                self.tables.clear();
                self.table_selected = 0;
                self.preview = None;
                self.table_preview = None;
                self.preference_preview = None;
                self.preview_focused = false;
                self.preview_scroll = 0;
                self.last_error = None;
            }
            AppDataEvent::TablesListed {
                database, tables, ..
            } => {
                self.mode = AppDataMode::Databases;
                self.current_database = Some(database);
                self.tables = tables;
                self.table_selected = self.table_selected.min(self.tables.len().saturating_sub(1));
                self.preview = None;
                self.table_preview = None;
                self.preference_preview = None;
                self.preview_focused = false;
                self.preview_scroll = 0;
                self.last_error = None;
            }
            AppDataEvent::TablePreviewed { preview, .. } => {
                self.mode = AppDataMode::Databases;
                self.preview = None;
                self.table_preview = Some(preview);
                self.preference_preview = None;
                self.preview_focused = true;
                self.preview_scroll = 0;
                self.last_error = None;
            }
            AppDataEvent::PreferencesListed { files, .. } => {
                self.mode = AppDataMode::Preferences;
                self.preference_files = files;
                self.pref_selected = self
                    .pref_selected
                    .min(self.preference_files.len().saturating_sub(1));
                self.preview = None;
                self.table_preview = None;
                self.preference_preview = None;
                self.preview_focused = false;
                self.preview_scroll = 0;
                self.last_error = None;
            }
            AppDataEvent::PreferencePreviewed { preview, .. } => {
                self.mode = AppDataMode::Preferences;
                self.preview = None;
                self.table_preview = None;
                self.preference_preview = Some(preview);
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
        self.table_preview = None;
        self.preference_preview = None;
        self.preview_focused = false;
        self.preview_scroll = 0;
    }

    pub fn parent_path(&self) -> Option<String> {
        parent_path(&self.path)
    }

    pub fn move_active_down(&mut self) {
        match self.mode {
            AppDataMode::Files => self.move_down(),
            AppDataMode::Databases => {
                if self.current_database.is_some() {
                    if !self.tables.is_empty() {
                        self.table_selected = (self.table_selected + 1).min(self.tables.len() - 1);
                    }
                } else if !self.databases.is_empty() {
                    self.db_selected = (self.db_selected + 1).min(self.databases.len() - 1);
                }
            }
            AppDataMode::Preferences => {
                if !self.preference_files.is_empty() {
                    self.pref_selected =
                        (self.pref_selected + 1).min(self.preference_files.len() - 1);
                }
            }
        }
    }

    pub fn move_active_up(&mut self) {
        match self.mode {
            AppDataMode::Files => self.move_up(),
            AppDataMode::Databases => {
                if self.current_database.is_some() {
                    self.table_selected = self.table_selected.saturating_sub(1);
                } else {
                    self.db_selected = self.db_selected.saturating_sub(1);
                }
            }
            AppDataMode::Preferences => {
                self.pref_selected = self.pref_selected.saturating_sub(1);
            }
        }
    }

    pub fn selected_database(&self) -> Option<&DatabaseEntry> {
        self.databases.get(self.db_selected)
    }

    pub fn selected_table(&self) -> Option<&DbTable> {
        self.tables.get(self.table_selected)
    }

    pub fn selected_preference_file(&self) -> Option<&PreferenceFile> {
        self.preference_files.get(self.pref_selected)
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

pub fn spawn_list_databases(handle: DeviceHandle, package: String, tx: Sender<Event>) {
    thread::spawn(move || {
        let event = match list_databases(&handle, &package) {
            Ok(databases) => AppDataEvent::DatabasesListed { package, databases },
            Err(message) => AppDataEvent::Error {
                package,
                path: "databases".to_string(),
                message,
            },
        };
        let _ = tx.send(Event::AppData(event));
    });
}

pub fn spawn_list_tables(
    handle: DeviceHandle,
    package: String,
    database: String,
    tx: Sender<Event>,
) {
    thread::spawn(move || {
        let event = match list_tables(&handle, &package, &database) {
            Ok(tables) => AppDataEvent::TablesListed {
                package,
                database,
                tables,
            },
            Err(message) => AppDataEvent::Error {
                package,
                path: database,
                message,
            },
        };
        let _ = tx.send(Event::AppData(event));
    });
}

pub fn spawn_preview_table(
    handle: DeviceHandle,
    package: String,
    database: String,
    table: String,
    tx: Sender<Event>,
) {
    thread::spawn(move || {
        let event = match preview_table(&handle, &package, &database, &table) {
            Ok(preview) => AppDataEvent::TablePreviewed { package, preview },
            Err(message) => AppDataEvent::Error {
                package,
                path: format!("{database}:{table}"),
                message,
            },
        };
        let _ = tx.send(Event::AppData(event));
    });
}

pub fn spawn_list_preferences(handle: DeviceHandle, package: String, tx: Sender<Event>) {
    thread::spawn(move || {
        let event = match list_preferences(&handle, &package) {
            Ok(files) => AppDataEvent::PreferencesListed { package, files },
            Err(message) => AppDataEvent::Error {
                package,
                path: "shared_prefs/files/datastore".to_string(),
                message,
            },
        };
        let _ = tx.send(Event::AppData(event));
    });
}

pub fn spawn_preview_preference(
    handle: DeviceHandle,
    package: String,
    file: PreferenceFile,
    tx: Sender<Event>,
) {
    thread::spawn(move || {
        let event = match preview_preference(&handle, &package, file.clone()) {
            Ok(preview) => AppDataEvent::PreferencePreviewed { package, preview },
            Err(message) => AppDataEvent::Error {
                package,
                path: file.path,
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

fn list_databases(handle: &DeviceHandle, package: &str) -> Result<Vec<DatabaseEntry>, String> {
    let entries = match list_path(handle, package, "databases") {
        Ok(entries) => entries,
        Err(message) if message.contains("No such file") || message.contains("No such") => {
            return Ok(Vec::new());
        }
        Err(message) => return Err(message),
    };
    let mut databases: Vec<DatabaseEntry> = entries
        .into_iter()
        .filter(|entry| entry.kind == DataEntryKind::File)
        .filter(|entry| {
            !entry.name.ends_with("-wal")
                && !entry.name.ends_with("-shm")
                && !entry.name.ends_with("-journal")
        })
        .map(|entry| DatabaseEntry {
            name: entry.name,
            path: entry.path,
            size_bytes: entry.size_bytes,
        })
        .collect();
    databases.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(databases)
}

fn list_tables(
    handle: &DeviceHandle,
    package: &str,
    database: &str,
) -> Result<Vec<DbTable>, String> {
    validate_package(package)?;
    let sql = "SELECT name,type FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY type,name;";
    let script = format!(
        "sqlite3 -separator '\t' {} {}",
        shell_quote(database),
        shell_quote(sql)
    );
    let output = run_as(handle, package, &script)?;
    if !output.status.success() {
        return Err(output_text(output));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_tables(&text))
}

fn preview_table(
    handle: &DeviceHandle,
    package: &str,
    database: &str,
    table: &str,
) -> Result<DbTablePreview, String> {
    validate_package(package)?;
    let sql = format!(
        "SELECT * FROM {} LIMIT {MAX_SQL_ROWS};",
        sql_identifier(table)
    );
    let script = format!(
        "sqlite3 -header -separator '\t' {} {}",
        shell_quote(database),
        shell_quote(&sql)
    );
    let output = run_as(handle, package, &script)?;
    if !output.status.success() {
        return Err(output_text(output));
    }
    Ok(parse_table_preview(
        database,
        table,
        &String::from_utf8_lossy(&output.stdout),
    ))
}

fn list_preferences(handle: &DeviceHandle, package: &str) -> Result<Vec<PreferenceFile>, String> {
    let mut files = Vec::new();
    if let Ok(entries) = list_path(handle, package, "shared_prefs") {
        files.extend(
            entries
                .into_iter()
                .filter(|entry| entry.kind == DataEntryKind::File && entry.name.ends_with(".xml"))
                .map(|entry| PreferenceFile {
                    name: entry.name,
                    path: entry.path,
                    kind: PreferenceFileKind::SharedPreferences,
                    size_bytes: entry.size_bytes,
                }),
        );
    }
    if let Ok(entries) = list_path(handle, package, "files/datastore") {
        files.extend(
            entries
                .into_iter()
                .filter(|entry| entry.kind == DataEntryKind::File)
                .map(|entry| PreferenceFile {
                    name: entry.name,
                    path: entry.path,
                    kind: PreferenceFileKind::DataStore,
                    size_bytes: entry.size_bytes,
                }),
        );
    }
    files.sort_by(|left, right| {
        preference_rank(&left.kind)
            .cmp(&preference_rank(&right.kind))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(files)
}

fn preference_rank(kind: &PreferenceFileKind) -> u8 {
    match kind {
        PreferenceFileKind::SharedPreferences => 0,
        PreferenceFileKind::DataStore => 1,
    }
}

fn preview_preference(
    handle: &DeviceHandle,
    package: &str,
    file: PreferenceFile,
) -> Result<PreferencePreview, String> {
    validate_package(package)?;
    let script = format!("cat {}", shell_quote(&file.path));
    let output = run_as(handle, package, &script)?;
    if !output.status.success() {
        return Err(output_text(output));
    }
    let (rows, message) = match file.kind {
        PreferenceFileKind::SharedPreferences => (
            parse_shared_preferences_xml(&String::from_utf8_lossy(&output.stdout)),
            None,
        ),
        PreferenceFileKind::DataStore => parse_datastore_preferences(&output.stdout),
    };
    Ok(PreferencePreview {
        file,
        rows,
        message,
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

fn parse_tables(text: &str) -> Vec<DbTable> {
    text.lines()
        .filter_map(|line| {
            let (name, kind) = line.split_once('\t')?;
            (!name.trim().is_empty()).then(|| DbTable {
                name: name.trim().to_string(),
                kind: kind.trim().to_string(),
            })
        })
        .collect()
}

fn parse_table_preview(database: &str, table: &str, text: &str) -> DbTablePreview {
    let mut lines = text.lines();
    let columns = lines
        .next()
        .map(split_tsv)
        .unwrap_or_else(|| vec!["(empty)".to_string()]);
    let rows = lines.map(split_tsv).collect();
    DbTablePreview {
        database: database.to_string(),
        table: table.to_string(),
        columns,
        rows,
    }
}

fn split_tsv(line: &str) -> Vec<String> {
    line.split('\t').map(str::to_string).collect()
}

fn parse_shared_preferences_xml(text: &str) -> Vec<PreferenceRow> {
    let mut rows = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        if rest.starts_with('/') || rest.starts_with('?') || rest.starts_with("map") {
            continue;
        }
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..end];
        let after = &rest[end + 1..];
        let self_closing = tag.trim_end().ends_with('/');
        let mut parts = tag
            .trim()
            .trim_end_matches('/')
            .splitn(2, char::is_whitespace);
        let Some(kind) = parts.next() else {
            rest = after;
            continue;
        };
        let attrs = parts.next().unwrap_or("");
        let Some(key) = attr_value(attrs, "name") else {
            rest = after;
            continue;
        };

        if kind == "set" {
            if let Some(close) = after.find("</set>") {
                let body = &after[..close];
                let values = parse_string_set(body);
                rows.push(PreferenceRow {
                    key,
                    value_type: "set".to_string(),
                    value: values.join(", "),
                });
                rest = &after[close + "</set>".len()..];
                continue;
            }
        }

        let value = if kind == "string" && !self_closing {
            if let Some(close) = after.find("</string>") {
                xml_unescape(&after[..close])
            } else {
                String::new()
            }
        } else {
            attr_value(attrs, "value").unwrap_or_default()
        };
        rows.push(PreferenceRow {
            key,
            value_type: kind.to_string(),
            value,
        });
        rest = after;
    }
    rows.sort_by(|left, right| left.key.to_lowercase().cmp(&right.key.to_lowercase()));
    rows
}

fn parse_string_set(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<string") {
        rest = &rest[start..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        let after = &rest[open_end + 1..];
        let Some(close) = after.find("</string>") else {
            break;
        };
        values.push(xml_unescape(&after[..close]));
        rest = &after[close + "</string>".len()..];
    }
    values
}

fn attr_value(attrs: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = attrs.find(&needle)? + needle.len();
    let end = attrs[start..].find('"')?;
    Some(xml_unescape(&attrs[start..start + end]))
}

fn xml_unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn parse_datastore_preferences(bytes: &[u8]) -> (Vec<PreferenceRow>, Option<String>) {
    match parse_datastore_preferences_inner(bytes) {
        Ok(mut rows) => {
            rows.sort_by(|left, right| left.key.to_lowercase().cmp(&right.key.to_lowercase()));
            (rows, None)
        }
        Err(message) => (Vec::new(), Some(message)),
    }
}

fn parse_datastore_preferences_inner(bytes: &[u8]) -> Result<Vec<PreferenceRow>, String> {
    let mut rows = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        let (tag, next) = read_varint(bytes, pos)?;
        pos = next;
        let field = tag >> 3;
        let wire = (tag & 0x07) as u8;
        if field == 1 && wire == 2 {
            let (len, next) = read_varint(bytes, pos)?;
            pos = next;
            let end = checked_end(pos, len as usize, bytes.len())?;
            if let Some(row) = parse_datastore_entry(&bytes[pos..end])? {
                rows.push(row);
            }
            pos = end;
        } else {
            pos = skip_proto_value(bytes, pos, wire)?;
        }
    }
    Ok(rows)
}

fn parse_datastore_entry(bytes: &[u8]) -> Result<Option<PreferenceRow>, String> {
    let mut key = None;
    let mut value = None;
    let mut pos = 0;
    while pos < bytes.len() {
        let (tag, next) = read_varint(bytes, pos)?;
        pos = next;
        let field = tag >> 3;
        let wire = (tag & 0x07) as u8;
        match (field, wire) {
            (1, 2) => {
                let (len, next) = read_varint(bytes, pos)?;
                pos = next;
                let end = checked_end(pos, len as usize, bytes.len())?;
                key = Some(String::from_utf8_lossy(&bytes[pos..end]).into_owned());
                pos = end;
            }
            (2, 2) => {
                let (len, next) = read_varint(bytes, pos)?;
                pos = next;
                let end = checked_end(pos, len as usize, bytes.len())?;
                value = Some(parse_datastore_value(&bytes[pos..end])?);
                pos = end;
            }
            _ => pos = skip_proto_value(bytes, pos, wire)?,
        }
    }
    Ok(match (key, value) {
        (Some(key), Some((value_type, value))) => Some(PreferenceRow {
            key,
            value_type,
            value,
        }),
        _ => None,
    })
}

fn parse_datastore_value(bytes: &[u8]) -> Result<(String, String), String> {
    let mut pos = 0;
    while pos < bytes.len() {
        let (tag, next) = read_varint(bytes, pos)?;
        pos = next;
        let field = tag >> 3;
        let wire = (tag & 0x07) as u8;
        match (field, wire) {
            (1, 0) => {
                let (v, _) = read_varint(bytes, pos)?;
                return Ok(("bool".to_string(), (v != 0).to_string()));
            }
            (2, 5) => {
                let end = checked_end(pos, 4, bytes.len())?;
                let v = f32::from_le_bytes(bytes[pos..end].try_into().unwrap());
                return Ok(("float".to_string(), v.to_string()));
            }
            (3, 1) => {
                let end = checked_end(pos, 8, bytes.len())?;
                let v = f64::from_le_bytes(bytes[pos..end].try_into().unwrap());
                return Ok(("double".to_string(), v.to_string()));
            }
            (4, 0) => {
                let (v, _) = read_varint(bytes, pos)?;
                return Ok(("int".to_string(), (v as u32 as i32).to_string()));
            }
            (5, 0) => {
                let (v, _) = read_varint(bytes, pos)?;
                return Ok(("long".to_string(), (v as i64).to_string()));
            }
            (6, 2) => {
                let (len, next) = read_varint(bytes, pos)?;
                pos = next;
                let end = checked_end(pos, len as usize, bytes.len())?;
                let value = String::from_utf8_lossy(&bytes[pos..end]).into_owned();
                return Ok(("string".to_string(), value));
            }
            (7, 2) => {
                let (len, next) = read_varint(bytes, pos)?;
                pos = next;
                let end = checked_end(pos, len as usize, bytes.len())?;
                return Ok((
                    "set".to_string(),
                    parse_datastore_string_set(&bytes[pos..end])?.join(", "),
                ));
            }
            (8, 2) => {
                let (len, _) = read_varint(bytes, pos)?;
                return Ok(("bytes".to_string(), format!("{} bytes", len)));
            }
            _ => pos = skip_proto_value(bytes, pos, wire)?,
        }
    }
    Ok(("unknown".to_string(), String::new()))
}

fn parse_datastore_string_set(bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut values = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        let (tag, next) = read_varint(bytes, pos)?;
        pos = next;
        let field = tag >> 3;
        let wire = (tag & 0x07) as u8;
        if field == 1 && wire == 2 {
            let (len, next) = read_varint(bytes, pos)?;
            pos = next;
            let end = checked_end(pos, len as usize, bytes.len())?;
            values.push(String::from_utf8_lossy(&bytes[pos..end]).into_owned());
            pos = end;
        } else {
            pos = skip_proto_value(bytes, pos, wire)?;
        }
    }
    Ok(values)
}

fn read_varint(bytes: &[u8], mut pos: usize) -> Result<(u64, usize), String> {
    let mut value = 0u64;
    let mut shift = 0;
    while pos < bytes.len() {
        let byte = bytes[pos];
        pos += 1;
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok((value, pos));
        }
        shift += 7;
        if shift >= 64 {
            return Err("invalid DataStore protobuf varint".to_string());
        }
    }
    Err("truncated DataStore protobuf".to_string())
}

fn skip_proto_value(bytes: &[u8], pos: usize, wire: u8) -> Result<usize, String> {
    match wire {
        0 => read_varint(bytes, pos).map(|(_, next)| next),
        1 => checked_end(pos, 8, bytes.len()),
        2 => {
            let (len, next) = read_varint(bytes, pos)?;
            checked_end(next, len as usize, bytes.len())
        }
        5 => checked_end(pos, 4, bytes.len()),
        _ => Err("unsupported DataStore protobuf wire type".to_string()),
    }
}

fn checked_end(pos: usize, len: usize, total: usize) -> Result<usize, String> {
    pos.checked_add(len)
        .filter(|end| *end <= total)
        .ok_or_else(|| "truncated DataStore protobuf".to_string())
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

fn sql_identifier(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shared_preferences_xml() {
        let xml = r#"
            <map>
                <string name="token">a&amp;b</string>
                <boolean name="enabled" value="true" />
                <int name="count" value="7" />
                <set name="tags"><string>one</string><string>two</string></set>
            </map>
        "#;

        let rows = parse_shared_preferences_xml(xml);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows.iter().find(|r| r.key == "token").unwrap().value, "a&b");
        assert_eq!(
            rows.iter().find(|r| r.key == "enabled").unwrap().value,
            "true"
        );
        assert_eq!(
            rows.iter().find(|r| r.key == "tags").unwrap().value,
            "one, two"
        );
    }

    #[test]
    fn parses_sqlite_tsv_preview() {
        let preview = parse_table_preview("databases/app.db", "users", "id\tname\n1\tAda\n");
        assert_eq!(preview.columns, vec!["id", "name"]);
        assert_eq!(preview.rows, vec![vec!["1".to_string(), "Ada".to_string()]]);
    }

    #[test]
    fn parses_datastore_preferences_proto() {
        let bytes = [
            0x0a, 0x0d, // preferences entry
            0x0a, 0x04, b'n', b'a', b'm', b'e', // key
            0x12, 0x05, // value message
            0x32, 0x03, b'A', b'd', b'a', // string value
        ];

        let (rows, message) = parse_datastore_preferences(&bytes);
        assert!(message.is_none());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, "name");
        assert_eq!(rows[0].value_type, "string");
        assert_eq!(rows[0].value, "Ada");
    }
}
