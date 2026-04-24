use std::process::Output;
use std::sync::mpsc::Sender;
use std::thread;

use crate::adb::{self, DeviceHandle};
use crate::dispatch::Event;

#[derive(Debug, Clone)]
pub struct IntentResult {
    pub url: String,
    pub package: Option<String>,
    pub success: bool,
    pub summary: String,
    pub output: String,
}

#[derive(Default)]
pub struct IntentsState {
    pub url: String,
    pub use_target_package: bool,
    pub running: bool,
    pub last: Option<IntentResult>,
    pub history: Vec<String>,
}

impl IntentsState {
    pub fn set_url(&mut self, url: String) {
        self.url = url;
    }

    pub fn remember(&mut self, url: &str) {
        if url.is_empty() {
            return;
        }
        self.history.retain(|item| item != url);
        self.history.insert(0, url.to_string());
        self.history.truncate(8);
    }
}

pub fn spawn_launch(handle: DeviceHandle, url: String, package: Option<String>, tx: Sender<Event>) {
    thread::spawn(move || {
        let result = launch(&handle, url, package);
        let _ = tx.send(Event::Intent(result));
    });
}

fn launch(handle: &DeviceHandle, url: String, package: Option<String>) -> IntentResult {
    if url.trim().is_empty() {
        return IntentResult {
            url,
            package,
            success: false,
            summary: "deep link URL is empty".to_string(),
            output: "deep link URL is empty".to_string(),
        };
    }
    if let Some(package) = package.as_deref() {
        if let Err(message) = validate_package(package) {
            return IntentResult {
                url,
                package: Some(package.to_string()),
                success: false,
                summary: message.clone(),
                output: message,
            };
        }
    }

    let mut cmd = adb::command(handle);
    cmd.args([
        "shell",
        "am",
        "start",
        "-a",
        "android.intent.action.VIEW",
        "-d",
        &url,
    ]);
    if let Some(package) = package.as_deref() {
        cmd.args(["-p", package]);
    }

    match cmd.output() {
        Ok(output) => build_result(url, package, output),
        Err(err) => IntentResult {
            url,
            package,
            success: false,
            summary: format!("intent failed: {}", err),
            output: err.to_string(),
        },
    }
}

fn build_result(url: String, package: Option<String>, output: Output) -> IntentResult {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else if stdout.trim().is_empty() {
        stderr.trim().to_string()
    } else {
        format!("{}\n{}", stdout.trim(), stderr.trim())
    };
    let success = output.status.success();
    let first = text.lines().next().unwrap_or("").trim();
    let summary = if success {
        if first.is_empty() {
            "intent launched".to_string()
        } else {
            first.to_string()
        }
    } else if first.is_empty() {
        "intent failed".to_string()
    } else {
        format!("intent failed: {first}")
    };
    IntentResult {
        url,
        package,
        success,
        summary,
        output: text,
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
