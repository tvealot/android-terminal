use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::adb::{self, DeviceHandle};
use crate::dispatch::Event;

const HISTORY: usize = 60;
const POLL_INTERVAL_MS: u64 = 2000;
const CLK_TCK: u64 = 100;
const GC_DROP_THRESHOLD_KB: u64 = 512;

pub type PerfPackageHandle = Arc<Mutex<Option<String>>>;

pub fn new_package_handle() -> PerfPackageHandle {
    Arc::new(Mutex::new(None))
}

fn current_package(handle: &PerfPackageHandle) -> Option<String> {
    handle.lock().ok().and_then(|g| g.clone())
}

#[derive(Debug, Clone, Default)]
pub struct PerfSample {
    pub pid: u32,
    pub pss_total_kb: u64,
    pub rss_total_kb: u64,
    pub java_heap_kb: u64,
    pub native_heap_kb: u64,
    pub code_kb: u64,
    pub stack_kb: u64,
    pub graphics_kb: u64,
    pub private_other_kb: u64,
    pub system_kb: u64,
    pub dalvik_heap_alloc_kb: u64,
    pub native_heap_alloc_kb: u64,
    pub cpu_percent: f32,
    pub jank_percent: f32,
    pub frames_total: u64,
    pub p50_ms: f32,
    pub p90_ms: f32,
    pub p95_ms: f32,
    pub p99_ms: f32,
    pub gc_markers: u32,
    pub gc_delta: u32,
}

#[derive(Default)]
pub struct PerfState {
    pub package_handle: PerfPackageHandle,
    pub samples: VecDeque<PerfSample>,
    pub last_error: Option<String>,
}

impl PerfState {
    pub fn new(handle: PerfPackageHandle) -> Self {
        Self {
            package_handle: handle,
            samples: VecDeque::new(),
            last_error: None,
        }
    }

