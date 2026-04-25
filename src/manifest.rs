use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::adb::{self, DeviceHandle};
use crate::dispatch::Event;

#[derive(Default)]
pub struct ManifestState {
    pub running: bool,
    pub scroll: usize,
    pub last: Option<ManifestReport>,
}

impl ManifestState {
    pub fn reset_for_package(&mut self) {
        self.running = false;
        self.scroll = 0;
        self.last = None;
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_add(n);
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }
}

#[derive(Debug, Clone)]
pub struct ManifestReport {
    pub package: String,
    pub success: bool,
    pub summary: String,
    pub output: String,
}

#[derive(Default)]
struct ParsedManifest {
    package: Option<String>,
    version_code: Option<String>,
    version_name: Option<String>,
    permissions: Vec<String>,
    activities: Vec<Component>,
    services: Vec<Component>,
    receivers: Vec<Component>,
    deeplinks: Vec<DeepLink>,
}

#[derive(Debug, Clone)]
struct Component {
    name: String,
    exported: Option<String>,
    enabled: Option<String>,
}

impl Component {
    fn new() -> Self {
        Self {
            name: "(unnamed)".to_string(),
            exported: None,
            enabled: None,
        }
    }
}

#[derive(Debug, Clone)]
struct FilterCtx {
    indent: usize,
    component: String,
    actions: Vec<String>,
    categories: Vec<String>,
    data: Vec<DataSpec>,
}

#[derive(Debug, Clone, Default)]
struct DataSpec {
    scheme: Option<String>,
    host: Option<String>,
    port: Option<String>,
    path: Option<String>,
    path_prefix: Option<String>,
    path_pattern: Option<String>,
    mime_type: Option<String>,
}

#[derive(Debug, Clone)]
struct DeepLink {
    component: String,
    actions: Vec<String>,
    categories: Vec<String>,
    data: Vec<DataSpec>,
}

#[derive(Debug, Clone, Copy)]
enum ComponentKind {
    Activity,
    Service,
    Receiver,
}

pub fn spawn_inspect(handle: DeviceHandle, package: String, tx: Sender<Event>) {
    thread::spawn(move || {
        let report = inspect(&handle, package);
        let _ = tx.send(Event::Manifest(report));
    });
}

fn inspect(handle: &DeviceHandle, package: String) -> ManifestReport {
    if let Err(message) = validate_package(&package) {
        return ManifestReport {
            package,
            success: false,
            summary: message.clone(),
            output: message,
        };
    }

    let apk_paths = match installed_apk_paths(handle, &package) {
        Ok(paths) if !paths.is_empty() => paths,
        Ok(_) => {
            let message = "pm path returned no APK paths".to_string();
            return failure(package, message);
        }
        Err(message) => return failure(package, message),
    };

    let base_apk = apk_paths
        .iter()
        .find(|p| p.ends_with("/base.apk"))
        .or_else(|| apk_paths.first())
        .cloned()
        .unwrap_or_default();

    let mut notes = Vec::new();
    let mut parsed = ParsedManifest::default();
    let mut tool_label = "dumpsys package fallback".to_string();
    let mut success = true;
    let mut inspected = false;

    if let Some(aapt) = find_aapt() {
        match pull_and_inspect(handle, &package, &base_apk, |local| {
            run_aapt(&aapt, local)
        }) {
            Ok((label, manifest)) => {
                tool_label = format!("aapt ({})", aapt.display());
                notes.push(label);
                parsed = manifest;
                inspected = true;
            }
            Err(message) => {
                notes.push(format!("aapt failed: {message}"));
                success = false;
            }
        }
    }

    if !inspected {
        if let Some(apkanalyzer) = find_apkanalyzer() {
            match pull_and_inspect(handle, &package, &base_apk, |local| {
                run_apkanalyzer(&apkanalyzer, local)
            }) {
                Ok((label, manifest)) => {
                    tool_label = format!("apkanalyzer ({})", apkanalyzer.display());
                    notes.push(label);
                    parsed = manifest;
                    inspected = true;
                }
                Err(message) => {
                    notes.push(format!("apkanalyzer failed: {message}"));
                    success = false;
                }
            }
        }
    }

    if !inspected {
        if notes.is_empty() {
            notes.push("aapt/apkanalyzer not found; using dumpsys package summary".to_string());
        } else {
            notes.push("using dumpsys package summary fallback".to_string());
        }
        match dumpsys_package(handle, &package) {
            Ok(text) => parsed = parse_dumpsys(&text),
            Err(message) => {
                notes.push(format!("dumpsys failed: {message}"));
                success = false;
            }
        }
    }

    let output = render_report(&package, &apk_paths, &base_apk, &tool_label, &notes, &parsed);
    let summary = if success {
        format!("manifest: {} via {}", package, short_tool(&tool_label))
    } else {
        format!("manifest: {} with warnings", package)
    };
    ManifestReport {
        package,
        success,
        summary,
        output,
    }
}

