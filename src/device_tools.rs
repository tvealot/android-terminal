use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, SystemTime};

use chrono::Local;
use regex::Regex;

use crate::adb::{self, DeviceHandle};
use crate::dispatch::Event;

const MAX_DEPTH: usize = 5;
const MAX_PROJECTS: usize = 256;
const MAX_SCAN_FILES: usize = 1600;
const RECORD_SECONDS: &str = "30";

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

#[derive(Debug, Clone)]
pub struct WorkPackage {
    pub package: String,
    pub project_dir: PathBuf,
    pub project_name: String,
    pub source: String,
}

impl WorkPackage {
    pub fn new(package: String, project_dir: PathBuf, source: String) -> Self {
        let project_name = project_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| project_dir.display().to_string());
        Self {
            package,
            project_dir,
            project_name,
            source,
        }
    }

    pub fn path_label(&self) -> String {
        display_path(&self.project_dir)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceToolAction {
    Scrcpy,
    ScreenRecord,
    WifiAdb,
    InstallLatestApk,
    Launch,
    ForceStop,
    ClearData,
    Uninstall,
}

impl DeviceToolAction {
    pub fn label(self) -> &'static str {
        match self {
            DeviceToolAction::Scrcpy => "scrcpy",
            DeviceToolAction::ScreenRecord => "screenrecord",
            DeviceToolAction::WifiAdb => "wifi adb",
            DeviceToolAction::InstallLatestApk => "install apk",
            DeviceToolAction::Launch => "launch",
            DeviceToolAction::ForceStop => "force stop",
            DeviceToolAction::ClearData => "clear data",
            DeviceToolAction::Uninstall => "uninstall",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDeviceTool {
    pub action: DeviceToolAction,
    pub package: String,
}

#[derive(Debug, Clone)]
pub struct DeviceToolResult {
    pub action: DeviceToolAction,
    pub package: Option<String>,
    pub success: bool,
    pub summary: String,
    pub output: String,
}

pub struct DeviceToolsDialog {
    pub scan_roots: Vec<PathBuf>,
    pub packages: Vec<WorkPackage>,
    pub selected: usize,
    pub loading: bool,
    pub running: bool,
    pub pending_confirm: Option<PendingDeviceTool>,
    pub last: Option<DeviceToolResult>,
}

impl DeviceToolsDialog {
    pub fn new(scan_roots: Vec<PathBuf>, seed_packages: Vec<WorkPackage>) -> Self {
        Self {
            scan_roots,
            packages: seed_packages,
            selected: 0,
            loading: true,
            running: false,
            pending_confirm: None,
            last: None,
        }
    }

    pub fn move_down(&mut self) {
        if !self.packages.is_empty() {
            self.selected = (self.selected + 1).min(self.packages.len() - 1);
        }
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn selected_package(&self) -> Option<&WorkPackage> {
        self.packages.get(self.selected)
    }

    pub fn replace_packages(&mut self, packages: Vec<WorkPackage>) {
        self.packages = packages;
        if self.packages.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(self.packages.len() - 1);
        }
        self.loading = false;
    }
}

pub fn default_root() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join("Documents"))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn spawn_scan(roots: Vec<PathBuf>, seed_packages: Vec<WorkPackage>, tx: Sender<Event>) {
    thread::spawn(move || {
        let packages = scan_packages(roots, seed_packages);
        let _ = tx.send(Event::DeviceToolPackages(packages));
    });
}

pub fn spawn_scrcpy(handle: DeviceHandle, tx: Sender<Event>) {
    thread::spawn(move || {
        let result = launch_scrcpy(&handle);
        let _ = tx.send(Event::DeviceTool(result));
    });
}

pub fn spawn_screenrecord(handle: DeviceHandle, tx: Sender<Event>) {
    thread::spawn(move || {
        let result = screenrecord(&handle);
        let _ = tx.send(Event::DeviceTool(result));
    });
}

pub fn spawn_wifi_adb(handle: DeviceHandle, tx: Sender<Event>) {
    thread::spawn(move || {
        let result = wifi_adb(&handle);
        let _ = tx.send(Event::DeviceTool(result));
    });
}

pub fn spawn_install_latest_apk(handle: DeviceHandle, package: WorkPackage, tx: Sender<Event>) {
    thread::spawn(move || {
        let result = install_latest_apk(&handle, package);
        let _ = tx.send(Event::DeviceTool(result));
    });
}

pub fn spawn_launch(handle: DeviceHandle, package: String, tx: Sender<Event>) {
    thread::spawn(move || {
        let result = package_action(&handle, DeviceToolAction::Launch, package);
        let _ = tx.send(Event::DeviceTool(result));
    });
}

pub fn spawn_force_stop(handle: DeviceHandle, package: String, tx: Sender<Event>) {
    thread::spawn(move || {
        let result = package_action(&handle, DeviceToolAction::ForceStop, package);
        let _ = tx.send(Event::DeviceTool(result));
    });
}

pub fn spawn_clear_data(handle: DeviceHandle, package: String, tx: Sender<Event>) {
    thread::spawn(move || {
        let result = package_action(&handle, DeviceToolAction::ClearData, package);
        let _ = tx.send(Event::DeviceTool(result));
    });
}

pub fn spawn_uninstall(handle: DeviceHandle, package: String, tx: Sender<Event>) {
    thread::spawn(move || {
        let result = uninstall(&handle, package);
        let _ = tx.send(Event::DeviceTool(result));
    });
}

fn scan_packages(roots: Vec<PathBuf>, seed_packages: Vec<WorkPackage>) -> Vec<WorkPackage> {
    let mut projects = Vec::new();
    let mut seen_projects = HashSet::new();
    for root in roots {
        walk_projects(&root, 0, &mut seen_projects, &mut projects);
        if projects.len() >= MAX_PROJECTS {
            break;
        }
    }

    let mut out = seed_packages;
    let mut seen_packages: HashSet<(String, PathBuf)> = out
        .iter()
        .map(|p| (p.package.clone(), p.project_dir.clone()))
        .collect();

    for project in projects {
        for (package, source) in packages_from_project(&project) {
            let key = (package.clone(), project.clone());
            if seen_packages.insert(key) {
                out.push(WorkPackage::new(package, project.clone(), source));
            }
        }
    }

    out.sort_by(|a, b| {
        a.project_name
            .to_lowercase()
            .cmp(&b.project_name.to_lowercase())
            .then_with(|| a.package.cmp(&b.package))
            .then_with(|| a.source.cmp(&b.source))
    });
    out
}

fn walk_projects(dir: &Path, depth: usize, seen: &mut HashSet<PathBuf>, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || out.len() >= MAX_PROJECTS {
        return;
    }
    if dir.join("gradlew").is_file() {
        let path = dir.to_path_buf();
        if seen.insert(path.clone()) {
            out.push(path);
        }
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
        if name_s.starts_with('.') || SKIP_DIRS.iter().any(|s| *s == name_s.as_ref()) {
            continue;
        }
        walk_projects(&entry.path(), depth + 1, seen, out);
        if out.len() >= MAX_PROJECTS {
            break;
        }
    }
}

fn packages_from_project(project: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    collect_candidate_files(project, 0, &mut files);

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for file in files {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        for (package, kind) in extract_packages(&text, file.file_name().and_then(|n| n.to_str())) {
            if !is_valid_package(&package) || !seen.insert(package.clone()) {
                continue;
            }
            let source = file
                .strip_prefix(project)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| file.display().to_string());
            out.push((package, format!("{kind}: {source}")));
        }
    }
    out
}

fn collect_candidate_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || out.len() >= MAX_SCAN_FILES {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            let name = entry.file_name();
            let name_s = name.to_string_lossy();
            if name_s.starts_with('.') || SKIP_DIRS.iter().any(|s| *s == name_s.as_ref()) {
                continue;
            }
            collect_candidate_files(&path, depth + 1, out);
        } else if is_candidate_file(&path) {
            out.push(path);
        }
        if out.len() >= MAX_SCAN_FILES {
            break;
        }
    }
}

