use std::path::PathBuf;

use color_eyre::eyre::{bail, Result};
use serde_json::{json, Value};

use crate::panel::{Feature, PANELS};
use crate::{
    adb, app_control, config, device_actions, device_tools, emulator_picker, files, fps, gradle,
    issues, logcat, manifest, monitor, perf, processes, project_picker,
};

const DEFAULT_LIMIT: usize = 50;
const NETWORK_KEYWORDS: &[&str] = &[
    "okhttp",
    "retrofit",
    "http",
    "https",
    "socket",
    "websocket",
    "grpc",
    "apollo",
    "dns",
    "ssl",
    "tls",
    "request",
    "response",
];

const SECTIONS: &[(&str, &str)] = &[
    (
        "actions",
        "available device action commands; does not run them",
    ),
    ("app", "configured target package and available app actions"),
    ("config", "loaded config paths and values"),
    ("daemons", "Gradle/Kotlin/Android host processes"),
    (
        "data",
        "configured app data modes; live browsing is interactive",
    ),
    (
        "devices",
        "adb devices with model, Android version, and battery",
    ),
    ("emulators", "available Android Virtual Devices"),
    ("files", "configured project file tree root"),
    (
        "fps",
        "one focused-app frame pacing sample when a package is set",
    ),
    ("gradle", "Gradle config plus host daemons"),
    ("intents", "deep-link runner info; launching is interactive"),
    (
        "issues",
        "crash/ANR/native issues detected from logcat dump",
    ),
    ("logcat", "current adb logcat dump"),
    (
        "manifest",
        "installed manifest report for configured or supplied package",
    ),
    ("monitor", "one device battery/memory sample"),
    ("network", "network-like lines from logcat dump"),
    (
        "packages",
        "Android package ids discovered from workspaces/projects",
    ),
    (
        "panels",
        "TUI panel registry and persisted visibility state",
    ),
    (
        "perf",
        "one app memory/CPU/gfx sample when a package is set",
    ),
    ("processes", "device process list sorted by RSS"),
    (
        "projects",
        "Android projects discovered under Documents or --root",
    ),
    ("shell", "embedded adb shell status; interactive only"),
    ("workspaces", "saved workspace profiles"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Plain,
    Json,
}

#[derive(Debug, Clone)]
struct SectionOptions {
    format: OutputFormat,
    limit: usize,
    device: Option<String>,
    root: Option<PathBuf>,
    package: Option<String>,
}

impl Default for SectionOptions {
    fn default() -> Self {
        Self {
            format: OutputFormat::Plain,
            limit: DEFAULT_LIMIT,
            device: None,
            root: None,
            package: None,
        }
    }
}

pub fn handle_args() -> Result<bool> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return Ok(false);
    }

    let command = args.remove(0);
    match command.as_str() {
        "-h" | "--help" | "help" => {
            print_help();
            Ok(true)
        }
        "tui" | "ui" => Ok(false),
        "sections" => {
            let opts = parse_options(&args)?;
            print_sections(&opts)?;
            Ok(true)
        }
        "section" => {
            let Some(section) = args.first() else {
                bail!("missing section name; try `droidscope sections`");
            };
            print_section(section, &args[1..])?;
            Ok(true)
        }
        other => {
            if canonical_section(other).is_some() {
                print_section(other, &args)?;
                Ok(true)
            } else {
                bail!("unknown command `{other}`; try `droidscope help`");
            }
        }
    }
}

fn print_help() {
    let program = std::env::args()
        .next()
        .unwrap_or_else(|| "droidscope".to_string());
    println!("{program} [command]");
    println!();
    println!("Without a command, starts the interactive TUI.");
    println!();
    println!("Commands:");
    println!("  tui                         start the interactive TUI explicitly");
    println!("  sections [--json]           list available non-interactive sections");
    println!("  section <name> [options]    print one named section");
    println!("  <section> [options]         shortcut for `section <section>`");
    println!("  help                        show this help");
    println!();
    println!("Common options:");
    println!("  --json                      emit JSON");
    println!("  --limit <n>                 cap list output (default {DEFAULT_LIMIT})");
    println!("  --device <serial>           target a specific adb device");
    println!("  --root <path>               scan/read a specific local root");
    println!("  --package <id>              target a specific Android package");
    println!();
    println!("Examples:");
    println!("  {program} daemons --json");
    println!("  {program} devices");
    println!("  {program} processes --device emulator-5554 --limit 20");
    println!("  {program} packages --root ~/Documents/git");
}

fn print_sections(opts: &SectionOptions) -> Result<()> {
    match opts.format {
        OutputFormat::Plain => {
            println!("available sections:");
            for (name, desc) in SECTIONS {
                println!("  {name:<10} {desc}");
            }
        }
        OutputFormat::Json => {
            let sections: Vec<_> = SECTIONS
                .iter()
                .map(|(name, desc)| json!({ "name": name, "description": desc }))
                .collect();
            print_json(json!({
                "section": "sections",
                "items": sections,
            }))?;
        }
    }
    Ok(())
}

