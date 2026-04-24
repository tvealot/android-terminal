use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;

use crate::dispatch::Event;

pub struct EmulatorPicker {
    pub entries: Vec<String>,
    pub selected: usize,
    pub loading: bool,
}

impl EmulatorPicker {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected: 0,
            loading: true,
        }
    }
}

pub fn emulator_binary() -> Option<PathBuf> {
    if Command::new("emulator")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Some(PathBuf::from("emulator"));
    }
    for var in ["ANDROID_SDK_ROOT", "ANDROID_HOME"] {
        if let Ok(root) = std::env::var(var) {
            let path = PathBuf::from(root).join("emulator").join("emulator");
            if path.is_file() {
                return Some(path);
            }
        }
    }
    if let Some(home) = dirs::home_dir() {
        for rel in ["Library/Android/sdk/emulator/emulator", "Android/Sdk/emulator/emulator"] {
            let path = home.join(rel);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

pub fn spawn_scan(tx: Sender<Event>) {
    thread::spawn(move || match list_avds() {
        Ok(list) => {
            let _ = tx.send(Event::Emulators(list));
        }
        Err(e) => {
            let _ = tx.send(Event::Emulators(Vec::new()));
            let _ = tx.send(Event::Status {
                text: format!("emulator: {}", e),
                error: true,
            });
        }
    });
}

fn list_avds() -> Result<Vec<String>, String> {
    let bin = emulator_binary().ok_or_else(|| "emulator binary not found (set ANDROID_SDK_ROOT)".to_string())?;
    let out = Command::new(&bin)
        .arg("-list-avds")
        .output()
        .map_err(|e| format!("{}: {}", bin.display(), e))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut avds: Vec<String> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("INFO"))
        .map(|l| l.to_string())
        .collect();
    avds.sort();
    Ok(avds)
}

pub fn launch(avd: &str) -> Result<(), String> {
    let bin = emulator_binary().ok_or_else(|| "emulator binary not found".to_string())?;
    Command::new(&bin)
        .arg("-avd")
        .arg(avd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{}: {}", bin.display(), e))
}
