use std::collections::VecDeque;

use crate::logcat::LogLine;

const MAX_ISSUES: usize = 200;

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
}

#[derive(Default)]
pub struct IssuesState {
    pub issues: VecDeque<Issue>,
    pub selected: usize,
}

impl IssuesState {
    pub fn detect(&mut self, line: &LogLine) {
        let Some(kind) = classify(line) else {
            return;
        };
        // Coalesce with the last issue if same kind + pid + tag (crash loops).
        if let Some(last) = self.issues.back_mut() {
            if last.kind == kind && last.pid == line.pid && last.tag == line.tag {
                last.count += 1;
                last.timestamp = line.timestamp.clone();
                return;
            }
        }
        if self.issues.len() >= MAX_ISSUES {
            self.issues.pop_front();
            if self.selected > 0 {
                self.selected -= 1;
            }
        }
        self.issues.push_back(Issue {
            kind,
            timestamp: line.timestamp.clone(),
            pid: line.pid,
            tag: line.tag.clone(),
            excerpt: line.message.clone(),
            count: 1,
        });
    }

    pub fn clear(&mut self) {
        self.issues.clear();
        self.selected = 0;
    }

    pub fn move_down(&mut self) {
        if !self.issues.is_empty() {
            self.selected = (self.selected + 1).min(self.issues.len() - 1);
        }
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
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