fn print_section(section: &str, args: &[String]) -> Result<()> {
    let section = canonical_section(section)
        .ok_or_else(|| color_eyre::eyre::eyre!("unknown section `{section}`"))?;
    let opts = parse_options(args)?;
    match section {
        "actions" => print_actions(&opts)?,
        "app" => print_app(&opts)?,
        "config" => print_config(&opts)?,
        "daemons" => print_daemons(&opts)?,
        "data" => print_data(&opts)?,
        "devices" => print_devices(&opts)?,
        "emulators" => print_emulators(&opts)?,
        "files" => print_files(&opts)?,
        "fps" => print_fps(&opts)?,
        "gradle" => print_gradle(&opts)?,
        "intents" => print_intents(&opts)?,
        "issues" => print_issues(&opts)?,
        "logcat" => print_logcat(&opts)?,
        "manifest" => print_manifest(&opts)?,
        "monitor" => print_monitor(&opts)?,
        "network" => print_network(&opts)?,
        "packages" => print_packages(&opts)?,
        "panels" => print_panels(&opts)?,
        "perf" => print_perf(&opts)?,
        "processes" => print_processes(&opts)?,
        "projects" => print_projects(&opts)?,
        "shell" => print_shell(&opts)?,
        "workspaces" => print_workspaces(&opts)?,
        _ => unreachable!("canonical section is listed but not handled"),
    }
    Ok(())
}

fn canonical_section(input: &str) -> Option<&str> {
    match input {
        "deamons" => Some("daemons"),
        "actions" | "app" | "config" | "daemons" | "data" | "devices" | "emulators" | "files"
        | "fps" | "gradle" | "intents" | "issues" | "logcat" | "manifest" | "monitor"
        | "network" | "packages" | "panels" | "perf" | "processes" | "projects" | "shell"
        | "workspaces" => Some(input),
        _ => None,
    }
}

fn print_daemons(opts: &SectionOptions) -> Result<()> {
    let procs = limited(gradle::scan_host_gradle(), opts.limit);
    match opts.format {
        OutputFormat::Plain => {
            println!("Gradle/Kotlin/Android host processes");
            if procs.is_empty() {
                println!("no matching processes");
                return Ok(());
            }
            println!(
                "{:<10} {:>7} {:>8} {:>8}  command",
                "kind", "pid", "cpu%", "rss_mb"
            );
            for proc in procs {
                println!(
                    "{:<10} {:>7} {:>8.1} {:>8.0}  {}",
                    proc.kind,
                    proc.pid,
                    proc.cpu,
                    proc.rss_kb as f64 / 1024.0,
                    proc.command
                );
            }
        }
        OutputFormat::Json => {
            let items: Vec<_> = procs
                .into_iter()
                .map(|proc| {
                    json!({
                        "kind": proc.kind,
                        "pid": proc.pid,
                        "cpu": proc.cpu,
                        "rss_kb": proc.rss_kb,
                        "rss_mb": proc.rss_kb as f64 / 1024.0,
                        "command": proc.command,
                    })
                })
                .collect();
            print_json(json!({ "section": "daemons", "items": items }))?;
        }
    }
    Ok(())
}

fn print_devices(opts: &SectionOptions) -> Result<()> {
    let devices = limited(adb::devices::list_all(), opts.limit);
    match opts.format {
        OutputFormat::Plain => {
            println!("Android devices");
            if devices.is_empty() {
                println!("no devices");
                return Ok(());
            }
            println!(
                "{:<24} {:<10} {:<24} {:<9} {:<5} {:>7}",
                "serial", "state", "model", "android", "sdk", "battery"
            );
            for d in devices {
                println!(
                    "{:<24} {:<10} {:<24} {:<9} {:<5} {:>6}",
                    d.serial,
                    d.state,
                    d.model.as_deref().unwrap_or("-"),
                    d.release.as_deref().unwrap_or("-"),
                    d.sdk.as_deref().unwrap_or("-"),
                    d.battery
                        .map(|v| format!("{v}%"))
                        .unwrap_or_else(|| "-".to_string())
                );
            }
        }
        OutputFormat::Json => {
            let items: Vec<_> = devices
                .into_iter()
                .map(|d| {
                    json!({
                        "serial": d.serial,
                        "state": d.state,
                        "model": d.model,
                        "release": d.release,
                        "sdk": d.sdk,
                        "battery": d.battery,
                    })
                })
                .collect();
            print_json(json!({ "section": "devices", "items": items }))?;
        }
    }
    Ok(())
}

