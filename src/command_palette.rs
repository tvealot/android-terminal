use crate::panel::{Feature, PanelId, PANELS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Quit,
    ToggleHelp,
    PickProject,
    OpenWorkspaces,
    SaveWorkspace,
    RunGradle,
    PickVariant,
    PickDevice,
    LaunchEmulator,
    CycleFocusNext,
    CycleFocusPrev,
    NextScreen,
    PrevScreen,
    EditLayout,
    ToggleZoom,
    TogglePanel(PanelId),
    FocusPanel(PanelId),
}

pub struct CommandEntry {
    pub kind: CommandKind,
    pub label: String,
    pub category: &'static str,
    pub hint: String,
}

pub struct CommandPalette {
    pub query: String,
    pub selected: usize,
    pub commands: Vec<CommandEntry>,
}

impl CommandPalette {
    pub fn new(commands: Vec<CommandEntry>) -> Self {
        Self {
            query: String::new(),
            selected: 0,
            commands,
        }
    }

    pub fn filtered(&self) -> Vec<(usize, i32)> {
        let mut out: Vec<(usize, i32)> = self
            .commands
            .iter()
            .enumerate()
            .filter_map(|(i, c)| score_entry(&self.query, c).map(|s| (i, s)))
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    pub fn move_down(&mut self, len: usize) {
        if len == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected + 1).min(len - 1);
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn current_kind(&self) -> Option<CommandKind> {
        self.filtered()
            .get(self.selected)
            .map(|(i, _)| self.commands[*i].kind)
    }
}

fn score_entry(query: &str, entry: &CommandEntry) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let label = fuzzy_score(query, &entry.label);
    let cat = fuzzy_score(query, entry.category);
    let hint = if entry.hint.eq_ignore_ascii_case(query) {
        Some(200)
    } else {
        None
    };
    [label, cat, hint].into_iter().flatten().max()
}

fn fuzzy_score(query: &str, target: &str) -> Option<i32> {
    let q: Vec<char> = query.to_lowercase().chars().collect();
    let t: Vec<char> = target.to_lowercase().chars().collect();
    if q.is_empty() {
        return Some(0);
    }
    let mut qi = 0usize;
    let mut score = 0i32;
    let mut last_match: Option<usize> = None;
    let mut prev_was_sep = true;
    for (ti, &tc) in t.iter().enumerate() {
        let is_sep = !tc.is_alphanumeric();
        if qi < q.len() && q[qi] == tc {
            score += 10;
            if let Some(lm) = last_match {
                if ti == lm + 1 {
                    score += 6;
                }
            } else if ti == 0 {
                score += 4;
            }
            if prev_was_sep {
                score += 8;
            }
            last_match = Some(ti);
            qi += 1;
        }
        prev_was_sep = is_sep;
    }
    if qi == q.len() {
        Some(score - (t.len() as i32) / 4)
    } else {
        None
    }
}

pub fn build_commands(jvm_available: bool) -> Vec<CommandEntry> {
    let mut v = vec![
        entry(CommandKind::Quit, "Quit", "App", "q"),
        entry(CommandKind::ToggleHelp, "Toggle help overlay", "App", "?"),
        entry(CommandKind::PickProject, "Pick Android project…", "Project", "w"),
        entry(CommandKind::OpenWorkspaces, "Open saved workspaces…", "Project", "W"),
        entry(CommandKind::SaveWorkspace, "Save current workspace", "Project", "S"),
        entry(CommandKind::RunGradle, "Run Gradle default task", "Build", "r"),
        entry(CommandKind::PickVariant, "Pick build variant…", "Build", "V"),
        entry(CommandKind::PickDevice, "Pick device…", "Device", "d"),
        entry(CommandKind::LaunchEmulator, "Launch emulator AVD…", "Device", "e"),
        entry(CommandKind::CycleFocusNext, "Focus next panel", "Layout", "Tab"),
        entry(CommandKind::CycleFocusPrev, "Focus previous panel", "Layout", "Shift+Tab"),
        entry(CommandKind::NextScreen, "Next screen", "Layout", "]"),
        entry(CommandKind::PrevScreen, "Previous screen", "Layout", "["),
        entry(CommandKind::EditLayout, "Edit grid layout", "Layout", "0"),
        entry(CommandKind::ToggleZoom, "Toggle zoom focused panel", "Layout", "z"),
    ];
    for p in PANELS {
        if p.requires == Feature::Jvm && !jvm_available {
            continue;
        }
        v.push(CommandEntry {
            kind: CommandKind::TogglePanel(p.id),
            label: format!("Toggle panel: {}", p.name),
            category: "Panel",
            hint: p.toggle_key.to_string(),
        });
        v.push(CommandEntry {
            kind: CommandKind::FocusPanel(p.id),
            label: format!("Focus panel: {}", p.name),
            category: "Panel",
            hint: p.focus_key.to_string(),
        });
    }
    v
}

fn entry(kind: CommandKind, label: &str, category: &'static str, hint: &str) -> CommandEntry {
    CommandEntry {
        kind,
        label: label.to_string(),
        category,
        hint: hint.to_string(),
    }
}