    pub fn push(&mut self, sample: PerfSample) {
        if self.samples.len() >= HISTORY {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
        self.last_error = None;
    }

    pub fn latest(&self) -> Option<&PerfSample> {
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

struct CpuPrev {
    at: Instant,
    ticks: u64,
    pid: u32,
}

pub fn spawn_poller(device: DeviceHandle, pkg: PerfPackageHandle, tx: Sender<Event>) {
    thread::spawn(move || {
        let mut prev_cpu: Option<CpuPrev> = None;
        let mut prev_dalvik_alloc: Option<u64> = None;
        let mut gc_markers: u32 = 0;
        let mut last_pkg: Option<String> = None;
        loop {
            let Some(p) = current_package(&pkg) else {
                last_pkg = None;
                prev_cpu = None;
                prev_dalvik_alloc = None;
                gc_markers = 0;
                thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
                continue;
            };
            if last_pkg.as_ref() != Some(&p) {
                prev_cpu = None;
                prev_dalvik_alloc = None;
                gc_markers = 0;
                last_pkg = Some(p.clone());
            }
            match sample(
                &device,
                &p,
                &mut prev_cpu,
                &mut prev_dalvik_alloc,
                &mut gc_markers,
            ) {
                Ok(s) => {
                    if tx.send(Event::Perf(s)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Event::Status {
                        text: format!("perf: {}", e),
                        error: true,
                    });
                }
            }
            thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }
    });
}

fn sample(
    device: &DeviceHandle,
    pkg: &str,
    prev_cpu: &mut Option<CpuPrev>,
    prev_dalvik_alloc: &mut Option<u64>,
    gc_markers: &mut u32,
) -> Result<PerfSample, String> {
    let pid = pidof(device, pkg)?;
    let mem_raw = shell(device, &["dumpsys", "meminfo", pkg])?;
    let mem = parse_meminfo(&mem_raw);
    let gfx_raw = shell(device, &["dumpsys", "gfxinfo", pkg])?;
    let gfx = parse_gfxinfo(&gfx_raw);
    let stat_raw = shell(device, &["cat", &format!("/proc/{}/stat", pid)])?;
    let ticks = parse_proc_stat_ticks(&stat_raw);
    let now = Instant::now();
    let cpu_percent = match (prev_cpu.as_ref(), ticks) {
        (Some(p), Some(ticks1)) if p.pid == pid => {
            let dt = now.duration_since(p.at).as_secs_f64();
            if dt > 0.0 {
                let delta = ticks1.saturating_sub(p.ticks) as f64;
                ((delta / CLK_TCK as f64) / dt * 100.0) as f32
            } else {
                0.0
            }
        }
        _ => 0.0,
    };
    if let Some(ticks1) = ticks {
        *prev_cpu = Some(CpuPrev {
            at: now,
            ticks: ticks1,
            pid,
        });
    }

    let mut gc_delta: u32 = 0;
    if let Some(prev) = *prev_dalvik_alloc {
        if prev > mem.dalvik_heap_alloc_kb
            && prev - mem.dalvik_heap_alloc_kb >= GC_DROP_THRESHOLD_KB
        {
            gc_delta = 1;
            *gc_markers = gc_markers.saturating_add(1);
        }
    }
    *prev_dalvik_alloc = Some(mem.dalvik_heap_alloc_kb);

    Ok(PerfSample {
        pid,
        pss_total_kb: mem.pss_total_kb,
        rss_total_kb: mem.rss_total_kb,
        java_heap_kb: mem.java_heap_kb,
        native_heap_kb: mem.native_heap_kb,
        code_kb: mem.code_kb,
        stack_kb: mem.stack_kb,
        graphics_kb: mem.graphics_kb,
        private_other_kb: mem.private_other_kb,
        system_kb: mem.system_kb,
        dalvik_heap_alloc_kb: mem.dalvik_heap_alloc_kb,
        native_heap_alloc_kb: mem.native_heap_alloc_kb,
        cpu_percent,
        jank_percent: gfx.jank_percent,
        frames_total: gfx.frames_total,
        p50_ms: gfx.p50_ms,
        p90_ms: gfx.p90_ms,
        p95_ms: gfx.p95_ms,
        p99_ms: gfx.p99_ms,
        gc_markers: *gc_markers,
        gc_delta,
    })
}

fn pidof(device: &DeviceHandle, pkg: &str) -> Result<u32, String> {
    let raw = shell(device, &["pidof", "-s", pkg])?;
    raw.split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "process not running".to_string())
}

fn shell(device: &DeviceHandle, args: &[&str]) -> Result<String, String> {
    let mut cmd = adb::command(device);
    cmd.arg("shell");
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[derive(Debug, Default)]
struct MemInfo {
    pss_total_kb: u64,
    rss_total_kb: u64,
    java_heap_kb: u64,
    native_heap_kb: u64,
    code_kb: u64,
    stack_kb: u64,
    graphics_kb: u64,
    private_other_kb: u64,
    system_kb: u64,
    dalvik_heap_alloc_kb: u64,
    native_heap_alloc_kb: u64,
}

fn parse_meminfo(raw: &str) -> MemInfo {
    let mut m = MemInfo::default();
    let mut in_summary = false;
    for line in raw.lines() {
        let t = line.trim();
        if t.starts_with("App Summary") {
            in_summary = true;
            continue;
        }
        if in_summary {
            if let Some(v) = t.strip_prefix("Java Heap:") {
                m.java_heap_kb = first_u64(v);
            } else if let Some(v) = t.strip_prefix("Native Heap:") {
                m.native_heap_kb = first_u64(v);
            } else if let Some(v) = t.strip_prefix("Code:") {
                m.code_kb = first_u64(v);
            } else if let Some(v) = t.strip_prefix("Stack:") {
                m.stack_kb = first_u64(v);
            } else if let Some(v) = t.strip_prefix("Graphics:") {
                m.graphics_kb = first_u64(v);
            } else if let Some(v) = t.strip_prefix("Private Other:") {
                m.private_other_kb = first_u64(v);
            } else if let Some(v) = t.strip_prefix("System:") {
                m.system_kb = first_u64(v);
            } else if let Some(v) = t.strip_prefix("TOTAL PSS:") {
                m.pss_total_kb = first_u64(v);
                if let Some(rss_idx) = v.find("TOTAL RSS:") {
                    let rest = &v[rss_idx + "TOTAL RSS:".len()..];
                    m.rss_total_kb = first_u64(rest);
                }
            }
        } else if let Some(rest) = t.strip_prefix("Native Heap") {
            let cols = parse_numbers(rest);
            if cols.len() >= 7 {
                m.native_heap_alloc_kb = cols[6];
            }
            if m.rss_total_kb == 0 && cols.len() >= 5 {
                // cols[4] = Rss Total for that row; we want total elsewhere.
            }
        } else if let Some(rest) = t.strip_prefix("Dalvik Heap") {
            let cols = parse_numbers(rest);
            if cols.len() >= 7 {
                m.dalvik_heap_alloc_kb = cols[6];
            }
        } else if let Some(rest) = t.strip_prefix("TOTAL") {
            if m.pss_total_kb == 0 {
                let cols = parse_numbers(rest);
                if !cols.is_empty() {
                    m.pss_total_kb = cols[0];
                }
                if cols.len() >= 5 && m.rss_total_kb == 0 {
                    m.rss_total_kb = cols[4];
                }
            }
        }
    }
    m
}

#[derive(Debug, Default)]
struct GfxInfo {
    frames_total: u64,
    jank_percent: f32,
    p50_ms: f32,
    p90_ms: f32,
    p95_ms: f32,
    p99_ms: f32,
}

fn parse_gfxinfo(raw: &str) -> GfxInfo {
    let mut g = GfxInfo::default();
    for line in raw.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix("Total frames rendered:") {
            g.frames_total = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = t.strip_prefix("Janky frames:") {
            let mut it = v.trim().split_whitespace();
            it.next();
            if let Some(pct) = it.next() {
                let cleaned = pct.trim_matches(|c: char| c == '(' || c == ')' || c == '%');
                g.jank_percent = cleaned.parse().unwrap_or(0.0);
            }
        } else if let Some(v) = t.strip_prefix("50th percentile:") {
            g.p50_ms = parse_ms(v);
        } else if let Some(v) = t.strip_prefix("90th percentile:") {
            g.p90_ms = parse_ms(v);
        } else if let Some(v) = t.strip_prefix("95th percentile:") {
            g.p95_ms = parse_ms(v);
        } else if let Some(v) = t.strip_prefix("99th percentile:") {
            g.p99_ms = parse_ms(v);
        }
    }
    g
}

fn parse_ms(v: &str) -> f32 {
    v.trim()
        .trim_end_matches("ms")
        .trim()
        .parse()
        .unwrap_or(0.0)
}

fn parse_proc_stat_ticks(raw: &str) -> Option<u64> {
    let idx = raw.rfind(')')?;
    let rest = &raw[idx + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

fn parse_numbers(s: &str) -> Vec<u64> {
    s.split_whitespace()
        .filter_map(|w| w.parse::<u64>().ok())
        .collect()
}

fn first_u64(s: &str) -> u64 {
    s.split_whitespace()
        .find_map(|w| w.parse::<u64>().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_meminfo_app_summary() {
        let raw = "\
** MEMINFO in pid 12345 [com.example] **
                   Pss  Private  Private  SwapPss      Rss     Heap     Heap     Heap
                 Total    Dirty    Clean    Dirty    Total     Size    Alloc     Free
                ------   ------   ------   ------   ------   ------   ------   ------
  Native Heap     9324     9324        0        0    19832    12288    10284     2004
  Dalvik Heap     4216     4108        0        0     9704    11376     6232     5144
     TOTAL       41212    32776     1308       20   119567    23664    16516     7148

 App Summary
                       Pss(KB)                        Rss(KB)
                        ------                         ------
           Java Heap:     7728                          14868
         Native Heap:     9324                          19832
                Code:     9292                          65128
               Stack:       40                             44
            Graphics:    14020                          14020
       Private Other:     2068
              System:     3620

           TOTAL PSS:    41212            TOTAL RSS:   120192       TOTAL SWAP PSS:       20
";
        let m = parse_meminfo(raw);
        assert_eq!(m.pss_total_kb, 41212);
        assert_eq!(m.rss_total_kb, 120192);
        assert_eq!(m.java_heap_kb, 7728);
        assert_eq!(m.native_heap_kb, 9324);
        assert_eq!(m.code_kb, 9292);
        assert_eq!(m.stack_kb, 40);
        assert_eq!(m.graphics_kb, 14020);
        assert_eq!(m.private_other_kb, 2068);
        assert_eq!(m.system_kb, 3620);
        assert_eq!(m.dalvik_heap_alloc_kb, 6232);
        assert_eq!(m.native_heap_alloc_kb, 10284);
    }

    #[test]
    fn parses_gfxinfo() {
        let raw = "\
Total frames rendered: 412
Janky frames: 15 (3.64%)
50th percentile: 6ms
90th percentile: 12ms
95th percentile: 15ms
99th percentile: 28ms
";
        let g = parse_gfxinfo(raw);
        assert_eq!(g.frames_total, 412);
        assert!((g.jank_percent - 3.64).abs() < 0.01);
        assert!((g.p90_ms - 12.0).abs() < 0.01);
        assert!((g.p99_ms - 28.0).abs() < 0.01);
    }

    #[test]
    fn parses_proc_stat_ticks() {
        // pid (comm with space) R ppid pgrp sess tty tpgid flags minflt cminflt majflt cmajflt utime stime ...
        let raw = "1234 (com.ex ample) R 1 1 1 0 -1 4194304 100 0 0 0 50 30 0 0 20 0 1 0 12345 0 0 0 0";
        let ticks = parse_proc_stat_ticks(raw).unwrap();
        assert_eq!(ticks, 80);
    }
}
