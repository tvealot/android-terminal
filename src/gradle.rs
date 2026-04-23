use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use serde::Deserialize;

use crate::dispatch::Event;

pub fn jvm_available() -> bool {
    Command::new("java")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum GradleEvent {
    TaskStart {
        ts: String,
        path: String,
    },
    TaskFinish {
        ts: String,
        path: String,
        outcome: String,
        duration_ms: u64,
    },
    BuildFinish {
        ts: String,
        outcome: String,
    },
    Error {
        ts: String,
        message: String,
    },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ActiveTask {
    pub path: String,
    pub started_at: String,
}

#[derive(Debug, Clone)]
pub struct HostGradleProc {
    pub pid: u32,
    pub cpu: f32,
    pub rss_kb: u64,
    pub kind: &'static str,
}

#[derive(Default)]
pub struct GradleState {
    pub running: bool,
    pub active: HashMap<String, ActiveTask>,
    pub last_error: Option<String>,
    pub last_outcome: Option<String>,
    pub host_procs: Vec<HostGradleProc>,
    pub selected: usize,
}

impl GradleState {
    pub fn clamp_selected(&mut self) {
        let n = self.host_procs.len();
        if n == 0 {
            self.selected = 0;
        } else if self.selected >= n {
            self.selected = n - 1;
        }
    }

    pub fn selected_pid(&self) -> Option<u32> {
        self.host_procs.get(self.selected).map(|p| p.pid)
    }
}

impl GradleState {
    pub fn apply(&mut self, ev: GradleEvent) {
        match ev {
            GradleEvent::TaskStart { path, ts } => {
                self.active.insert(
                    path.clone(),
                    ActiveTask {
                        path,
                        started_at: ts,
                    },
                );
            }
            GradleEvent::TaskFinish { path, .. } => {
                self.active.remove(&path);
            }
            GradleEvent::BuildFinish { outcome, .. } => {
                self.running = false;
                self.active.clear();
                self.last_outcome = Some(outcome);
            }
            GradleEvent::Error { message, .. } => {
                self.running = false;
                self.last_error = Some(message);
            }
        }
    }
}

pub fn spawn(jar: &Path, project_dir: &Path, task: &str, tx: Sender<Event>) -> std::io::Result<()> {
    let mut child = Command::new("java")
        .arg("-jar")
        .arg(jar)
        .arg("--project")
        .arg(project_dir)
        .arg("--task")
        .arg(task)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let err_tx = tx.clone();

    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let _ = err_tx.send(Event::Gradle(GradleEvent::Error {
                ts: chrono::Local::now().to_rfc3339(),
                message: line,
            }));
        }
    });

    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            match serde_json::from_str::<GradleEvent>(&line) {
                Ok(ev) => {
                    if tx.send(Event::Gradle(ev)).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    let _ = tx.send(Event::Status {
                        text: format!("gradle: unparseable line: {}", err),
                        error: true,
                    });
                }
            }
        }
        let _ = child.wait();
    });
    Ok(())
}

pub fn kill_host(pid: u32) -> Result<(), String> {
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("kill exited with {}", status))
    }
}

pub fn spawn_host_poller(tx: Sender<Event>) {
    thread::spawn(move || loop {
        let procs = scan_host_gradle();
        if tx.send(Event::HostGradle(procs)).is_err() {
            break;
        }
        thread::sleep(Duration::from_secs(2));
    });
}

fn scan_host_gradle() -> Vec<HostGradleProc> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,pcpu=,rss=,command="])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        let mut it = trimmed.split_whitespace();
        let Some(pid_s) = it.next() else { continue };
        let Some(cpu_s) = it.next() else { continue };
        let Some(rss_s) = it.next() else { continue };
        let cmd_tokens: Vec<&str> = it.collect();
        if cmd_tokens.is_empty() {
            continue;
        }
        let cmd = cmd_tokens.join(" ");
        let kind = classify_gradle(&cmd);
        if kind.is_empty() {
            continue;
        }
        let Ok(pid) = pid_s.parse::<u32>() else { continue };
        let cpu = cpu_s.parse::<f32>().unwrap_or(0.0);
        let rss_kb = rss_s.parse::<u64>().unwrap_or(0);
        out.push(HostGradleProc {
            pid,
            cpu,
            rss_kb,
            kind,
        });
    }
    out
}

fn classify_gradle(cmd: &str) -> &'static str {
    if cmd.contains("GradleDaemon") || cmd.contains("org.gradle.launcher.daemon") {
        "daemon"
    } else if cmd.contains("org.gradle.launcher.GradleMain")
        || cmd.contains("org.gradle.launcher.Main")
    {
        "launcher"
    } else if cmd.contains("/gradlew ")
        || cmd.ends_with("/gradlew")
        || cmd.contains("gradle-wrapper.jar")
    {
        "wrapper"
    } else if cmd.contains("gradle-agent.jar") || cmd.contains("sh.droidscope.agent") {
        "agent"
    } else if cmd.contains("KotlinCompileDaemon")
        || cmd.contains("org.jetbrains.kotlin.daemon")
    {
        "kotlin"
    } else if cmd.contains("com.android.build") || cmd.contains("aapt2") {
        "android"
    } else {
        ""
    }
}

pub fn default_jar_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("droidscope")
        .join("gradle-agent.jar")
}
