use std::collections::VecDeque;
use std::process::Command;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use crate::dispatch::Event;

const HISTORY: usize = 60;
const POLL_INTERVAL_MS: u64 = 2000;

#[derive(Debug, Clone, Copy, Default)]
pub struct MonitorSample {
    pub battery_percent: u8,
    pub battery_temp_c: f32,
    pub mem_total_kb: u64,
    pub mem_available_kb: u64,
}

impl MonitorSample {
    pub fn mem_used_kb(&self) -> u64 {
        self.mem_total_kb.saturating_sub(self.mem_available_kb)
    }

    pub fn mem_used_percent(&self) -> f32 {
        if self.mem_total_kb == 0 {
            return 0.0;
        }
        (self.mem_used_kb() as f32 / self.mem_total_kb as f32) * 100.0
    }
}

#[derive(Default)]
pub struct MonitorState {
    pub samples: VecDeque<MonitorSample>,
    pub last_error: Option<String>,
}

impl MonitorState {
    pub fn push(&mut self, sample: MonitorSample) {
        if self.samples.len() >= HISTORY {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
        self.last_error = None;
    }

    pub fn latest(&self) -> Option<&MonitorSample> {
        self.samples.back()
    }
}

pub fn spawn_poller(tx: Sender<Event>) {
    thread::spawn(move || loop {
        match sample() {
            Ok(s) => {
                if tx.send(Event::Monitor(s)).is_err() {
                    break;
                }
            }
            Err(e) => {
                let _ = tx.send(Event::Status {
                    text: format!("monitor: {}", e),
                    error: true,
                });
            }
        }
        thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
    });
}

fn sample() -> Result<MonitorSample, String> {
    let battery_raw = adb_shell(&["dumpsys", "battery"])?;
    let (level, temp) = parse_battery(&battery_raw);
    let meminfo_raw = adb_shell(&["cat", "/proc/meminfo"])?;
    let (total, available) = parse_meminfo(&meminfo_raw);
    Ok(MonitorSample {
        battery_percent: level,
        battery_temp_c: temp,
        mem_total_kb: total,
        mem_available_kb: available,
    })
}

fn adb_shell(args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("adb");
    cmd.arg("shell");
    for a in args {
        cmd.arg(a);
    }
    let output = cmd.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_battery(raw: &str) -> (u8, f32) {
    let mut level = 0u8;
    let mut temp = 0.0f32;
    for line in raw.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("level:") {
            level = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("temperature:") {
            // dumpsys reports tenths of degrees C (e.g. 283 = 28.3°C)
            let raw: f32 = v.trim().parse().unwrap_or(0.0);
            temp = raw / 10.0;
        }
    }
    (level, temp)
}

fn parse_meminfo(raw: &str) -> (u64, u64) {
    let mut total = 0u64;
    let mut available = 0u64;
    for line in raw.lines() {
        if let Some(v) = line.strip_prefix("MemTotal:") {
            total = parse_kb(v);
        } else if let Some(v) = line.strip_prefix("MemAvailable:") {
            available = parse_kb(v);
        }
    }
    (total, available)
}

fn parse_kb(s: &str) -> u64 {
    s.trim()
        .split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}
