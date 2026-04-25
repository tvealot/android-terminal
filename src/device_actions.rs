use std::fs;
use std::path::PathBuf;
use std::process::Output;
use std::sync::mpsc::Sender;
use std::thread;

use chrono::Local;

use crate::adb::{self, DeviceHandle};
use crate::dispatch::Event;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceAction {
    Screenshot,
    ScreenRecord,
    RotateRight,
    DarkModeOn,
    DarkModeOff,
    Locale,
    FontScale,
    BatteryUnplug,
    BatteryPlug,
    AirplaneOn,
    AirplaneOff,
    WifiOn,
    WifiOff,
    DataOn,
    DataOff,
    InputText,
    Tap,
}

impl DeviceAction {
    pub fn label(self) -> &'static str {
        match self {
            DeviceAction::Screenshot => "screenshot",
            DeviceAction::ScreenRecord => "screenrecord",
            DeviceAction::RotateRight => "rotate right",
            DeviceAction::DarkModeOn => "dark mode on",
            DeviceAction::DarkModeOff => "dark mode off",
            DeviceAction::Locale => "set locale",
            DeviceAction::FontScale => "font scale",
            DeviceAction::BatteryUnplug => "battery unplug",
            DeviceAction::BatteryPlug => "battery plug",
            DeviceAction::AirplaneOn => "airplane on",
            DeviceAction::AirplaneOff => "airplane off",
            DeviceAction::WifiOn => "wifi on",
            DeviceAction::WifiOff => "wifi off",
            DeviceAction::DataOn => "data on",
            DeviceAction::DataOff => "data off",
            DeviceAction::InputText => "input text",
            DeviceAction::Tap => "tap",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            DeviceAction::Screenshot => "Save current screen PNG in the current directory",
            DeviceAction::ScreenRecord => "Record 10 seconds to MP4 in the current directory",
            DeviceAction::RotateRight => "Disable auto-rotate and advance user_rotation",
            DeviceAction::DarkModeOn => "cmd uimode night yes",
            DeviceAction::DarkModeOff => "cmd uimode night no",
            DeviceAction::Locale => "Set persist.sys.locale and restart zygote",
            DeviceAction::FontScale => "settings put system font_scale",
            DeviceAction::BatteryUnplug => "dumpsys battery unplug",
            DeviceAction::BatteryPlug => "dumpsys battery reset",
            DeviceAction::AirplaneOn => "Enable airplane mode",
            DeviceAction::AirplaneOff => "Disable airplane mode",
            DeviceAction::WifiOn => "svc wifi enable",
            DeviceAction::WifiOff => "svc wifi disable",
            DeviceAction::DataOn => "svc data enable",
            DeviceAction::DataOff => "svc data disable",
            DeviceAction::InputText => "adb shell input text",
            DeviceAction::Tap => "adb shell input tap x y",
        }
    }

    pub fn needs_input(self) -> bool {
        matches!(
            self,
            DeviceAction::Locale
                | DeviceAction::FontScale
                | DeviceAction::InputText
                | DeviceAction::Tap
        )
    }
}

pub const ACTIONS: &[DeviceAction] = &[
    DeviceAction::Screenshot,
    DeviceAction::ScreenRecord,
    DeviceAction::RotateRight,
    DeviceAction::DarkModeOn,
    DeviceAction::DarkModeOff,
    DeviceAction::Locale,
    DeviceAction::FontScale,
    DeviceAction::BatteryUnplug,
    DeviceAction::BatteryPlug,
    DeviceAction::AirplaneOn,
    DeviceAction::AirplaneOff,
    DeviceAction::WifiOn,
    DeviceAction::WifiOff,
    DeviceAction::DataOn,
    DeviceAction::DataOff,
    DeviceAction::InputText,
    DeviceAction::Tap,
];

#[derive(Debug, Clone)]
pub struct DeviceActionResult {
    pub action: DeviceAction,
    pub success: bool,
    pub summary: String,
    pub output: String,
}

#[derive(Default)]
pub struct DeviceActionsState {
    pub selected: usize,
    pub running: bool,
    pub last: Option<DeviceActionResult>,
    pub input: String,
}