fn is_candidate_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    matches!(
        name,
        "build.gradle" | "build.gradle.kts" | "AndroidManifest.xml"
    )
}

fn extract_packages(text: &str, filename: Option<&str>) -> Vec<(String, &'static str)> {
    if filename == Some("AndroidManifest.xml") {
        return extract_manifest_packages(text);
    }
    let mut out = Vec::new();
    let app_id = Regex::new(
        r#"(?m)\bapplicationId(?:\s*=\s*|\s+|\.set\(\s*)["']([A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z0-9_]+)+)["']"#,
    )
    .expect("valid applicationId regex");
    for cap in app_id.captures_iter(text) {
        out.push((cap[1].to_string(), "applicationId"));
    }
    if out.is_empty() {
        let namespace = Regex::new(
            r#"(?m)\bnamespace(?:\s*=\s*|\s+|\.set\(\s*)["']([A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z0-9_]+)+)["']"#,
        )
        .expect("valid namespace regex");
        for cap in namespace.captures_iter(text) {
            out.push((cap[1].to_string(), "namespace"));
        }
    }
    out
}

fn extract_manifest_packages(text: &str) -> Vec<(String, &'static str)> {
    let package =
        Regex::new(r#"\bpackage\s*=\s*["']([A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z0-9_]+)+)["']"#)
            .expect("valid manifest package regex");
    package
        .captures_iter(text)
        .map(|cap| (cap[1].to_string(), "manifest"))
        .collect()
}

fn launch_scrcpy(handle: &DeviceHandle) -> DeviceToolResult {
    let serial = adb::serial_of(handle);
    let mut cmd = Command::new("scrcpy");
    if let Some(serial) = serial.as_deref() {
        cmd.arg("-s").arg(serial);
    }
    match cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(_) => DeviceToolResult {
            action: DeviceToolAction::Scrcpy,
            package: None,
            success: true,
            summary: serial
                .map(|s| format!("scrcpy launched for {s}"))
                .unwrap_or_else(|| "scrcpy launched".to_string()),
            output: "scrcpy process started".to_string(),
        },
        Err(err) => DeviceToolResult {
            action: DeviceToolAction::Scrcpy,
            package: None,
            success: false,
            summary: format!("scrcpy failed: {err}"),
            output: err.to_string(),
        },
    }
}

