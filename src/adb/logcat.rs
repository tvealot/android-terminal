use std::io::{BufRead, BufReader};
use std::process::{Child, Stdio};
use std::sync::mpsc::Sender;
use std::thread;

use crate::adb::{command, DeviceHandle};
use crate::dispatch::Event;
use crate::logcat::LogLine;

pub fn spawn(handle: &DeviceHandle, tx: Sender<Event>) -> std::io::Result<Child> {
    let mut child = command(handle)
        .args(["logcat", "-v", "threadtime"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let stdout = child.stdout.take().expect("piped stdout");
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if let Some(parsed) = LogLine::parse(&line) {
                if tx.send(Event::Logcat(parsed)).is_err() {
                    break;
                }
            }
        }
    });
    Ok(child)
}