fn print_monitor(opts: &SectionOptions) -> Result<()> {
    let handle = device_handle(&opts.device);
    let sample = monitor::sample(&handle).map_err(|e| color_eyre::eyre::eyre!("monitor: {e}"))?;
    match opts.format {
        OutputFormat::Plain => {
            println!("Device monitor");
            println!("battery: {}%", sample.battery_percent);
            println!("battery_temp_c: {:.1}", sample.battery_temp_c);
            println!("mem_total_mb: {:.0}", sample.mem_total_kb as f64 / 1024.0);
            println!(
                "mem_available_mb: {:.0}",
                sample.mem_available_kb as f64 / 1024.0
            );
            println!("mem_used_percent: {:.1}", sample.mem_used_percent());
        }
        OutputFormat::Json => {
            print_json(json!({
                "section": "monitor",
                "device": opts.device,
                "battery_percent": sample.battery_percent,
                "battery_temp_c": sample.battery_temp_c,
                "mem_total_kb": sample.mem_total_kb,
                "mem_available_kb": sample.mem_available_kb,
                "mem_used_kb": sample.mem_used_kb(),
                "mem_used_percent": sample.mem_used_percent(),
            }))?;
        }
    }
    Ok(())
}

fn print_processes(opts: &SectionOptions) -> Result<()> {
    let handle = device_handle(&opts.device);
    let procs = limited(
        processes::sample(&handle).map_err(|e| color_eyre::eyre::eyre!("processes: {e}"))?,
        opts.limit,
    );
    match opts.format {
        OutputFormat::Plain => {
            println!("Device processes");
            if procs.is_empty() {
                println!("no processes");
                return Ok(());
            }
            println!("{:>7} {:<12} {:>8}  name", "pid", "user", "rss_mb");
            for p in procs {
                println!(
                    "{:>7} {:<12} {:>8.0}  {}",
                    p.pid,
                    p.user,
                    p.rss_kb as f64 / 1024.0,
                    p.name
                );
            }
        }
        OutputFormat::Json => {
            let items: Vec<_> = procs
                .into_iter()
                .map(|p| {
                    json!({
                        "pid": p.pid,
                        "user": p.user,
                        "rss_kb": p.rss_kb,
                        "rss_mb": p.rss_kb as f64 / 1024.0,
                        "name": p.name,
                    })
                })
                .collect();
            print_json(json!({ "section": "processes", "device": opts.device, "items": items }))?;
        }
    }
    Ok(())
}

fn print_logcat(opts: &SectionOptions) -> Result<()> {
    let lines = limited(read_logcat(opts)?, opts.limit);
    match opts.format {
        OutputFormat::Plain => {
            println!("Logcat dump");
            if lines.is_empty() {
                println!("no parsed logcat lines");
                return Ok(());
            }
            for line in lines {
                println!(
                    "{} {:>5} {} {:<20} {}",
                    line.timestamp,
                    line.pid,
                    line.level.short(),
                    line.tag,
                    line.message
                );
            }
        }
        OutputFormat::Json => {
            let items = log_lines_json(lines);
            print_json(json!({ "section": "logcat", "device": opts.device, "items": items }))?;
        }
    }
    Ok(())
}

fn print_network(opts: &SectionOptions) -> Result<()> {
    let lines: Vec<_> = read_logcat(opts)?
        .into_iter()
        .filter(is_network_line)
        .collect();
    let lines = limited(lines, opts.limit);
    match opts.format {
        OutputFormat::Plain => {
            println!("Network-like logcat lines");
            if lines.is_empty() {
                println!("no matching lines");
                return Ok(());
            }
            for line in lines {
                println!("{} {:<20} {}", line.timestamp, line.tag, line.message);
            }
        }
        OutputFormat::Json => {
            let items = log_lines_json(lines);
            print_json(json!({ "section": "network", "device": opts.device, "items": items }))?;
        }
    }
    Ok(())
}

fn print_issues(opts: &SectionOptions) -> Result<()> {
    let mut state = issues::IssuesState::default();
    for line in read_logcat(opts)? {
        state.detect(&line);
    }
    let items: Vec<_> = limited(state.issues.into_iter().collect::<Vec<_>>(), opts.limit);
    match opts.format {
        OutputFormat::Plain => {
            println!("Issues from logcat dump");
            if items.is_empty() {
                println!("no crash/ANR/native issues detected");
                return Ok(());
            }
            for issue in items {
                println!(
                    "{} {:<6} pid={:<6} {:<18} {}",
                    issue.timestamp,
                    issue.kind.label(),
                    issue.pid,
                    issue.tag,
                    issue.excerpt
                );
            }
        }
        OutputFormat::Json => {
            let values: Vec<_> = items
                .into_iter()
                .map(|issue| {
                    json!({
                        "kind": issue.kind.label(),
                        "timestamp": issue.timestamp,
                        "pid": issue.pid,
                        "tag": issue.tag,
                        "excerpt": issue.excerpt,
                        "count": issue.count,
                        "buffer": issue.buffer,
                    })
                })
                .collect();
            print_json(json!({ "section": "issues", "device": opts.device, "items": values }))?;
        }
    }
    Ok(())
}

