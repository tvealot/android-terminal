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
    stdout
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?;
            let state = parts.next()?;
            Some(DeviceEntry {
                serial: serial.to_string(),
                state: state.to_string(),
            })
        })
        .collect()
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
