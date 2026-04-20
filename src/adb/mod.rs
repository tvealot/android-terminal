pub mod devices;
pub mod logcat;

use std::process::Command;

pub fn is_available() -> bool {
    Command::new("adb")
        .arg("version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
