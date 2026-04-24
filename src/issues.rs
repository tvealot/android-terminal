use std::collections::VecDeque;

use crate::logcat::LogLine;

const MAX_ISSUES: usize = 200;
const CAPTURE_LINES: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    Crash,
    Anr,
    Tombstone,
}

impl IssueKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Crash => "CRASH",
            Self::Anr => "ANR",
            Self::Tombstone => "NATIVE",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub kind: IssueKind,
    pub timestamp: String,
    pub pid: u32,
    pub tag: String,
    pub excerpt: String,
    pub count: u32,
    pub buffer: Vec<String>,
}

struct Capture {
    issue_idx: usize,
    pid: u32,
    remaining: usize,
}

#[derive(Default)]
pub struct IssuesState {
    pub issues: VecDeque<Issue>,
    pub selected: usize,
    pub expanded: Option<usize>,
    pub detail_scroll: u16,
    capture: Option<Capture>,
}

impl IssuesState {
    pub fn detect(&mut self, line: &LogLine) {
        if let Some(kind) = classify(line) {
            if let Some(last) = self.issues.back_mut() {
                if last.kind == kind && last.pid == line.pid && last.tag == line.tag {
                    last.count += 1;
                    last.timestamp = line.timestamp.clone();
                    self.start_capture_for_last(line);
                    return;
                }
            }
            if self.issues.len() >= MAX_ISSUES {
                self.issues.pop_front();
                if self.selected > 0 {
                    self.selected -= 1;
                }
                if let Some(cap) = self.capture.as_mut() {
                    if cap.issue_idx == 0 {
                        self.capture = None;
                    } else {
                        cap.issue_idx -= 1;
                    }
                }
                if let Some(idx) = self.expanded {
                    self.expanded = if idx == 0 { None } else { Some(idx - 1) };
                }
            }
            self.issues.push_back(Issue {
                kind,
                timestamp: line.timestamp.clone(),
                pid: line.pid,
                tag: line.tag.clone(),
                excerpt: line.message.clone(),
                count: 1,
                buffer: Vec::new(),
            });
            self.start_capture_for_last(line);
            return;
        }
        self.capture_follow(line);
    }

    fn start_capture_for_last(&mut self, line: &LogLine) {
        let idx = self.issues.len().saturating_sub(1);
        if let Some(issue) = self.issues.get_mut(idx) {
            if issue.buffer.len() < CAPTURE_LINES {
                issue.buffer.push(format_line(line));
            }
        }
        self.capture = Some(Capture {
            issue_idx: idx,
            pid: line.pid,
            remaining: CAPTURE_LINES.saturating_sub(1),
        });
    }

    fn capture_follow(&mut self, line: &LogLine) {
        let Some(cap) = self.capture.as_mut() else { return };
        if line.pid != cap.pid {
            return;
        }
        if let Some(issue) = self.issues.get_mut(cap.issue_idx) {
            issue.buffer.push(format_line(line));
        }
        cap.remaining = cap.remaining.saturating_sub(1);
        if cap.remaining == 0 {
            self.capture = None;
        }
    }

    pub fn clear(&mut self) {
        self.issues.clear();
        self.selected = 0;
        self.expanded = None;
        self.detail_scroll = 0;
        self.capture = None;
    }

    pub fn move_down(&mut self) {
        if self.issues.is_empty() {
            return;
        }
        if let Some(idx) = self.expanded {
            let max = self
                .issues
                .get(idx)
                .map(|i| i.buffer.len().saturating_sub(1) as u16)
                .unwrap_or(0);
            self.detail_scroll = (self.detail_scroll + 1).min(max);
        } else {
            self.selected = (self.selected + 1).min(self.issues.len() - 1);
        }
    }

    pub fn move_up(&mut self) {
        if self.expanded.is_some() {
            self.detail_scroll = self.detail_scroll.saturating_sub(1);
        } else {
            self.selected = self.selected.saturating_sub(1);
        }
    }

    pub fn toggle_expand(&mut self) {
        if self.expanded.is_some() {
            self.expanded = None;
            self.detail_scroll = 0;
        } else if !self.issues.is_empty() {
            self.expanded = Some(self.selected);
            self.detail_scroll = 0;
        }
    }

    pub fn close_detail(&mut self) {
        self.expanded = None;
        self.detail_scroll = 0;
    }

    pub fn selected_stacktrace(&self) -> Option<String> {
        let idx = self.expanded.unwrap_or(self.selected);
        let issue = self.issues.get(idx)?;
        if issue.buffer.is_empty() {
            return None;
        }
        let header = format!(
            "{} · pid={} · {} · {}",
            issue.kind.label(),
            issue.pid,
            issue.tag,
            issue.timestamp
        );
        let mut out = String::with_capacity(header.len() + 2 + issue.buffer.iter().map(|s| s.len() + 1).sum::<usize>());
        out.push_str(&header);
        out.push('\n');
        for line in &issue.buffer {
            out.push_str(line);
            out.push('\n');
        }
        Some(out)
    }
}

fn format_line(line: &LogLine) -> String {
    format!(
        "{} {} {} {}: {}",
        line.timestamp,
        line.pid,
        line.level.short(),
        line.tag,
        line.message
    )
}

fn classify(line: &LogLine) -> Option<IssueKind> {
    let msg = &line.message;
    if msg.starts_with("FATAL EXCEPTION") || line.tag == "AndroidRuntime" && msg.contains("FATAL EXCEPTION") {
        return Some(IssueKind::Crash);
    }
    if msg.starts_with("ANR in ") || line.tag == "ActivityManager" && msg.starts_with("ANR in ") {
        return Some(IssueKind::Anr);
    }
    if line.tag == "tombstoned" || line.tag == "DEBUG" && msg.contains("*** *** ***") {
        return Some(IssueKind::Tombstone);
    }
    None
}
