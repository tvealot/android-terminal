use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::adb::{self, DeviceHandle};
use crate::dispatch::Event;

const HISTORY: usize = 60;
const POLL_INTERVAL_MS: u64 = 2000;

pub type FpsPackageHandle = Arc<Mutex<Option<String>>>;

pub fn new_package_handle() -> FpsPackageHandle {
    Arc::new(Mutex::new(None))
}

fn current_package(handle: &FpsPackageHandle) -> Option<String> {
    handle.lock().ok().and_then(|g| g.clone())
}

#[derive(Debug, Clone, Default)]
pub struct FpsSample {
    pub total_frames: u64,
    pub janky_frames: u64,
    pub janky_percent: f32,
    pub p50_ms: f32,
    pub p90_ms: f32,
    pub p95_ms: f32,
    pub p99_ms: f32,
    pub missed_vsync: u64,
    pub high_input_latency: u64,
    pub slow_ui: u64,
    pub slow_bitmap: u64,
    pub slow_draw: u64,
}

#[derive(Default)]
pub struct FpsState {
    pub package_handle: FpsPackageHandle,
    pub samples: VecDeque<FpsSample>,
    pub last_error: Option<String>,
}

impl FpsState {
    pub fn new(handle: FpsPackageHandle) -> Self {
        Self {
            package_handle: handle,
            samples: VecDeque::new(),
            last_error: None,
        }
    }

    pub fn push(&mut self, sample: FpsSample) {
        if self.samples.len() >= HISTORY {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
        self.last_error = None;
    }

    pub fn latest(&self) -> Option<&FpsSample> {
        self.samples.back()
    }

    pub fn current_package(&self) -> Option<String> {
        current_package(&self.package_handle)
    }

    pub fn set_package(&mut self, pkg: Option<String>) {
        if let Ok(mut g) = self.package_handle.lock() {
            *g = pkg;
        }
        self.samples.clear();
        self.last_error = None;
    }
}

pub fn spawn_poller(device: DeviceHandle, pkg: FpsPackageHandle, tx: Sender<Event>) {
    thread::spawn(move || {
        let mut last_pkg: Option<String> = None;
        loop {
            let Some(p) = current_package(&pkg) else {
                last_pkg = None;
                thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
                continue;
            };
            if last_pkg.as_ref() != Some(&p) {
                let _ = reset(&device, &p);
                last_pkg = Some(p.clone());
                thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
                continue;
            }
            match sample(&device, &p) {
                Ok(s) => {
                    if tx.send(Event::Fps(s)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Event::Status {
                        text: format!("fps: {}", e),
                        error: true,
                    });
                }
            }
            let _ = reset(&device, &p);
            thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }
    });
}

fn reset(device: &DeviceHandle, pkg: &str) -> Result<(), String> {
    let out = adb::command(device)
        .args(["shell", "dumpsys", "gfxinfo", pkg, "reset"])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(())
}

fn sample(device: &DeviceHandle, pkg: &str) -> Result<FpsSample, String> {
    let out = adb::command(device)
        .args(["shell", "dumpsys", "gfxinfo", pkg])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(parse(&text))
}

fn parse(raw: &str) -> FpsSample {
    let mut s = FpsSample::default();
    for line in raw.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix("Total frames rendered:") {
            s.total_frames = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = t.strip_prefix("Janky frames:") {
            // "15 (3.64%)"
            let v = v.trim();
            let mut it = v.split_whitespace();
            if let Some(n) = it.next() {
                s.janky_frames = n.parse().unwrap_or(0);
            }
            if let Some(pct) = it.next() {
                let cleaned = pct.trim_matches(|c: char| c == '(' || c == ')' || c == '%');
                s.janky_percent = cleaned.parse().unwrap_or(0.0);
            }
        } else if let Some(v) = t.strip_prefix("50th percentile:") {
            s.p50_ms = parse_ms(v);
        } else if let Some(v) = t.strip_prefix("90th percentile:") {
            s.p90_ms = parse_ms(v);
        } else if let Some(v) = t.strip_prefix("95th percentile:") {
            s.p95_ms = parse_ms(v);
        } else if let Some(v) = t.strip_prefix("99th percentile:") {
            s.p99_ms = parse_ms(v);
        } else if let Some(v) = t.strip_prefix("Number Missed Vsync:") {
            s.missed_vsync = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = t.strip_prefix("Number High input latency:") {
            s.high_input_latency = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = t.strip_prefix("Number Slow UI thread:") {
            s.slow_ui = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = t.strip_prefix("Number Slow bitmap uploads:") {
            s.slow_bitmap = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = t.strip_prefix("Number Slow issue draw commands:") {
            s.slow_draw = v.trim().parse().unwrap_or(0);
        }
    }
    s
}

fn parse_ms(v: &str) -> f32 {
    v.trim()
        .trim_end_matches("ms")
        .trim()
        .parse()
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gfxinfo_output() {
        let raw = "\
** Graphics info for pid 1234 [com.example] **

Stats since: 9999999ns
Total frames rendered: 412
Janky frames: 15 (3.64%)
50th percentile: 6ms
90th percentile: 12ms
95th percentile: 15ms
99th percentile: 28ms
Number Missed Vsync: 3
Number High input latency: 1
Number Slow UI thread: 10
Number Slow bitmap uploads: 0
Number Slow issue draw commands: 2
";
        let s = parse(raw);
        assert_eq!(s.total_frames, 412);
        assert_eq!(s.janky_frames, 15);
        assert!((s.janky_percent - 3.64).abs() < 0.01);
        assert!((s.p50_ms - 6.0).abs() < 0.01);
        assert!((s.p99_ms - 28.0).abs() < 0.01);
        assert_eq!(s.missed_vsync, 3);
        assert_eq!(s.slow_draw, 2);
    }
}
