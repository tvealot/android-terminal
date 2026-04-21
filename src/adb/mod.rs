pub mod devices;
pub mod logcat;

use std::process::Command;
use std::sync::{Arc, Mutex};

/// Shared selected-device serial. `None` = let adb pick the default (single device).
pub type DeviceHandle = Arc<Mutex<Option<String>>>;

pub fn new_handle() -> DeviceHandle {
    Arc::new(Mutex::new(None))
}

pub fn serial_of(handle: &DeviceHandle) -> Option<String> {
    handle.lock().ok().and_then(|g| g.clone())
}

/// Returns a `Command` pre-targeted at the selected device (via `-s`) if one is set.
pub fn command(handle: &DeviceHandle) -> Command {
    let mut cmd = Command::new("adb");
    if let Some(serial) = serial_of(handle) {
        cmd.arg("-s").arg(serial);
    }
    cmd
}

pub fn is_available() -> bool {
    Command::new("adb")
        .arg("version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
