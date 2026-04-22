use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use crate::adb::{self, DeviceHandle};
use crate::dispatch::Event;

const POLL_INTERVAL_MS: u64 = 3000;

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub user: String,
    pub rss_kb: u64,
    pub name: String,
}

#[derive(Default)]
pub struct ProcessesState {
    pub processes: Vec<ProcessInfo>,
    pub last_error: Option<String>,
    pub selected: usize,
}

impl ProcessesState {
    pub fn replace(&mut self, processes: Vec<ProcessInfo>) {
        self.processes = processes;
        self.last_error = None;
        if self.selected >= self.processes.len() {
            self.selected = self.processes.len().saturating_sub(1);
        }
    }
}

pub fn spawn_poller(handle: DeviceHandle, tx: Sender<Event>) {
    thread::spawn(move || loop {
        match sample(&handle) {
            Ok(procs) => {
                if tx.send(Event::Processes(procs)).is_err() {
                    break;
                }
            }
            Err(e) => {
                let _ = tx.send(Event::Status {
                    text: format!("processes: {}", e),
                    error: true,
                });
            }
        }
        thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
    });
}

fn sample(handle: &DeviceHandle) -> Result<Vec<ProcessInfo>, String> {
    // toybox ps on Android: PID USER RSS NAME
    let output = adb::command(handle)
        .args(["shell", "ps", "-A", "-o", "PID,USER,RSS,NAME"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut out = Vec::new();
    for line in stdout.lines().skip(1) {
        let mut cols = line.split_whitespace();
        let Some(pid) = cols.next().and_then(|s| s.parse().ok()) else {
            continue;
        };
        let Some(user) = cols.next() else { continue };
        let Some(rss) = cols.next().and_then(|s| s.parse().ok()) else {
            continue;
        };
        let name: String = cols.collect::<Vec<_>>().join(" ");
        if name.is_empty() {
            continue;
        }
        out.push(ProcessInfo {
            pid,
            user: user.to_string(),
            rss_kb: rss,
            name,
        });
    }
    // sort by RSS desc
    out.sort_by(|a, b| b.rss_kb.cmp(&a.rss_kb));
    Ok(out)
}