fn failure(package: String, message: String) -> ManifestReport {
    ManifestReport {
        package,
        success: false,
        summary: message.clone(),
        output: message,
    }
}

fn installed_apk_paths(handle: &DeviceHandle, package: &str) -> Result<Vec<String>, String> {
    let output = adb::command(handle)
        .args(["shell", "pm", "path", package])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(output_text(output));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("package:"))
        .map(str::to_string)
        .collect())
}

fn pull_and_inspect<F>(
    handle: &DeviceHandle,
    package: &str,
    remote_apk: &str,
    inspect_local: F,
) -> Result<(String, ParsedManifest), String>
where
    F: FnOnce(&Path) -> Result<ParsedManifest, String>,
{
    let dir = temp_dir(package)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let local = dir.join("base.apk");
    let output = adb::command(handle)
        .args(["pull", remote_apk, local.to_string_lossy().as_ref()])
        .output()
        .map_err(|e| e.to_string());
    let result = match output {
        Ok(output) if output.status.success() => inspect_local(&local),
        Ok(output) => Err(output_text(output)),
        Err(message) => Err(message),
    };
    let _ = fs::remove_dir_all(&dir);
    result.map(|manifest| (format!("pulled {remote_apk}"), manifest))
}

fn run_aapt(aapt: &Path, apk: &Path) -> Result<ParsedManifest, String> {
    let output = Command::new(aapt)
        .args(["dump", "xmltree"])
        .arg(apk)
        .arg("AndroidManifest.xml")
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(output_text(output));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_aapt_xmltree(&text))
}

fn run_apkanalyzer(apkanalyzer: &Path, apk: &Path) -> Result<ParsedManifest, String> {
    let output = Command::new(apkanalyzer)
        .args(["manifest", "print"])
        .arg(apk)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(output_text(output));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_manifest_xml(&text))
}

