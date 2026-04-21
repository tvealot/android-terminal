use std::collections::VecDeque;

const MAX_LINES: usize = 2000;

#[derive(Debug, Clone)]
pub struct LogLine {
    pub timestamp: String,
    pub pid: u32,
    pub level: LogLevel,
    pub tag: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
}

impl Default for LogcatState {
    fn default() -> Self {
        Self {
            lines: VecDeque::new(),
            filter: String::new(),
            min_level: LogLevel::Verbose,
            filter_package: None,
            filter_pid: None,
        }
    }
}

impl LogcatState {
    pub fn push(&mut self, line: LogLine) {
        if self.lines.len() >= MAX_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    pub fn clear_package_filter(&mut self) {
        self.filter_package = None;
        self.filter_pid = None;
    }

    pub fn visible<'a>(&'a self) -> Box<dyn Iterator<Item = &'a LogLine> + 'a> {
        let min = self.min_level;
        let needle = if self.filter.is_empty() { None } else { Some(self.filter.to_lowercase()) };
        let pid = self.filter_pid;
        Box::new(self.lines.iter().filter(move |l| {
            if l.level < min {
                return false;
            }
            if let Some(p) = pid {
                if l.pid != p {
                    return false;
                }
            }
            if let Some(n) = &needle {
                if !l.tag.to_lowercase().contains(n) && !l.message.to_lowercase().contains(n) {
                    return false;
                }
            }
            true
        }))
    }
}