impl DeviceActionsState {
    pub fn move_down(&mut self) {
        if !ACTIONS.is_empty() {
            self.selected = (self.selected + 1).min(ACTIONS.len() - 1);
        }
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn selected_action(&self) -> DeviceAction {
        ACTIONS
            .get(self.selected)
            .copied()
            .unwrap_or(DeviceAction::Screenshot)
    }
}

pub fn spawn_action(
    handle: DeviceHandle,
    action: DeviceAction,
    input: Option<String>,
    tx: Sender<Event>,
) {
    thread::spawn(move || {
        let result = run_action(&handle, action, input);
        let _ = tx.send(Event::DeviceAction(result));
    });
}

fn run_action(
    handle: &DeviceHandle,
    action: DeviceAction,
    input: Option<String>,
) -> DeviceActionResult {
    let result = match action {
        DeviceAction::Screenshot => screenshot(handle),
        DeviceAction::ScreenRecord => screenrecord(handle),
        DeviceAction::RotateRight => rotate_right(handle),
        DeviceAction::DarkModeOn => run_shell(handle, action, &["cmd", "uimode", "night", "yes"]),
        DeviceAction::DarkModeOff => run_shell(handle, action, &["cmd", "uimode", "night", "no"]),
        DeviceAction::Locale => set_locale(handle, input.as_deref().unwrap_or_default()),
        DeviceAction::FontScale => set_font_scale(handle, input.as_deref().unwrap_or_default()),
        DeviceAction::BatteryUnplug => run_shell(handle, action, &["dumpsys", "battery", "unplug"]),
        DeviceAction::BatteryPlug => run_shell(handle, action, &["dumpsys", "battery", "reset"]),
        DeviceAction::AirplaneOn => set_airplane(handle, true),
        DeviceAction::AirplaneOff => set_airplane(handle, false),
        DeviceAction::WifiOn => run_shell(handle, action, &["svc", "wifi", "enable"]),
        DeviceAction::WifiOff => run_shell(handle, action, &["svc", "wifi", "disable"]),
        DeviceAction::DataOn => run_shell(handle, action, &["svc", "data", "enable"]),
        DeviceAction::DataOff => run_shell(handle, action, &["svc", "data", "disable"]),
        DeviceAction::InputText => input_text(handle, input.as_deref().unwrap_or_default()),
        DeviceAction::Tap => tap(handle, input.as_deref().unwrap_or_default()),
    };

    match result {
        Ok((summary, output)) => DeviceActionResult {
            action,
            success: true,
            summary,
            output,
        },
        Err(message) => DeviceActionResult {
            action,
            success: false,
            summary: format!("{} failed: {}", action.label(), message),
            output: message,
        },
    }
}

fn screenshot(handle: &DeviceHandle) -> Result<(String, String), String> {
    let path = artifact_path("screenshot", "png")?;
    let output = adb::command(handle)
        .args(["exec-out", "screencap", "-p"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(error_text(&output));
    }
    fs::write(&path, output.stdout).map_err(|e| e.to_string())?;
    Ok((
        format!("screenshot saved: {}", path.display()),
        path.display().to_string(),
    ))
}

fn screenrecord(handle: &DeviceHandle) -> Result<(String, String), String> {
    let local = artifact_path("screenrecord", "mp4")?;
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let remote = format!("/sdcard/droidscope-screenrecord-{stamp}.mp4");
    let record = adb::command(handle)
        .args(["shell", "screenrecord", "--time-limit", "10", &remote])
        .output()
        .map_err(|e| e.to_string())?;
    if !record.status.success() {
        return Err(error_text(&record));
    }
    let pull = adb::command(handle)
        .args(["pull", &remote, path_str(&local)?])
        .output()
        .map_err(|e| e.to_string())?;
    let _ = adb::command(handle)
        .args(["shell", "rm", "-f", &remote])
        .output();
    if !pull.status.success() {
        return Err(error_text(&pull));
    }
    Ok((
        format!("screenrecord saved: {}", local.display()),
        output_text(&pull),
    ))
}

fn rotate_right(handle: &DeviceHandle) -> Result<(String, String), String> {
    let current = adb::command(handle)
        .args(["shell", "settings", "get", "system", "user_rotation"])
        .output()
        .map_err(|e| e.to_string())?;
    let raw = String::from_utf8_lossy(&current.stdout);
    let rotation = raw.trim().parse::<u8>().unwrap_or(0);
    let next = (rotation + 1) % 4;
    run_shell(
        handle,
        DeviceAction::RotateRight,
        &["settings", "put", "system", "accelerometer_rotation", "0"],
    )?;
    let next_value = next.to_string();
    run_shell(
        handle,
        DeviceAction::RotateRight,
        &["settings", "put", "system", "user_rotation", &next_value],
    )?;
    Ok((format!("rotation set to {}", next), String::new()))
}

fn set_locale(handle: &DeviceHandle, locale: &str) -> Result<(String, String), String> {
    let locale = locale.trim();
    if locale.is_empty() {
        return Err("locale is empty".to_string());
    }
    if !locale
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err("locale may contain only letters, digits, dash, underscore".to_string());
    }
    run_shell(
        handle,
        DeviceAction::Locale,
        &["setprop", "persist.sys.locale", locale],
    )?;
    let restart = run_shell(
        handle,
        DeviceAction::Locale,
        &["setprop", "ctl.restart", "zygote"],
    );
    match restart {
        Ok((_, output)) => Ok((format!("locale set: {}", locale), output)),
        Err(e) => Ok((
            format!("locale set: {} (restart failed)", locale),
            format!("restart zygote: {e}"),
        )),
    }
}

fn set_font_scale(handle: &DeviceHandle, value: &str) -> Result<(String, String), String> {
    let value = value.trim();
    let parsed = value
        .parse::<f32>()
        .map_err(|_| "font scale must be a number, for example 1.15".to_string())?;
    if !(0.5..=2.0).contains(&parsed) {
        return Err("font scale must be between 0.5 and 2.0".to_string());
    }
    run_shell(
        handle,
        DeviceAction::FontScale,
        &["settings", "put", "system", "font_scale", value],
    )?;
    Ok((format!("font scale set: {}", value), String::new()))
}

fn set_airplane(handle: &DeviceHandle, enabled: bool) -> Result<(String, String), String> {
    let mode = if enabled { "enable" } else { "disable" };
    let action = if enabled {
        DeviceAction::AirplaneOn
    } else {
        DeviceAction::AirplaneOff
    };
    match run_shell(
        handle,
        action,
        &["cmd", "connectivity", "airplane-mode", mode],
    ) {
        Ok(result) => Ok(result),
        Err(first) => {
            let value = if enabled { "1" } else { "0" };
            let state = if enabled { "true" } else { "false" };
            run_shell(
                handle,
                action,
                &["settings", "put", "global", "airplane_mode_on", value],
            )?;
            let broadcast = run_shell(
                handle,
                action,
                &[
                    "am",
                    "broadcast",
                    "-a",
                    "android.intent.action.AIRPLANE_MODE",
                    "--ez",
                    "state",
                    state,
                ],
            )?;
            Ok((
                format!(
                    "airplane mode {}",
                    if enabled { "enabled" } else { "disabled" }
                ),
                format!("cmd connectivity failed: {first}\n{}", broadcast.1),
            ))
        }
    }
}

fn input_text(handle: &DeviceHandle, text: &str) -> Result<(String, String), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("text is empty".to_string());
    }
    let encoded = encode_input_text(text)?;
    run_shell(
        handle,
        DeviceAction::InputText,
        &["input", "text", &encoded],
    )?;
    Ok((
        format!("input text sent ({} chars)", text.chars().count()),
        encoded,
    ))
}

