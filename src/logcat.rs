use std::collections::VecDeque;

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

const MAX_LINES: usize = 2000;

#[derive(Debug, Clone)]
pub struct LogLine {
    pub timestamp: String,
    pub pid: u32,
    pub level: LogLevel,
    pub tag: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogLevel {
    Verbose,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    pub fn from_char(c: char) -> Self {
        match c {
            'V' => Self::Verbose,
            'D' => Self::Debug,
            'I' => Self::Info,
            'W' => Self::Warn,
            'E' => Self::Error,
            'F' => Self::Fatal,
            _ => Self::Info,
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Self::Verbose => "V",
            Self::Debug => "D",
            Self::Info => "I",
            Self::Warn => "W",
            Self::Error => "E",
            Self::Fatal => "F",
        }
    }

    pub fn next_cycle(self) -> Self {
        match self {
            Self::Verbose => Self::Debug,
            Self::Debug => Self::Info,
            Self::Info => Self::Warn,
            Self::Warn => Self::Error,
            Self::Error => Self::Verbose,
            Self::Fatal => Self::Verbose,
        }
    }
}

impl Default for LogLevel {
    fn default() -> Self {
        Self::Verbose
    }
}

impl LogLine {
    // threadtime format: "MM-DD HH:MM:SS.sss  PID  TID L TAG: message"
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim_end();
        if trimmed.is_empty() || trimmed.starts_with("---------") {
            return None;
        }
        let mut parts = trimmed.splitn(7, ' ').filter(|s| !s.is_empty());
        let date = parts.next()?;
        let time = parts.next()?;
        let pid_str = parts.next()?;
        let _tid = parts.next()?;
        let level = parts.next()?;
        let tag = parts.next()?;
        let message = parts.next().unwrap_or("").trim_start_matches(':').trim_start();
        Some(Self {
            timestamp: format!("{} {}", date, time),
            pid: pid_str.parse().unwrap_or(0),
            level: LogLevel::from_char(level.chars().next().unwrap_or('I')),
            tag: tag.trim_end_matches(':').to_string(),
            message: message.to_string(),
        })
    }
}

pub struct LogcatState {
    pub lines: VecDeque<LogLine>,
    pub filter: String,
    pub min_level: LogLevel,
    pub filter_package: Option<String>,
    pub filter_pid: Option<u32>,
    pub paused: bool,
    pub use_regex: bool,
    pub regex_error: Option<String>,
    pub scroll: usize, // 0 = follow tail
    compiled: Option<Regex>,
}

impl Default for LogcatState {
    fn default() -> Self {
        Self {
            lines: VecDeque::new(),
            filter: String::new(),
            min_level: LogLevel::Verbose,
            filter_package: None,
            filter_pid: None,
            paused: false,
            use_regex: false,
            regex_error: None,
            scroll: 0,
            compiled: None,
        }
    }
}

impl LogcatState {
    pub fn push(&mut self, line: LogLine) {
        if self.paused {
            return;
        }
        let passes = self.matches(&line);
        if self.lines.len() >= MAX_LINES {
            let dropped = self.lines.pop_front();
            if self.scroll > 0 {
                if let Some(d) = dropped {
                    if self.matches(&d) {
                        self.scroll = self.scroll.saturating_sub(1);
                    }
                }
            }
        }
        self.lines.push_back(line);
        if self.scroll > 0 && passes {
            self.scroll = self.scroll.saturating_add(1);
        }
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.scroll = 0;
    }

    pub fn clear_package_filter(&mut self) {
        self.filter_package = None;
        self.filter_pid = None;
    }

    pub fn recompile(&mut self) {
        self.regex_error = None;
        self.compiled = None;
        if !self.use_regex || self.filter.is_empty() {
            return;
        }
        match RegexBuilder::new(&self.filter).case_insensitive(true).build() {
            Ok(r) => self.compiled = Some(r),
            Err(e) => self.regex_error = Some(e.to_string()),
        }
    }

    pub fn toggle_regex(&mut self) {
        self.use_regex = !self.use_regex;
        self.recompile();
    }

    pub fn matches(&self, line: &LogLine) -> bool {
        if line.level < self.min_level {
            return false;
        }
        if let Some(p) = self.filter_pid {
            if line.pid != p {
                return false;
            }
        }
        if self.filter.is_empty() {
            return true;
        }
        if self.use_regex {
            match &self.compiled {
                Some(r) => r.is_match(&line.tag) || r.is_match(&line.message),
                None => true, // invalid regex → show all, error shown in header
            }
        } else {
            let needle = self.filter.to_lowercase();
            line.tag.to_lowercase().contains(&needle)
                || line.message.to_lowercase().contains(&needle)
        }
    }

    pub fn visible<'a>(&'a self) -> Vec<&'a LogLine> {
        self.lines.iter().filter(|l| self.matches(l)).collect()
    }

    pub fn match_spans(&self, haystack: &str) -> Vec<(usize, usize)> {
        if self.filter.is_empty() {
            return Vec::new();
        }
        if self.use_regex {
            match &self.compiled {
                Some(r) => r.find_iter(haystack).map(|m| (m.start(), m.end())).collect(),
                None => Vec::new(),
            }
        } else {
            let needle = self.filter.to_lowercase();
            let hay = haystack.to_lowercase();
            let mut out = Vec::new();
            let mut i = 0;
            while let Some(pos) = hay[i..].find(&needle) {
                let start = i + pos;
                let end = start + needle.len();
                out.push((start, end));
                i = end;
            }
            out
        }
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_add(n);
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll = 0;
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll = usize::MAX;
    }
}
