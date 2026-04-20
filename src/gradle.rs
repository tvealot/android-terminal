use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;

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
pub struct CompletedTask {
    pub path: String,
    pub outcome: String,
    pub duration_ms: u64,
}

#[derive(Default)]
pub struct GradleState {
    pub running: bool,
    pub active: HashMap<String, ActiveTask>,
    pub completed: Vec<CompletedTask>,
    pub last_error: Option<String>,
    pub last_outcome: Option<String>,
}

impl GradleState {
    pub fn apply(&mut self, ev: GradleEvent) {
        match ev {
            GradleEvent::TaskStart { path, ts } => {
                self.active.insert(path.clone(), ActiveTask { path, started_at: ts });
            }
            GradleEvent::TaskFinish { path, outcome, duration_ms, .. } => {
                self.active.remove(&path);
                self.completed.push(CompletedTask { path, outcome, duration_ms });
                if self.completed.len() > 200 {
                    let excess = self.completed.len() - 200;
                    self.completed.drain(..excess);
                }
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

pub fn spawn(
    jar: &Path,
    project_dir: &Path,
    task: &str,
    tx: Sender<Event>,
) -> std::io::Result<()> {
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

pub fn default_jar_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("droidscope")
        .join("gradle-agent.jar")
}