fn dumpsys_package(handle: &DeviceHandle, package: &str) -> Result<String, String> {
    let output = adb::command(handle)
        .args(["shell", "dumpsys", "package", package])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(output_text(output));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_aapt_xmltree(text: &str) -> ParsedManifest {
    let mut parsed = ParsedManifest::default();
    let mut element: Option<(usize, String)> = None;
    let mut component: Option<(usize, ComponentKind, usize)> = None;
    let mut filter: Option<FilterCtx> = None;
    let mut leaf: Option<(usize, String)> = None;

    for line in text.lines() {
        let indent = leading_spaces(line);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        close_contexts(indent, &mut element, &mut component, &mut filter, &mut leaf, &mut parsed);

        if let Some(rest) = trimmed.strip_prefix("E: ") {
            let tag = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            element = Some((indent, tag.clone()));
            match tag.as_str() {
                "activity" | "activity-alias" => {
                    parsed.activities.push(Component::new());
                    component = Some((indent, ComponentKind::Activity, parsed.activities.len() - 1));
                }
                "service" => {
                    parsed.services.push(Component::new());
                    component = Some((indent, ComponentKind::Service, parsed.services.len() - 1));
                }
                "receiver" => {
                    parsed.receivers.push(Component::new());
                    component = Some((indent, ComponentKind::Receiver, parsed.receivers.len() - 1));
                }
                "intent-filter" => {
                    if let Some((_, kind, index)) = component {
                        filter = Some(FilterCtx {
                            indent,
                            component: component_name(&parsed, kind, index),
                            actions: Vec::new(),
                            categories: Vec::new(),
                            data: Vec::new(),
                        });
                    }
                }
                "action" | "category" | "data" => {
                    if let Some(ctx) = filter.as_mut() {
                        if tag == "data" {
                            ctx.data.push(DataSpec::default());
                        }
                        leaf = Some((indent, tag));
                    }
                }
                _ => {}
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("A: ") {
            let Some((key, value)) = parse_aapt_attr(rest) else {
                continue;
            };
            if let Some((_, tag)) = &element {
                apply_element_attr(&mut parsed, tag, &key, &value);
            }
            if let Some((_, kind, index)) = component {
                if leaf.is_none() {
                    apply_component_attr(&mut parsed, kind, index, &key, &value);
                    if let Some(ctx) = filter.as_mut() {
                        ctx.component = component_name(&parsed, kind, index);
                    }
                }
            }
            if let (Some(ctx), Some((_, leaf_tag))) = (filter.as_mut(), leaf.as_ref()) {
                apply_filter_attr(ctx, leaf_tag, &key, &value);
            }
        }
    }

    close_contexts(0, &mut element, &mut component, &mut filter, &mut leaf, &mut parsed);
    dedup_parsed(&mut parsed);
    parsed
}

fn parse_manifest_xml(text: &str) -> ParsedManifest {
    let mut parsed = ParsedManifest::default();
    let mut component: Option<(usize, ComponentKind, usize)> = None;
    let mut filter: Option<FilterCtx> = None;
    let mut leaf: Option<(usize, String)> = None;

    for line in text.lines() {
        let indent = leading_spaces(line);
        let trimmed = line.trim();
        if trimmed.starts_with("</") {
            close_contexts(indent, &mut None, &mut component, &mut filter, &mut leaf, &mut parsed);
            continue;
        }
        close_contexts(indent, &mut None, &mut component, &mut filter, &mut leaf, &mut parsed);
        let Some(tag) = xml_start_tag(trimmed) else {
            if let Some((_, leaf_tag)) = leaf.as_ref() {
                if let Some(ctx) = filter.as_mut() {
                    for (key, value) in parse_xml_attrs(trimmed) {
                        apply_filter_attr(ctx, leaf_tag, &key, &value);
                    }
                }
            } else if let Some((_, kind, index)) = component {
                for (key, value) in parse_xml_attrs(trimmed) {
                    apply_component_attr(&mut parsed, kind, index, &key, &value);
                    if let Some(ctx) = filter.as_mut() {
                        ctx.component = component_name(&parsed, kind, index);
                    }
                }
            }
            continue;
        };
        match tag.as_str() {
            "manifest" => {
                for (key, value) in parse_xml_attrs(trimmed) {
                    apply_element_attr(&mut parsed, "manifest", &key, &value);
                }
            }
            "uses-permission" | "uses-permission-sdk-23" => {
                for (key, value) in parse_xml_attrs(trimmed) {
                    apply_element_attr(&mut parsed, &tag, &key, &value);
                }
            }
            "activity" | "activity-alias" => {
                parsed.activities.push(Component::new());
                let index = parsed.activities.len() - 1;
                component = Some((indent, ComponentKind::Activity, index));
                for (key, value) in parse_xml_attrs(trimmed) {
                    apply_component_attr(&mut parsed, ComponentKind::Activity, index, &key, &value);
                }
            }
            "service" => {
                parsed.services.push(Component::new());
                let index = parsed.services.len() - 1;
                component = Some((indent, ComponentKind::Service, index));
                for (key, value) in parse_xml_attrs(trimmed) {
                    apply_component_attr(&mut parsed, ComponentKind::Service, index, &key, &value);
                }
            }
            "receiver" => {
                parsed.receivers.push(Component::new());
                let index = parsed.receivers.len() - 1;
                component = Some((indent, ComponentKind::Receiver, index));
                for (key, value) in parse_xml_attrs(trimmed) {
                    apply_component_attr(&mut parsed, ComponentKind::Receiver, index, &key, &value);
                }
            }
            "intent-filter" => {
                if let Some((_, kind, index)) = component {
                    filter = Some(FilterCtx {
                        indent,
                        component: component_name(&parsed, kind, index),
                        actions: Vec::new(),
                        categories: Vec::new(),
                        data: Vec::new(),
                    });
                }
            }
            "action" | "category" | "data" => {
                if let Some(ctx) = filter.as_mut() {
                    if tag == "data" {
                        ctx.data.push(DataSpec::default());
                    }
                    for (key, value) in parse_xml_attrs(trimmed) {
                        apply_filter_attr(ctx, &tag, &key, &value);
                    }
                    leaf = Some((indent, tag));
                }
            }
            _ => {}
        }
    }

    close_contexts(0, &mut None, &mut component, &mut filter, &mut leaf, &mut parsed);
    dedup_parsed(&mut parsed);
    parsed
}

fn parse_dumpsys(text: &str) -> ParsedManifest {
    let mut parsed = ParsedManifest::default();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("versionCode=") {
            parsed.version_code = value.split_whitespace().next().map(str::to_string);
        } else if let Some(value) = trimmed.strip_prefix("versionName=") {
            parsed.version_name = Some(value.to_string());
        } else if trimmed.starts_with("android.permission.") {
            push_unique(&mut parsed.permissions, trimmed.to_string());
        }
    }
    parsed
}

fn close_contexts(
    indent: usize,
    element: &mut Option<(usize, String)>,
    component: &mut Option<(usize, ComponentKind, usize)>,
    filter: &mut Option<FilterCtx>,
    leaf: &mut Option<(usize, String)>,
    parsed: &mut ParsedManifest,
) {
    if element.as_ref().is_some_and(|(i, _)| indent <= *i) {
        *element = None;
    }
    if leaf.as_ref().is_some_and(|(i, _)| indent <= *i) {
        *leaf = None;
    }
    if filter.as_ref().is_some_and(|ctx| indent <= ctx.indent) {
        if let Some(ctx) = filter.take() {
            if !ctx.data.is_empty() {
                parsed.deeplinks.push(DeepLink {
                    component: ctx.component,
                    actions: ctx.actions,
                    categories: ctx.categories,
                    data: ctx.data,
                });
            }
        }
    }
    if component.as_ref().is_some_and(|(i, _, _)| indent <= *i) {
        *component = None;
    }
}

fn apply_element_attr(parsed: &mut ParsedManifest, tag: &str, key: &str, value: &str) {
    match tag {
        "manifest" => match short_key(key) {
            "package" => parsed.package = Some(value.to_string()),
            "versionCode" => parsed.version_code = Some(normalize_int(value)),
            "versionName" => parsed.version_name = Some(value.to_string()),
            _ => {}
        },
        "uses-permission" | "uses-permission-sdk-23" => {
            if short_key(key) == "name" {
                push_unique(&mut parsed.permissions, value.to_string());
            }
        }
        _ => {}
    }
}

fn apply_component_attr(
    parsed: &mut ParsedManifest,
    kind: ComponentKind,
    index: usize,
    key: &str,
    value: &str,
) {
    let component = match kind {
        ComponentKind::Activity => parsed.activities.get_mut(index),
        ComponentKind::Service => parsed.services.get_mut(index),
        ComponentKind::Receiver => parsed.receivers.get_mut(index),
    };
    let Some(component) = component else {
        return;
    };
    match short_key(key) {
        "name" => component.name = value.to_string(),
        "exported" => component.exported = Some(normalize_bool(value)),
        "enabled" => component.enabled = Some(normalize_bool(value)),
        _ => {}
    }
}

fn apply_filter_attr(ctx: &mut FilterCtx, tag: &str, key: &str, value: &str) {
    match (tag, short_key(key)) {
        ("action", "name") => push_unique(&mut ctx.actions, value.to_string()),
        ("category", "name") => push_unique(&mut ctx.categories, value.to_string()),
        ("data", data_key) => {
            if ctx.data.is_empty() {
                ctx.data.push(DataSpec::default());
            }
            let data = ctx.data.last_mut().expect("data just inserted");
            match data_key {
                "scheme" => data.scheme = Some(value.to_string()),
                "host" => data.host = Some(value.to_string()),
                "port" => data.port = Some(value.to_string()),
                "path" => data.path = Some(value.to_string()),
                "pathPrefix" => data.path_prefix = Some(value.to_string()),
                "pathPattern" => data.path_pattern = Some(value.to_string()),
                "mimeType" => data.mime_type = Some(value.to_string()),
                _ => {}
            }
        }
        _ => {}
    }
}

fn component_name(parsed: &ParsedManifest, kind: ComponentKind, index: usize) -> String {
    let item = match kind {
        ComponentKind::Activity => parsed.activities.get(index),
        ComponentKind::Service => parsed.services.get(index),
        ComponentKind::Receiver => parsed.receivers.get(index),
    };
    item.map(|c| c.name.clone()).unwrap_or_else(|| "(unnamed)".to_string())
}

fn render_report(
    target_package: &str,
    apk_paths: &[String],
    base_apk: &str,
    tool: &str,
    notes: &[String],
    parsed: &ParsedManifest,
) -> String {
    let mut out = Vec::new();
    out.push("APK / Manifest inspector".to_string());
    out.push(format!("target package: {target_package}"));
    if let Some(package) = &parsed.package {
        out.push(format!("manifest package: {package}"));
    }
    out.push(format!("inspector: {tool}"));
    out.push(format!("base APK: {base_apk}"));
    out.push("installed APK paths:".to_string());
    for path in apk_paths {
        out.push(format!("  {path}"));
    }
    if !notes.is_empty() {
        out.push("notes:".to_string());
        for note in notes {
            out.push(format!("  {note}"));
        }
    }
    out.push(String::new());
    out.push("version:".to_string());
    out.push(format!(
        "  versionCode: {}",
        parsed.version_code.as_deref().unwrap_or("(unknown)")
    ));
    out.push(format!(
        "  versionName: {}",
        parsed.version_name.as_deref().unwrap_or("(unknown)")
    ));
    out.push(String::new());
    render_list(&mut out, "permissions", &parsed.permissions);
    render_components(&mut out, "activities", &parsed.activities);
    render_components(&mut out, "services", &parsed.services);
    render_components(&mut out, "receivers", &parsed.receivers);
    render_deeplinks(&mut out, &parsed.deeplinks);
    out.join("\n")
}

fn render_list(out: &mut Vec<String>, label: &str, items: &[String]) {
    out.push(format!("{label} ({}):", items.len()));
    if items.is_empty() {
        out.push("  (none found)".to_string());
    } else {
        for item in items {
            out.push(format!("  {item}"));
        }
    }
    out.push(String::new());
}

fn render_components(out: &mut Vec<String>, label: &str, items: &[Component]) {
    out.push(format!("{label} ({}):", items.len()));
    if items.is_empty() {
        out.push("  (none found)".to_string());
    } else {
        for item in items {
            let mut flags = Vec::new();
            if let Some(exported) = &item.exported {
                flags.push(format!("exported={exported}"));
            }
            if let Some(enabled) = &item.enabled {
                flags.push(format!("enabled={enabled}"));
            }
            if flags.is_empty() {
                out.push(format!("  {}", item.name));
            } else {
                out.push(format!("  {}  [{}]", item.name, flags.join(", ")));
            }
        }
    }
    out.push(String::new());
}

fn render_deeplinks(out: &mut Vec<String>, items: &[DeepLink]) {
    out.push(format!("deeplinks ({}):", items.len()));
    if items.is_empty() {
        out.push("  (none found)".to_string());
    } else {
        for item in items {
            out.push(format!("  {}", item.component));
            if !item.actions.is_empty() {
                out.push(format!("    actions: {}", item.actions.join(", ")));
            }
            if !item.categories.is_empty() {
                out.push(format!("    categories: {}", item.categories.join(", ")));
            }
            for data in &item.data {
                out.push(format!("    data: {}", render_data(data)));
            }
        }
    }
}

fn render_data(data: &DataSpec) -> String {
    let mut parts = Vec::new();
    if let Some(scheme) = &data.scheme {
        parts.push(format!("scheme={scheme}"));
    }
    if let Some(host) = &data.host {
        parts.push(format!("host={host}"));
    }
    if let Some(port) = &data.port {
        parts.push(format!("port={port}"));
    }
    if let Some(path) = &data.path {
        parts.push(format!("path={path}"));
    }
    if let Some(path_prefix) = &data.path_prefix {
        parts.push(format!("pathPrefix={path_prefix}"));
    }
    if let Some(path_pattern) = &data.path_pattern {
        parts.push(format!("pathPattern={path_pattern}"));
    }
    if let Some(mime_type) = &data.mime_type {
        parts.push(format!("mimeType={mime_type}"));
    }
    if parts.is_empty() {
        "(empty data tag)".to_string()
    } else {
        parts.join(" ")
    }
}

fn dedup_parsed(parsed: &mut ParsedManifest) {
    parsed.permissions.sort();
    parsed.permissions.dedup();
    parsed.activities.sort_by(|a, b| a.name.cmp(&b.name));
    parsed.services.sort_by(|a, b| a.name.cmp(&b.name));
    parsed.receivers.sort_by(|a, b| a.name.cmp(&b.name));
    parsed.deeplinks.sort_by(|a, b| a.component.cmp(&b.component));
}

fn parse_aapt_attr(input: &str) -> Option<(String, String)> {
    let (left, right) = input.split_once('=')?;
    let key = left.split('(').next().unwrap_or(left).trim().to_string();
    let value = parse_attr_value(right.trim());
    Some((key, value))
}

fn parse_attr_value(raw: &str) -> String {
    if let Some(start) = raw.find('"') {
        if let Some(end) = raw[start + 1..].find('"') {
            return raw[start + 1..start + 1 + end].to_string();
        }
    }
    raw.strip_prefix("(type ")
        .and_then(|s| s.split(')').nth(1))
        .unwrap_or(raw)
        .trim()
        .to_string()
}

fn xml_start_tag(line: &str) -> Option<String> {
    let rest = line.strip_prefix('<')?;
    if rest.starts_with('/') || rest.starts_with('!') || rest.starts_with('?') {
        return None;
    }
    Some(
        rest.split(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .next()
            .unwrap_or("")
            .to_string(),
    )
}

fn parse_xml_attrs(line: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut rest = line;
    while let Some(eq) = rest.find('=') {
        let left = &rest[..eq];
        let key = left
            .split_whitespace()
            .last()
            .unwrap_or("")
            .trim_matches('<')
            .to_string();
        let after_eq = rest[eq + 1..].trim_start();
        let Some(quote) = after_eq.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            break;
        };
        let value_start = quote.len_utf8();
        let Some(value_end) = after_eq[value_start..].find(quote) else {
            break;
        };
        let value = after_eq[value_start..value_start + value_end].to_string();
        if !key.is_empty() {
            attrs.push((key, value));
        }
        rest = &after_eq[value_start + value_end + quote.len_utf8()..];
    }
    attrs
}

fn short_key(key: &str) -> &str {
    key.rsplit(':').next().unwrap_or(key)
}

fn normalize_int(value: &str) -> String {
    if let Some(hex) = value.strip_prefix("0x") {
        if let Ok(num) = u64::from_str_radix(hex, 16) {
            return num.to_string();
        }
    }
    value.to_string()
}

fn normalize_bool(value: &str) -> String {
    match value {
        "0xffffffff" => "true".to_string(),
        "0x0" => "false".to_string(),
        _ => value.to_string(),
    }
}

fn leading_spaces(s: &str) -> usize {
    s.chars().take_while(|c| c.is_whitespace()).count()
}

fn push_unique(items: &mut Vec<String>, value: String) {
    if !value.is_empty() && !items.iter().any(|item| item == &value) {
        items.push(value);
    }
}

fn output_text(output: Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else if stdout.trim().is_empty() {
        stderr.trim().to_string()
    } else {
        format!("{}\n{}", stdout.trim(), stderr.trim())
    }
}

fn temp_dir(package: &str) -> Result<PathBuf, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let safe = package.replace('.', "_");
    Ok(env::temp_dir().join(format!("droidscope-apk-{safe}-{}-{now}", std::process::id())))
}

fn find_aapt() -> Option<PathBuf> {
    find_in_path("aapt").or_else(|| find_android_tool("build-tools", "aapt"))
}

fn find_apkanalyzer() -> Option<PathBuf> {
    find_in_path("apkanalyzer")
        .or_else(|| find_android_tool("cmdline-tools", "apkanalyzer"))
        .or_else(|| find_android_tool("tools", "apkanalyzer"))
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

fn find_android_tool(group: &str, name: &str) -> Option<PathBuf> {
    let mut sdk_roots = Vec::new();
    if let Some(path) = env::var_os("ANDROID_HOME") {
        sdk_roots.push(PathBuf::from(path));
    }
    if let Some(path) = env::var_os("ANDROID_SDK_ROOT") {
        sdk_roots.push(PathBuf::from(path));
    }
    if let Some(home) = dirs::home_dir() {
        sdk_roots.push(home.join("Library/Android/sdk"));
        sdk_roots.push(home.join("Android/Sdk"));
    }

    let mut candidates = Vec::new();
    for sdk in sdk_roots {
        match group {
            "build-tools" => collect_nested_tool(&mut candidates, &sdk.join(group), name),
            "cmdline-tools" => collect_nested_tool(&mut candidates, &sdk.join(group), &format!("bin/{name}")),
            "tools" => candidates.push(sdk.join("tools").join("bin").join(name)),
            _ => {}
        }
    }
    candidates.sort();
    candidates.into_iter().rev().find(|path| path.is_file())
}

fn collect_nested_tool(candidates: &mut Vec<PathBuf>, root: &Path, name: &str) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            candidates.push(path.join(name));
        }
    }
}

