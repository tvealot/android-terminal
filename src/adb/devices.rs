use std::process::Command;

use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use crate::dispatch::Event;

#[allow(dead_code)]
pub fn list() -> Vec<String> {
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
            if state == "device" {
                Some(serial.to_string())
            } else {
                None
            }
        })
        .collect()
}

pub fn spawn(tx: Sender<Event>) {
    thread::spawn(move || {
        let mut previous: Option<Vec<String>> = None;
        loop {
            let devices = list();
            if previous.as_ref() != Some(&devices) {
                if tx.send(Event::Devices(devices.clone())).is_err() {
                    break;
                }
                previous = Some(devices);
            }
            thread::sleep(Duration::from_secs(3));
        }
    });
}
