use std::sync::mpsc::{channel, Receiver, Sender};

use crate::adb::devices::DeviceEntry;
use crate::gradle::{GradleEvent, HostGradleProc};
use crate::logcat::LogLine;
use crate::monitor::MonitorSample;
use crate::processes::ProcessInfo;
use crate::project_picker::ProjectEntry;

pub enum Event {
    Logcat(LogLine),
    Gradle(GradleEvent),
    HostGradle(Vec<HostGradleProc>),
    Monitor(MonitorSample),
    Processes(Vec<ProcessInfo>),
    Devices(Vec<DeviceEntry>),
    Projects(Vec<ProjectEntry>),
    Emulators(Vec<String>),
    Fps(crate::fps::FpsSample),
    AppControl(crate::app_control::AppActionResult),
    AppData(crate::app_data::AppDataEvent),
    Intent(crate::intents::IntentResult),
    Status { text: String, error: bool },
}

pub struct DispatchContext {
    pub tx: Sender<Event>,
    rx: Receiver<Event>,
}

impl DispatchContext {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        Self { tx, rx }
    }

    pub fn drain(&self) -> Vec<Event> {
        let mut out = Vec::new();
        while let Ok(ev) = self.rx.try_recv() {
            out.push(ev);
        }
        out
    }
}