fn print_projects(opts: &SectionOptions) -> Result<()> {
    let root = opts
        .root
        .clone()
        .unwrap_or_else(project_picker::default_root);
    let projects = limited(project_picker::scan(root.clone()), opts.limit);
    match opts.format {
        OutputFormat::Plain => {
            println!("Android projects under {}", root.display());
            if projects.is_empty() {
                println!("no projects found");
                return Ok(());
            }
            for project in projects {
                println!("{}  {}", project.modified_label(), project.display);
            }
        }
        OutputFormat::Json => {
            let items: Vec<_> = projects
                .into_iter()
                .map(|p| {
                    json!({
                        "path": p.path,
                        "display": p.display,
                        "modified": p.modified_label(),
                    })
                })
                .collect();
            print_json(json!({ "section": "projects", "root": root, "items": items }))?;
        }
    }
    Ok(())
}

fn print_packages(opts: &SectionOptions) -> Result<()> {
    let roots = package_roots(opts);
    let packages = limited(
        device_tools::scan_packages(roots.clone(), seed_packages()),
        opts.limit,
    );
    match opts.format {
        OutputFormat::Plain => {
            println!("Discovered Android packages");
            if packages.is_empty() {
                println!("no packages found");
                return Ok(());
            }
            for pkg in packages {
                println!(
                    "{:<45} {:<22} {}",
                    pkg.package,
                    pkg.project_name,
                    pkg.path_label()
                );
            }
        }
        OutputFormat::Json => {
            let items: Vec<_> = packages
                .into_iter()
                .map(|p| {
                    json!({
                        "package": p.package,
                        "project_dir": p.project_dir,
                        "project_name": p.project_name,
                        "source": p.source,
                    })
                })
                .collect();
            print_json(json!({ "section": "packages", "roots": roots, "items": items }))?;
        }
    }
    Ok(())
}

fn print_emulators(opts: &SectionOptions) -> Result<()> {
    let avds = limited(
        emulator_picker::list_avds().map_err(|e| color_eyre::eyre::eyre!("emulators: {e}"))?,
        opts.limit,
    );
    match opts.format {
        OutputFormat::Plain => {
            println!("Android Virtual Devices");
            if avds.is_empty() {
                println!("no AVDs");
                return Ok(());
            }
            for avd in avds {
                println!("{avd}");
            }
        }
        OutputFormat::Json => {
            print_json(json!({ "section": "emulators", "items": avds }))?;
        }
    }
    Ok(())
}

fn print_files(opts: &SectionOptions) -> Result<()> {
    let cfg = config::load_config();
    let root = opts.root.clone().or(cfg.gradle.project_dir);
    let state = files::FilesState::new(root.clone());
    let entries = limited(state.flatten_visible(), opts.limit);
    match opts.format {
        OutputFormat::Plain => {
            println!("Files");
            let Some(root) = root else {
                println!("no root configured; pass --root or set gradle.project_dir");
                return Ok(());
            };
            println!("root: {}", root.display());
            if let Some(err) = state.error {
                println!("{err}");
                return Ok(());
            }
            if entries.is_empty() {
                println!("no files");
                return Ok(());
            }
            for entry in entries {
                let kind = if entry.is_dir { "dir " } else { "file" };
                println!("{kind}  {}{}", "  ".repeat(entry.depth), entry.name);
            }
        }
        OutputFormat::Json => {
            let items: Vec<_> = entries
                .into_iter()
                .map(|e| {
                    json!({
                        "depth": e.depth,
                        "name": e.name,
                        "path": e.path,
                        "is_dir": e.is_dir,
                        "expanded": e.expanded,
                    })
                })
                .collect();
            print_json(json!({
                "section": "files",
                "root": root,
                "error": state.error,
                "items": items,
            }))?;
        }
    }
    Ok(())
}

fn print_config(opts: &SectionOptions) -> Result<()> {
    let cfg = config::load_config();
    let cfg_dir = config::config_dir();
    match opts.format {
        OutputFormat::Plain => {
            println!("Config");
            println!("config_dir: {}", cfg_dir.display());
            println!(
                "gradle.project_dir: {}",
                path_or_dash(cfg.gradle.project_dir.as_ref())
            );
            println!(
                "gradle.default_task: {}",
                cfg.gradle.default_task.as_deref().unwrap_or("-")
            );
            println!(
                "gradle.jar_path: {}",
                path_or_dash(cfg.gradle.jar_path.as_ref())
            );
            println!(
                "android.package: {}",
                cfg.android.package.as_deref().unwrap_or("-")
            );
            println!("ui.theme: {}", cfg.ui.theme);
        }
        OutputFormat::Json => {
            print_json(json!({
                "section": "config",
                "config_dir": cfg_dir,
                "files": {
                    "config": config::config_dir().join("config.toml"),
                    "state": config::config_dir().join("state.json"),
                    "workspaces": config::config_dir().join("workspaces.json"),
                },
                "gradle": {
                    "project_dir": cfg.gradle.project_dir,
                    "default_task": cfg.gradle.default_task,
                    "jar_path": cfg.gradle.jar_path,
                },
                "android": {
                    "package": cfg.android.package,
                },
                "ui": {
                    "theme": cfg.ui.theme,
                },
            }))?;
        }
    }
    Ok(())
}