fn validate_package(package: &str) -> Result<(), String> {
    if package.trim().is_empty() {
        return Err("target package is empty".to_string());
    }
    let valid = package
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.'));
    if valid {
        Ok(())
    } else {
        Err("target package contains unsupported characters".to_string())
    }
}

fn short_tool(tool: &str) -> &str {
    tool.split_whitespace().next().unwrap_or(tool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_aapt_manifest_summary() {
        let raw = r#"
E: manifest (line=2)
  A: package="com.example.app" (Raw: "com.example.app")
  A: android:versionCode(0x0101021b)=(type 0x10)0x2a
  A: android:versionName(0x0101021c)="1.2.3" (Raw: "1.2.3")
  E: uses-permission (line=5)
    A: android:name(0x01010003)="android.permission.INTERNET" (Raw: "android.permission.INTERNET")
  E: application (line=8)
    E: activity (line=10)
      A: android:name(0x01010003)=".MainActivity" (Raw: ".MainActivity")
      A: android:exported(0x01010010)=(type 0x12)0xffffffff
      E: intent-filter (line=12)
        E: action (line=13)
          A: android:name(0x01010003)="android.intent.action.VIEW" (Raw: "android.intent.action.VIEW")
        E: category (line=14)
          A: android:name(0x01010003)="android.intent.category.BROWSABLE" (Raw: "android.intent.category.BROWSABLE")
        E: data (line=15)
          A: android:scheme(0x01010027)="https" (Raw: "https")
          A: android:host(0x01010028)="example.com" (Raw: "example.com")
    E: service (line=20)
      A: android:name(0x01010003)=".SyncService" (Raw: ".SyncService")
    E: receiver (line=25)
      A: android:name(0x01010003)=".BootReceiver" (Raw: ".BootReceiver")
"#;

        let parsed = parse_aapt_xmltree(raw);

        assert_eq!(parsed.package.as_deref(), Some("com.example.app"));
        assert_eq!(parsed.version_code.as_deref(), Some("42"));
        assert_eq!(parsed.version_name.as_deref(), Some("1.2.3"));
        assert_eq!(parsed.permissions, vec!["android.permission.INTERNET"]);
        assert_eq!(parsed.activities[0].name, ".MainActivity");
        assert_eq!(parsed.activities[0].exported.as_deref(), Some("true"));
        assert_eq!(parsed.services[0].name, ".SyncService");
        assert_eq!(parsed.receivers[0].name, ".BootReceiver");
        assert_eq!(parsed.deeplinks.len(), 1);
        assert_eq!(parsed.deeplinks[0].data[0].scheme.as_deref(), Some("https"));
        assert_eq!(parsed.deeplinks[0].data[0].host.as_deref(), Some("example.com"));
    }

    #[test]
    fn parses_xml_multiple_data_tags() {
        let raw = r#"
<manifest package="com.example.app" android:versionCode="7" android:versionName="2.0">
    <uses-permission android:name="android.permission.CAMERA" />
    <application>
        <activity android:name=".MainActivity" android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.VIEW" />
                <category android:name="android.intent.category.DEFAULT" />
                <data android:scheme="myapp" />
                <data android:scheme="https" android:host="example.com" android:pathPrefix="/open" />
            </intent-filter>
        </activity>
    </application>
</manifest>
"#;

        let parsed = parse_manifest_xml(raw);

        assert_eq!(parsed.version_code.as_deref(), Some("7"));
        assert_eq!(parsed.version_name.as_deref(), Some("2.0"));
        assert_eq!(parsed.permissions, vec!["android.permission.CAMERA"]);
        assert_eq!(parsed.activities[0].name, ".MainActivity");
        assert_eq!(parsed.deeplinks.len(), 1);
        assert_eq!(parsed.deeplinks[0].data.len(), 2);
        assert_eq!(parsed.deeplinks[0].data[0].scheme.as_deref(), Some("myapp"));
        assert_eq!(parsed.deeplinks[0].data[1].scheme.as_deref(), Some("https"));
        assert_eq!(parsed.deeplinks[0].data[1].host.as_deref(), Some("example.com"));
    }
}
