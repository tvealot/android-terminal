use std::process::Command;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use crate::dispatch::Event;

const POLL_INTERVAL_MS: u64 = 4000;

#[derive(Debug, Clone)]
pub struct DeviceEntry {
    pub serial: String,
    pub state: String,
    pub model: Option<String>,
    pub release: Option<String>,
    pub sdk: Option<String>,
    pub battery: Option<u8>,
}

impl DeviceEntry {
    pub fn is_ready(&self) -> bool {
        self.state == "device"
    }
}

pub fn list_all() -> Vec<DeviceEntry> {
    let Ok(output) = Command::new("adb").arg("devices").output() else {
        return Vec::new();
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries: Vec<DeviceEntry> = stdout
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?;
            let state = parts.next()?;
            Some(DeviceEntry {
                serial: serial.to_string(),
                state: state.to_string(),
                model: None,
                release: None,
                sdk: None,
                battery: None,
            })
        })
        .collect();
    for entry in entries.iter_mut() {
        if entry.is_ready() {
            entry.model = getprop(&entry.serial, "ro.product.model");
            entry.release = getprop(&entry.serial, "ro.build.version.release");
            entry.sdk = getprop(&entry.serial, "ro.build.version.sdk");
            entry.battery = battery(&entry.serial);
        }
    }
    entries
}

fn getprop(serial: &str, key: &str) -> Option<String> {
    let out = Command::new("adb")
        .args(["-s", serial, "shell", "getprop", key])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn battery(serial: &str) -> Option<u8> {
    let out = Command::new("adb")
        .args(["-s", serial, "shell", "dumpsys", "battery"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix("level:") {
            return v.trim().parse().ok();
        }
    }
    None
}

pub fn spawn_poller(tx: Sender<Event>) {
    thread::spawn(move || loop {
        let list = list_all();
        if tx.send(Event::Devices(list)).is_err() {
            break;
        }
        thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
    });
}