fn print_workspaces(opts: &SectionOptions) -> Result<()> {
    let store = config::load_workspaces();
    let items = limited(store.workspaces, opts.limit);
    match opts.format {
        OutputFormat::Plain => {
            println!("Workspaces");
            println!("active: {}", store.active.as_deref().unwrap_or("-"));
            if items.is_empty() {
                println!("no saved workspaces");
                return Ok(());
            }
            for workspace in items {
                let marker = if Some(&workspace.id) == store.active.as_ref() {
                    "*"
                } else {
                    " "
                };
                println!(
                    "{marker} {:<24} {:<28} {}",
                    workspace.name,
                    workspace.default_task.as_deref().unwrap_or("-"),
                    workspace.project_dir.display()
                );
            }
        }
        OutputFormat::Json => {
            let values: Vec<_> = items
                .into_iter()
                .map(|w| {
                    json!({
                        "id": w.id,
                        "name": w.name,
                        "project_dir": w.project_dir,
                        "default_task": w.default_task,
                        "package": w.package,
                        "preferred_device": w.preferred_device,
                        "active_screen": w.active_screen,
                    })
                })
                .collect();
            print_json(json!({
                "section": "workspaces",
                "active": store.active,
                "items": values,
            }))?;
        }
    }
    Ok(())
}

fn print_panels(opts: &SectionOptions) -> Result<()> {
    let state = config::load_state();
    let visible: Vec<_> = state.visible.iter().map(|id| id.slug()).collect();
    match opts.format {
        OutputFormat::Plain => {
            println!("Panels");
            println!("active_screen: {}", state.active_screen + 1);
            println!("focus: {}", state.focus.slug());
            println!("visible: {}", visible.join(", "));
            println!();
            println!("{:<12} {:<8} {:<8} requires", "name", "toggle", "focus");
            for panel in PANELS {
                let requires = match panel.requires {
                    Feature::None => "-",
                    Feature::Jvm => "jvm",
                };
                println!(
                    "{:<12} {:<8} {:<8} {}",
                    panel.name, panel.toggle_key, panel.focus_key, requires
                );
            }
        }
        OutputFormat::Json => {
            let panels: Vec<_> = PANELS
                .iter()
                .map(|p| {
                    let requires = match p.requires {
                        Feature::None => "none",
                        Feature::Jvm => "jvm",
                    };
                    json!({
                        "id": p.id.slug(),
                        "name": p.name,
                        "toggle_key": p.toggle_key,
                        "focus_key": p.focus_key,
                        "requires": requires,
                    })
                })
                .collect();
            print_json(json!({
                "section": "panels",
                "active_screen": state.active_screen,
                "focus": state.focus.slug(),
                "visible": visible,
                "items": panels,
            }))?;
        }
    }
    Ok(())
}

fn print_gradle(opts: &SectionOptions) -> Result<()> {
    let cfg = config::load_config();
    let daemons = gradle::scan_host_gradle();
    match opts.format {
        OutputFormat::Plain => {
            println!("Gradle");
            println!(
                "project_dir: {}",
                path_or_dash(cfg.gradle.project_dir.as_ref())
            );
            println!(
                "default_task: {}",
                cfg.gradle.default_task.as_deref().unwrap_or("-")
            );
            println!("jar_path: {}", path_or_dash(cfg.gradle.jar_path.as_ref()));
            println!("java_available: {}", gradle::jvm_available());
            println!("host_processes: {}", daemons.len());
            if !daemons.is_empty() {
                println!();
                print_daemons(opts)?;
            }
        }
        OutputFormat::Json => {
            let daemon_items: Vec<_> = daemons
                .into_iter()
                .map(|proc| {
                    json!({
                        "kind": proc.kind,
                        "pid": proc.pid,
                        "cpu": proc.cpu,
                        "rss_kb": proc.rss_kb,
                        "command": proc.command,
                    })
                })
                .collect();
            print_json(json!({
                "section": "gradle",
                "project_dir": cfg.gradle.project_dir,
                "default_task": cfg.gradle.default_task,
                "jar_path": cfg.gradle.jar_path,
                "java_available": gradle::jvm_available(),
                "host_processes": daemon_items,
            }))?;
        }
    }
    Ok(())
}

fn print_actions(opts: &SectionOptions) -> Result<()> {
    let items: Vec<_> = device_actions::ACTIONS
        .iter()
        .map(|action| {
            json!({
                "label": action.label(),
                "description": action.description(),
                "needs_input": action.needs_input(),
                "runs_from_cli": false,
            })
        })
        .collect();
    print_info_section(
        opts,
        "actions",
        "device actions are interactive/mutating",
        items,
    )
}