fn screenrecord(handle: &DeviceHandle) -> DeviceToolResult {
    match screenrecord_inner(handle) {
        Ok((summary, output)) => DeviceToolResult {
            action: DeviceToolAction::ScreenRecord,
            package: None,
            success: true,
            summary,
            output,
        },
        Err(message) => DeviceToolResult {
            action: DeviceToolAction::ScreenRecord,
            package: None,
            success: false,
            summary: format!("screenrecord failed: {message}"),
            output: message,
        },
    }
}

fn screenrecord_inner(handle: &DeviceHandle) -> Result<(String, String), String> {
    let local = artifact_path("device-screenrecord", "mp4")?;
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let remote = format!("/sdcard/droidscope-device-screenrecord-{stamp}.mp4");
    let record = adb::command(handle)
        .args([
            "shell",
            "screenrecord",
            "--time-limit",
            RECORD_SECONDS,
            &remote,
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !record.status.success() {
        return Err(output_text(&record));
    }
    let pull = adb::command(handle)
        .args(["pull", &remote, path_str(&local)?])
        .output()
        .map_err(|e| e.to_string())?;
    let _ = adb::command(handle)
        .args(["shell", "rm", "-f", &remote])
        .output();
    if !pull.status.success() {
        return Err(output_text(&pull));
    }
    Ok((
        format!("screenrecord saved: {}", local.display()),
        output_text(&pull),
    ))
}

fn wifi_adb(handle: &DeviceHandle) -> DeviceToolResult {
    match wifi_adb_inner(handle) {
        Ok((summary, output)) => DeviceToolResult {
            action: DeviceToolAction::WifiAdb,
            package: None,
            success: true,
            summary,
            output,
        },
        Err(message) => DeviceToolResult {
            action: DeviceToolAction::WifiAdb,
            package: None,
            success: false,
            summary: format!("wifi adb failed: {message}"),
            output: message,
        },
    }
}

fn wifi_adb_inner(handle: &DeviceHandle) -> Result<(String, String), String> {
    let ip = device_ip(handle)?;
    let tcpip = adb::command(handle)
        .args(["tcpip", "5555"])
        .output()
        .map_err(|e| e.to_string())?;
    if !tcpip.status.success() {
        return Err(output_text(&tcpip));
    }
    thread::sleep(Duration::from_millis(900));
    let endpoint = format!("{ip}:5555");
    let connect = Command::new("adb")
        .args(["connect", &endpoint])
        .output()
        .map_err(|e| e.to_string())?;
    let mut output = output_text(&tcpip);
    let connect_text = output_text(&connect);
    if !connect_text.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&connect_text);
    }
    if !connect.status.success() {
        return Err(output);
    }
    Ok((format!("wifi adb connected: {endpoint}"), output))
}

