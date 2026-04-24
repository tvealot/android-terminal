use std::process::Output;
use std::sync::mpsc::Sender;
use std::thread;

use crate::adb::{self, DeviceHandle};
use crate::dispatch::Event;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    Launch,
    ForceStop,
    ClearData,
    OpenSettings,
    PackageInfo,
}

impl AppAction {
    pub fn label(self) -> &'static str {
        match self {
            AppAction::Launch => "launch",
            AppAction::ForceStop => "force stop",
            AppAction::ClearData => "clear data",
            AppAction::OpenSettings => "open settings",
            AppAction::PackageInfo => "package info",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            AppAction::Launch => "Start launcher activity through monkey",
            AppAction::ForceStop => "Stop the app process and services",
            AppAction::ClearData => "Delete private app data via pm clear",
            AppAction::OpenSettings => "Open Android app details settings",
            AppAction::PackageInfo => "Read dumpsys package summary",
        }
    }

    pub fn destructive(self) -> bool {
        matches!(self, AppAction::ClearData)
    }
}

pub const ACTIONS: &[AppAction] = &[
    AppAction::Launch,
    AppAction::ForceStop,
    AppAction::ClearData,
    AppAction::OpenSettings,
    AppAction::PackageInfo,
];

#[derive(Debug, Clone)]
pub struct AppActionResult {
    pub action: AppAction,
    pub package: String,
    pub success: bool,
    pub summary: String,
    pub output: String,
}

#[derive(Default)]
pub struct AppControlState {
    pub selected: usize,
    pub running: bool,
    pub pending_confirm: Option<AppAction>,
    pub last: Option<AppActionResult>,
}

impl AppControlState {
    pub fn move_down(&mut self) {
        if !ACTIONS.is_empty() {
            self.selected = (self.selected + 1).min(ACTIONS.len() - 1);
        }
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn selected_action(&self) -> AppAction {
        ACTIONS
            .get(self.selected)
            .copied()
            .unwrap_or(AppAction::Launch)
    }
}

pub fn spawn_action(handle: DeviceHandle, package: String, action: AppAction, tx: Sender<Event>) {
    thread::spawn(move || {
        let result = run_action(&handle, package, action);
        let _ = tx.send(Event::AppControl(result));
    });
}

fn run_action(handle: &DeviceHandle, package: String, action: AppAction) -> AppActionResult {
    if let Err(message) = validate_package(&package) {
        return AppActionResult {
            action,
            package,
            success: false,
            summary: message.clone(),
            output: message,
        };
    }

    let output = match action {
        AppAction::Launch => adb::command(handle)
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
        AppAction::ForceStop => adb::command(handle)
            .args(["shell", "am", "force-stop", &package])
            .output(),
        AppAction::ClearData => adb::command(handle)
            .args(["shell", "pm", "clear", &package])
            .output(),
        AppAction::OpenSettings => {
            let uri = format!("package:{package}");
            adb::command(handle)
                .args([
                    "shell",
                    "am",
                    "start",
                    "-a",
                    "android.settings.APPLICATION_DETAILS_SETTINGS",
                    "-d",
                    &uri,
                ])
                .output()
        }
        AppAction::PackageInfo => adb::command(handle)
            .args(["shell", "dumpsys", "package", &package])
            .output(),
    };

    match output {
        Ok(output) => build_result(action, package, output),
        Err(err) => AppActionResult {
            action,
            package,
            success: false,
            summary: format!("{} failed: {}", action.label(), err),
            output: err.to_string(),
        },
    }
}

fn build_result(action: AppAction, package: String, output: Output) -> AppActionResult {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let raw = if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else if stdout.trim().is_empty() {
        stderr.trim().to_string()
    } else {
        format!("{}\n{}", stdout.trim(), stderr.trim())
    };
    let success = output.status.success();
    let output = if matches!(action, AppAction::PackageInfo) {
        summarize_package_info(&raw)
    } else {
        raw
    };
    let detail = output.lines().next().unwrap_or("").trim();
    let summary = if success {
        if detail.is_empty() {
            format!("{}: {}", package, action.label())
        } else {
            format!("{}: {}", action.label(), detail)
        }
    } else if detail.is_empty() {
        format!("{} failed with {}", action.label(), output_status(&output))
    } else {
        format!("{} failed: {}", action.label(), detail)
    };

    AppActionResult {
        action,
        package,
        success,
        summary,
        output,
    }
}

fn output_status(output: &str) -> String {
    if output.is_empty() {
        "non-zero exit".to_string()
    } else {
        output.to_string()
    }
}

fn summarize_package_info(raw: &str) -> String {
    let mut lines = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Package [")
            || trimmed.starts_with("versionCode=")
            || trimmed.starts_with("versionName=")
            || trimmed.starts_with("firstInstallTime=")
            || trimmed.starts_with("lastUpdateTime=")
            || trimmed.starts_with("targetSdk=")
            || trimmed.starts_with("minSdk=")
            || trimmed.starts_with("dataDir=")
            || trimmed.starts_with("installerPackageName=")
            || trimmed.starts_with("grantedPermissions:")
        {
            lines.push(trimmed.to_string());
        }
        if trimmed.starts_with("android.permission.") {
            lines.push(format!("  {trimmed}"));
        }
        if lines.len() >= 80 {
            break;
        }
    }

    if lines.is_empty() {
        raw.lines()
            .take(80)
            .map(str::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        lines.join("\n")
    }
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