fn print_app(opts: &SectionOptions) -> Result<()> {
    let package = package_or_config(opts);
    let items: Vec<_> = limited(app_control::ACTIONS.to_vec(), opts.limit)
        .into_iter()
        .map(|action| {
            json!({
                "label": action.label(),
                "description": action.description(),
                "destructive": action.destructive(),
                "runs_from_cli": false,
            })
        })
        .collect();
    match opts.format {
        OutputFormat::Plain => {
            println!("App control");
            println!("package: {}", package.as_deref().unwrap_or("-"));
            println!("available actions:");
            for item in items {
                println!(
                    "  {:<14} {}",
                    item["label"].as_str().unwrap_or("-"),
                    item["description"].as_str().unwrap_or("-")
                );
            }
        }
        OutputFormat::Json => {
            print_json(json!({
                "section": "app",
                "package": package,
                "items": items,
            }))?;
        }
    }
    Ok(())
}

fn print_data(opts: &SectionOptions) -> Result<()> {
    print_static_info(
        opts,
        "data",
        "app data browsing is read-only but interactive in the TUI",
        &[
            "files",
            "sqlite databases",
            "shared preferences",
            "datastore",
        ],
    )
}

fn print_intents(opts: &SectionOptions) -> Result<()> {
    print_static_info(
        opts,
        "intents",
        "intent launching is interactive/mutating in the TUI",
        &[
            "deep link url input",
            "resolver launch",
            "explicit package launch",
        ],
    )
}

fn print_shell(opts: &SectionOptions) -> Result<()> {
    print_static_info(
        opts,
        "shell",
        "embedded adb shell requires the interactive TUI",
        &["portable pty", "vt100 rendering", "Esc defocus"],
    )
}

fn print_manifest(opts: &SectionOptions) -> Result<()> {
    let Some(package) = package_or_config(opts) else {
        return print_static_info(
            opts,
            "manifest",
            "pass --package or configure android.package to inspect an installed APK",
            &[
                "installed APK path",
                "permissions",
                "components",
                "deep links",
            ],
        );
    };
    let handle = device_handle(&opts.device);
    let report = manifest::inspect(&handle, package);
    match opts.format {
        OutputFormat::Plain => {
            println!("{}", report.summary);
            println!("{}", report.output);
        }
        OutputFormat::Json => {
            print_json(json!({
                "section": "manifest",
                "package": report.package,
                "success": report.success,
                "summary": report.summary,
                "output": report.output,
            }))?;
        }
    }
    Ok(())
}

fn print_fps(opts: &SectionOptions) -> Result<()> {
    let Some(package) = package_or_config(opts) else {
        return print_static_info(
            opts,
            "fps",
            "pass --package or configure android.package for a one-shot gfxinfo sample",
            &["total frames", "janky frames", "p50/p90/p95/p99"],
        );
    };
    let handle = device_handle(&opts.device);
    let sample = fps::sample(&handle, &package).map_err(|e| color_eyre::eyre::eyre!("fps: {e}"))?;
    match opts.format {
        OutputFormat::Plain => {
            println!("FPS");
            println!("package: {package}");
            println!("total_frames: {}", sample.total_frames);
            println!("janky_frames: {}", sample.janky_frames);
            println!("janky_percent: {:.2}", sample.janky_percent);
            println!(
                "frame_ms: p50={:.2} p90={:.2} p95={:.2} p99={:.2}",
                sample.p50_ms, sample.p90_ms, sample.p95_ms, sample.p99_ms
            );
        }
        OutputFormat::Json => {
            print_json(json!({
                "section": "fps",
                "package": package,
                "sample": {
                    "total_frames": sample.total_frames,
                    "janky_frames": sample.janky_frames,
                    "janky_percent": sample.janky_percent,
                    "p50_ms": sample.p50_ms,
                    "p90_ms": sample.p90_ms,
                    "p95_ms": sample.p95_ms,
                    "p99_ms": sample.p99_ms,
                    "missed_vsync": sample.missed_vsync,
                    "high_input_latency": sample.high_input_latency,
                    "slow_ui": sample.slow_ui,
                    "slow_bitmap": sample.slow_bitmap,
                    "slow_draw": sample.slow_draw,
                },
            }))?;
        }
    }
    Ok(())
}