fn tap(handle: &DeviceHandle, value: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() != 2 {
        return Err("tap expects: x y".to_string());
    }
    let x = parse_coord(parts[0])?;
    let y = parse_coord(parts[1])?;
    run_shell(
        handle,
        DeviceAction::Tap,
        &["input", "tap", &x.to_string(), &y.to_string()],
    )?;
    Ok((format!("tap sent: {},{}", x, y), String::new()))
}

fn run_shell(
    handle: &DeviceHandle,
    action: DeviceAction,
    args: &[&str],
) -> Result<(String, String), String> {
    let output = adb::command(handle)
        .arg("shell")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        let text = output_text(&output);
        let summary = text
            .lines()
            .next()
            .filter(|line| !line.trim().is_empty())
            .map(|line| format!("{}: {}", action.label(), line.trim()))
            .unwrap_or_else(|| format!("{} done", action.label()));
        Ok((summary, text))
    } else {
        Err(error_text(&output))
    }
}

fn artifact_path(kind: &str, ext: &str) -> Result<PathBuf, String> {
    let dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    Ok(dir.join(format!("droidscope-{kind}-{stamp}.{ext}")))
}

fn output_text(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else if stdout.trim().is_empty() {
        stderr.trim().to_string()
    } else {
        format!("{}\n{}", stdout.trim(), stderr.trim())
    };
    text
}

fn error_text(output: &Output) -> String {
    let text = output_text(output);
    if text.is_empty() {
        format!("exit status {}", output.status)
    } else {
        text
    }
}

fn path_str(path: &PathBuf) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| "artifact path is not valid UTF-8".to_string())
}

fn encode_input_text(text: &str) -> Result<String, String> {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            ' ' => out.push_str("%s"),
            '%' => out.push_str("%25"),
            'A'..='Z' | 'a'..='z' | '0'..='9' | '.' | ',' | '_' | '-' | '@' | ':' | '/' => {
                out.push(c)
            }
            _ => return Err("text supports letters, digits, spaces, and .,_-@:/%".to_string()),
        }
    }
    Ok(out)
}

fn parse_coord(value: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .map_err(|_| "tap coordinates must be positive integers".to_string())
}