fn device_ip(handle: &DeviceHandle) -> Result<String, String> {
    let route = adb::command(handle)
        .args(["shell", "ip", "route"])
        .output()
        .map_err(|e| e.to_string())?;
    if route.status.success() {
        let text = String::from_utf8_lossy(&route.stdout);
        if let Some(ip) = parse_route_ip(&text) {
            return Ok(ip);
        }
    }
    let addr = adb::command(handle)
        .args(["shell", "ip", "-f", "inet", "addr", "show", "wlan0"])
        .output()
        .map_err(|e| e.to_string())?;
    if addr.status.success() {
        let text = String::from_utf8_lossy(&addr.stdout);
        if let Some(ip) = parse_inet_ip(&text) {
            return Ok(ip);
        }
    }
    Err("could not detect device Wi-Fi IP".to_string())
}

fn parse_route_ip(text: &str) -> Option<String> {
    text.lines()
        .find(|line| line.contains(" wlan0 ") && line.contains(" src "))
        .or_else(|| text.lines().find(|line| line.contains(" src ")))
        .and_then(|line| line.split(" src ").nth(1))
        .and_then(|tail| tail.split_whitespace().next())
        .filter(|ip| ip.contains('.'))
        .map(str::to_string)
}

fn parse_inet_ip(text: &str) -> Option<String> {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|pair| pair[0] == "inet")
        .and_then(|pair| pair[1].split('/').next())
        .filter(|ip| ip.contains('.'))
        .map(str::to_string)
}

fn install_latest_apk(handle: &DeviceHandle, package: WorkPackage) -> DeviceToolResult {
    match latest_apk(&package.project_dir) {
        Some(apk) => {
            let output = adb::command(handle)
                .arg("install")
                .arg("-r")
                .arg(&apk.path)
                .output();
            match output {
                Ok(output) => {
                    let raw = output_text(&output);
                    let success = output.status.success();
                    DeviceToolResult {
                        action: DeviceToolAction::InstallLatestApk,
                        package: Some(package.package.clone()),
                        success,
                        summary: if success {
                            format!("installed {}", apk.path.display())
                        } else {
                            format!("install failed: {}", apk.path.display())
                        },
                        output: if raw.is_empty() {
                            apk.path.display().to_string()
                        } else {
                            format!("{}\n{}", apk.path.display(), raw)
                        },
                    }
                }
                Err(err) => DeviceToolResult {
                    action: DeviceToolAction::InstallLatestApk,
                    package: Some(package.package.clone()),
                    success: false,
                    summary: format!("install failed: {err}"),
                    output: err.to_string(),
                },
            }
        }
        None => DeviceToolResult {
            action: DeviceToolAction::InstallLatestApk,
            package: Some(package.package.clone()),
            success: false,
            summary: format!("no APK found under {}", package.project_dir.display()),
            output: "build the project first, then run install apk again".to_string(),
        },
    }
}

struct ApkCandidate {
    path: PathBuf,
    modified: SystemTime,
}

fn latest_apk(project_dir: &Path) -> Option<ApkCandidate> {
    let mut apks = Vec::new();
    collect_apks(project_dir, 0, &mut apks);
    apks.into_iter().max_by(|a, b| a.modified.cmp(&b.modified))
}

fn collect_apks(dir: &Path, depth: usize, out: &mut Vec<ApkCandidate>) {
    if depth > 9 || out.len() >= 2048 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            let name = entry.file_name();
            let name_s = name.to_string_lossy();
            if name_s.starts_with('.') || matches!(name_s.as_ref(), ".git" | ".gradle") {
                continue;
            }
            collect_apks(&path, depth + 1, out);
        } else if is_output_apk(&path) {
            let modified = fs::metadata(&path)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            out.push(ApkCandidate { path, modified });
        }
    }
}

fn is_output_apk(path: &Path) -> bool {
    if path.extension().and_then(|e| e.to_str()) != Some("apk") {
        return false;
    }
    let parts: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    parts
        .windows(3)
        .any(|w| w[0] == "build" && w[1] == "outputs" && w[2] == "apk")
}

fn package_action(
    handle: &DeviceHandle,
    action: DeviceToolAction,
    package: String,
) -> DeviceToolResult {
    if !is_valid_package(&package) {
        return DeviceToolResult {
            action,
            package: Some(package),
            success: false,
            summary: "package contains unsupported characters".to_string(),
            output: "package may contain only letters, digits, dot, underscore".to_string(),
        };
    }
    let output = match action {
        DeviceToolAction::Launch => adb::command(handle)
            .args([
                "shell",
                "monkey",
                "-p",
                &package,
                "-c",
                "android.intent.category.LAUNCHER",
                "1",
            ])
            .output(),
        DeviceToolAction::ForceStop => adb::command(handle)
            .args(["shell", "am", "force-stop", &package])
            .output(),
        DeviceToolAction::ClearData => adb::command(handle)
            .args(["shell", "pm", "clear", &package])
            .output(),
        _ => unreachable!("unsupported package action"),
    };
    match output {
        Ok(output) => build_package_result(action, package, output),
        Err(err) => DeviceToolResult {
            action,
            package: Some(package.clone()),
            success: false,
            summary: format!("{} failed for {}: {}", action.label(), package, err),
            output: err.to_string(),
        },
    }
}