fn print_perf(opts: &SectionOptions) -> Result<()> {
    let Some(package) = package_or_config(opts) else {
        return print_static_info(
            opts,
            "perf",
            "pass --package or configure android.package for a one-shot perf sample",
            &["meminfo", "gfxinfo", "cpu percent"],
        );
    };
    let handle = device_handle(&opts.device);
    let sample =
        perf::sample_once(&handle, &package).map_err(|e| color_eyre::eyre::eyre!("perf: {e}"))?;
    match opts.format {
        OutputFormat::Plain => {
            println!("Perf");
            println!("package: {package}");
            println!("pid: {}", sample.pid);
            println!("pss_mb: {:.0}", sample.pss_total_kb as f64 / 1024.0);
            println!("rss_mb: {:.0}", sample.rss_total_kb as f64 / 1024.0);
            println!("cpu_percent: {:.1}", sample.cpu_percent);
            println!("jank_percent: {:.2}", sample.jank_percent);
        }
        OutputFormat::Json => {
            print_json(json!({
                "section": "perf",
                "package": package,
                "sample": {
                    "pid": sample.pid,
                    "pss_total_kb": sample.pss_total_kb,
                    "rss_total_kb": sample.rss_total_kb,
                    "java_heap_kb": sample.java_heap_kb,
                    "native_heap_kb": sample.native_heap_kb,
                    "code_kb": sample.code_kb,
                    "stack_kb": sample.stack_kb,
                    "graphics_kb": sample.graphics_kb,
                    "private_other_kb": sample.private_other_kb,
                    "system_kb": sample.system_kb,
                    "cpu_percent": sample.cpu_percent,
                    "jank_percent": sample.jank_percent,
                    "frames_total": sample.frames_total,
                    "p50_ms": sample.p50_ms,
                    "p90_ms": sample.p90_ms,
                    "p95_ms": sample.p95_ms,
                    "p99_ms": sample.p99_ms,
                },
            }))?;
        }
    }
    Ok(())
}

fn print_info_section(
    opts: &SectionOptions,
    section: &str,
    note: &str,
    items: Vec<Value>,
) -> Result<()> {
    let items = limited(items, opts.limit);
    match opts.format {
        OutputFormat::Plain => {
            println!("{}", title(section));
            println!("{note}");
            for item in items {
                println!(
                    "  {:<16} {}",
                    item["label"].as_str().unwrap_or("-"),
                    item["description"].as_str().unwrap_or("-")
                );
            }
        }
        OutputFormat::Json => {
            print_json(json!({
                "section": section,
                "note": note,
                "items": items,
            }))?;
        }
    }
    Ok(())
}

fn print_static_info(
    opts: &SectionOptions,
    section: &str,
    note: &str,
    capabilities: &[&str],
) -> Result<()> {
    let capabilities = limited(
        capabilities
            .iter()
            .map(|capability| capability.to_string())
            .collect(),
        opts.limit,
    );
    match opts.format {
        OutputFormat::Plain => {
            println!("{}", title(section));
            println!("{note}");
            for capability in &capabilities {
                println!("  {capability}");
            }
        }
        OutputFormat::Json => {
            print_json(json!({
                "section": section,
                "note": note,
                "capabilities": capabilities,
            }))?;
        }
    }
    Ok(())
}

