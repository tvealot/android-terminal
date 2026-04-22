use std::collections::VecDeque;

const MAX_LINES: usize = 2000;

#[derive(Debug, Clone)]
pub struct LogLine {
    pub timestamp: String,
    pub level: LogLevel,
    pub tag: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        let _pid = parts.next()?;
        let _tid = parts.next()?;
        let level = parts.next()?;
        let tag = parts.next()?;
        let message = parts
            .next()
            .unwrap_or("")
            .trim_start_matches(':')
            .trim_start();
        Some(Self {
            timestamp: format!("{} {}", date, time),
            level: LogLevel::from_char(level.chars().next().unwrap_or('I')),
            tag: tag.trim_end_matches(':').to_string(),
            message: message.to_string(),
        })
    }
}

#[derive(Default)]
pub struct LogcatState {
    pub lines: VecDeque<LogLine>,
    pub filter: String,
}

impl LogcatState {
    pub fn push(&mut self, line: LogLine) {
        if self.lines.len() >= MAX_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    pub fn visible<'a>(&'a self) -> Box<dyn Iterator<Item = &'a LogLine> + 'a> {
        if self.filter.is_empty() {
            Box::new(self.lines.iter())
        } else {
            let needle = self.filter.to_lowercase();
            Box::new(self.lines.iter().filter(move |l| {
                l.tag.to_lowercase().contains(&needle) || l.message.to_lowercase().contains(&needle)
            }))
        }
    }
}
