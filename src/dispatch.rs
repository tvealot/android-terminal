use std::sync::mpsc::{channel, Receiver, Sender};

use crate::gradle::GradleEvent;
use crate::logcat::LogLine;

pub enum Event {
    Logcat(LogLine),
    Gradle(GradleEvent),
    Devices(Vec<String>),
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