fn read_logcat(opts: &SectionOptions) -> Result<Vec<logcat::LogLine>> {
    let handle = device_handle(&opts.device);
    let output = adb::command(&handle)
        .args(["logcat", "-d", "-v", "threadtime"])
        .output()
        .map_err(|e| color_eyre::eyre::eyre!("logcat: {e}"))?;
    if !output.status.success() {
        bail!("logcat: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().filter_map(logcat::LogLine::parse).collect())
}

fn log_lines_json(lines: Vec<logcat::LogLine>) -> Vec<Value> {
    lines
        .into_iter()
        .map(|line| {
            json!({
                "timestamp": line.timestamp,
                "pid": line.pid,
                "level": line.level.short(),
                "tag": line.tag,
                "message": line.message,
            })
        })
        .collect()
}

fn is_network_line(line: &logcat::LogLine) -> bool {
    let tag = line.tag.to_lowercase();
    let message = line.message.to_lowercase();
    NETWORK_KEYWORDS
        .iter()
        .any(|needle| tag.contains(needle) || message.contains(needle))
}

fn package_roots(opts: &SectionOptions) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(root) = &opts.root {
        push_unique_path(&mut roots, root.clone());
        return roots;
    }
    let cfg = config::load_config();
    if let Some(project_dir) = cfg.gradle.project_dir {
        push_unique_path(&mut roots, project_dir);
    }
    for workspace in config::load_workspaces().workspaces {
        push_unique_path(&mut roots, workspace.project_dir);
    }
    push_unique_path(&mut roots, device_tools::default_root());
    roots
}

fn seed_packages() -> Vec<device_tools::WorkPackage> {
    let mut out = Vec::new();
    let cfg = config::load_config();
    if let (Some(package), Some(project_dir)) = (cfg.android.package, cfg.gradle.project_dir) {
        out.push(device_tools::WorkPackage::new(
            package,
            project_dir,
            "config".to_string(),
        ));
    }
    for workspace in config::load_workspaces().workspaces {
        if let Some(package) = workspace.package {
            out.push(device_tools::WorkPackage::new(
                package,
                workspace.project_dir,
                "workspace".to_string(),
            ));
        }
    }
    out
}

fn push_unique_path(out: &mut Vec<PathBuf>, path: PathBuf) {
    if !out.iter().any(|existing| existing == &path) {
        out.push(path);
    }
}

fn package_or_config(opts: &SectionOptions) -> Option<String> {
    opts.package
        .clone()
        .or_else(|| config::load_config().android.package)
}

fn device_handle(serial: &Option<String>) -> adb::DeviceHandle {
    let handle = adb::new_handle();
    if let Some(serial) = serial {
        if let Ok(mut guard) = handle.lock() {
            *guard = Some(serial.clone());
        }
    }
    handle
}

fn path_or_dash(path: Option<&PathBuf>) -> String {
    path.map(|p| p.display().to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn title(section: &str) -> String {
    let mut chars = section.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn limited<T>(items: Vec<T>, limit: usize) -> Vec<T> {
    if items.len() <= limit {
        items
    } else {
        items.into_iter().take(limit).collect()
    }
}

fn print_json(value: Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn parse_options(args: &[String]) -> Result<SectionOptions> {
    let mut opts = SectionOptions::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                opts.format = OutputFormat::Json;
                i += 1;
            }
            "--format" => {
                let Some(value) = args.get(i + 1) else {
                    bail!("missing value for --format");
                };
                opts.format = parse_format_value(value)?;
                i += 2;
            }
            arg if arg.starts_with("--format=") => {
                opts.format = parse_format_value(arg.trim_start_matches("--format="))?;
                i += 1;
            }
            "--limit" | "-n" => {
                let Some(value) = args.get(i + 1) else {
                    bail!("missing value for {}", args[i]);
                };
                opts.limit = parse_limit(value)?;
                i += 2;
            }
            arg if arg.starts_with("--limit=") => {
                opts.limit = parse_limit(arg.trim_start_matches("--limit="))?;
                i += 1;
            }
            "--device" | "-s" => {
                let Some(value) = args.get(i + 1) else {
                    bail!("missing value for {}", args[i]);
                };
                opts.device = Some(value.clone());
                i += 2;
            }
            arg if arg.starts_with("--device=") => {
                opts.device = Some(arg.trim_start_matches("--device=").to_string());
                i += 1;
            }
            "--root" => {
                let Some(value) = args.get(i + 1) else {
                    bail!("missing value for --root");
                };
                opts.root = Some(PathBuf::from(expand_tilde(value)));
                i += 2;
            }
            arg if arg.starts_with("--root=") => {
                opts.root = Some(PathBuf::from(expand_tilde(
                    arg.trim_start_matches("--root="),
                )));
                i += 1;
            }
            "--package" | "-p" => {
                let Some(value) = args.get(i + 1) else {
                    bail!("missing value for {}", args[i]);
                };
                opts.package = Some(value.clone());
                i += 2;
            }
            arg if arg.starts_with("--package=") => {
                opts.package = Some(arg.trim_start_matches("--package=").to_string());
                i += 1;
            }
            other => bail!("unknown argument `{other}`"),
        }
    }
    Ok(opts)
}

fn parse_format_value(value: &str) -> Result<OutputFormat> {
    match value {
        "plain" | "text" => Ok(OutputFormat::Plain),
        "json" => Ok(OutputFormat::Json),
        other => bail!("unknown format `{other}`; expected plain or json"),
    }
}

fn parse_limit(value: &str) -> Result<usize> {
    let limit = value
        .parse::<usize>()
        .map_err(|_| color_eyre::eyre::eyre!("invalid limit `{value}`"))?;
    if limit == 0 {
        bail!("limit must be greater than zero");
    }
    Ok(limit)
}

fn expand_tilde(value: &str) -> String {
    if value == "~" {
        return dirs::home_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| value.to_string());
    }
    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).display().to_string();
        }
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_json_flag() {
        assert_eq!(
            parse_options(&args(&["--json"])).unwrap().format,
            OutputFormat::Json
        );
    }

    #[test]
    fn parses_format_value() {
        assert_eq!(
            parse_options(&args(&["--format", "json"])).unwrap().format,
            OutputFormat::Json
        );
        assert_eq!(
            parse_options(&args(&["--format=plain"])).unwrap().format,
            OutputFormat::Plain
        );
    }

    #[test]
    fn parses_section_options() {
        let opts = parse_options(&args(&[
            "--limit",
            "12",
            "--device",
            "emulator-5554",
            "--root=~/Documents",
            "--package",
            "com.example",
        ]))
        .unwrap();
        assert_eq!(opts.limit, 12);
        assert_eq!(opts.device.as_deref(), Some("emulator-5554"));
        assert!(opts.root.is_some());
        assert_eq!(opts.package.as_deref(), Some("com.example"));
    }

    #[test]
    fn rejects_unknown_argument() {
        assert!(parse_options(&args(&["--wat"])).is_err());
    }
}