fn build_package_result(
    action: DeviceToolAction,
    package: String,
    output: Output,
) -> DeviceToolResult {
    let raw = output_text(&output);
    let success = output.status.success();
    let detail = raw.lines().next().unwrap_or("").trim();
    let summary = if success {
        if detail.is_empty() {
            format!("{}: {}", action.label(), package)
        } else {
            format!("{}: {}", action.label(), detail)
        }
    } else if detail.is_empty() {
        format!("{} failed for {}", action.label(), package)
    } else {
        format!("{} failed for {}: {}", action.label(), package, detail)
    };
    DeviceToolResult {
        action,
        package: Some(package),
        success,
        summary,
        output: raw,
    }
}

fn uninstall(handle: &DeviceHandle, package: String) -> DeviceToolResult {
    if !is_valid_package(&package) {
        return DeviceToolResult {
            action: DeviceToolAction::Uninstall,
            package: Some(package),
            success: false,
            summary: "package contains unsupported characters".to_string(),
            output: "package may contain only letters, digits, dot, underscore".to_string(),
        };
    }
    let output = adb::command(handle).args(["uninstall", &package]).output();
    match output {
        Ok(output) => {
            let raw = output_text(&output);
            let success = output.status.success();
            let detail = raw.lines().next().unwrap_or("").trim();
            DeviceToolResult {
                action: DeviceToolAction::Uninstall,
                package: Some(package.clone()),
                success,
                summary: if success {
                    format!("uninstalled {package}")
                } else if detail.is_empty() {
                    format!("uninstall failed for {package}")
                } else {
                    format!("uninstall failed for {package}: {detail}")
                },
                output: raw,
            }
        }
        Err(err) => DeviceToolResult {
            action: DeviceToolAction::Uninstall,
            package: Some(package.clone()),
            success: false,
            summary: format!("uninstall failed for {package}: {err}"),
            output: err.to_string(),
        },
    }
}

fn artifact_path(prefix: &str, ext: &str) -> Result<PathBuf, String> {
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let filename = format!("droidscope-{prefix}-{stamp}.{ext}");
    std::env::current_dir()
        .map(|dir| dir.join(filename))
        .map_err(|e| e.to_string())
}

fn path_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))
}

fn output_text(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else if stdout.trim().is_empty() {
        stderr.trim().to_string()
    } else {
        format!("{}\n{}", stdout.trim(), stderr.trim())
    }
}

fn is_valid_package(package: &str) -> bool {
    !package.trim().is_empty()
        && package
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.'))
        && package.contains('.')
}

fn display_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rel) = path.strip_prefix(&home) {
            return format!("~/{}", rel.display());
        }
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_gradle_application_id() {
        let text = r#"
            android {
                namespace = "com.example.lib"
                defaultConfig {
                    applicationId "com.example.app"
                }
            }
        "#;
        let packages = extract_packages(text, Some("build.gradle"));
        assert_eq!(
            packages,
            vec![("com.example.app".to_string(), "applicationId")]
        );
    }

    #[test]
    fn extracts_kts_namespace_when_application_id_missing() {
        let text = r#"
            android {
                namespace.set("com.example.feature")
            }
        "#;
        let packages = extract_packages(text, Some("build.gradle.kts"));
        assert_eq!(
            packages,
            vec![("com.example.feature".to_string(), "namespace")]
        );
    }

    #[test]
    fn extracts_manifest_package() {
        let text = r#"<manifest package="com.example.legacy" />"#;
        let packages = extract_packages(text, Some("AndroidManifest.xml"));
        assert_eq!(
            packages,
            vec![("com.example.legacy".to_string(), "manifest")]
        );
    }

    #[test]
    fn parses_wifi_ip_from_route() {
        let text = "192.168.1.0/24 dev wlan0 proto kernel scope link src 192.168.1.42";
        assert_eq!(parse_route_ip(text), Some("192.168.1.42".to_string()));
    }
}
