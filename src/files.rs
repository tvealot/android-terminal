use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyEvent};

const MAX_PREVIEW_BYTES: u64 = 64 * 1024;

#[derive(Clone)]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub expanded: bool,
    pub children: Option<Vec<FileNode>>,
}

#[derive(Clone)]
pub struct FlatEntry {
    pub depth: usize,
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub expanded: bool,
}

pub struct FileMeta {
    pub size_bytes: u64,
    pub modified: Option<String>,
}

pub enum DetailKind {
    Text { content: String },
    Binary { reason: String },
    TooLarge { size_bytes: u64 },
}

pub struct FilesState {
    pub root: Option<PathBuf>,
    pub root_children: Option<Vec<FileNode>>,
    pub selected_index: usize,
    pub error: Option<String>,
    pub detail_open: bool,
    pub detail_focused: bool,
    pub detail_scroll: usize,
    pub selected_file: Option<PathBuf>,
    pub selected_meta: Option<FileMeta>,
    pub selected_kind: Option<DetailKind>,
    pub detail_error: Option<String>,
}

impl FilesState {
    pub fn new(root: Option<PathBuf>) -> Self {
        let mut state = Self {
            root: None,
            root_children: None,
            selected_index: 0,
            error: None,
            detail_open: false,
            detail_focused: false,
            detail_scroll: 0,
            selected_file: None,
            selected_meta: None,
            selected_kind: None,
            detail_error: None,
        };
        state.set_root(root);
        state
    }

    pub fn set_root(&mut self, root: Option<PathBuf>) {
        if self.root == root {
            return;
        }
        self.root = root;
        self.selected_index = 0;
        self.close_detail();
        self.refresh();
    }

    pub fn refresh(&mut self) {
        self.error = None;
        self.selected_index = 0;
        self.root_children = None;

        let Some(root) = &self.root else {
            return;
        };

        match read_children(root) {
            Ok(children) => self.root_children = Some(children),
            Err(err) => self.error = Some(format!("files: {}", err)),
        }

        if let Some(path) = self.selected_file.clone() {
            if path.exists() {
                self.load_detail(&path);
            } else {
                self.close_detail();
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.detail_open && self.detail_focused {
            return match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.detail_scroll = self.detail_scroll.saturating_sub(1);
                    true
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.detail_scroll = self.detail_scroll.saturating_add(1);
                    true
                }
                KeyCode::Char(' ') => {
                    self.detail_scroll = self.detail_scroll.saturating_add(12);
                    true
                }
                KeyCode::Tab => {
                    self.detail_focused = false;
                    true
                }
                KeyCode::Backspace => {
                    self.close_detail();
                    true
                }
                _ => false,
            };
        }

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up();
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down();
                true
            }
            KeyCode::Enter | KeyCode::Right => {
                self.open_or_toggle_selected();
                true
            }
            KeyCode::Left => {
                self.collapse_selected();
                true
            }
            KeyCode::Backspace => {
                if self.detail_open {
                    self.close_detail();
                } else {
                    self.collapse_selected();
                }
                true
            }
            KeyCode::Tab if self.detail_open => {
                self.detail_focused = true;
                true
            }
            KeyCode::Char('r') => {
                self.refresh();
                true
            }
            _ => false,
        }
    }

    pub fn flatten_visible(&self) -> Vec<FlatEntry> {
        let mut out = Vec::new();
        if let Some(children) = &self.root_children {
            for child in children {
                flatten_node(child, 0, &mut out);
            }
        }
        out
    }

    pub fn root_label(&self) -> Option<String> {
        self.root.as_ref().map(|path| path.display().to_string())
    }

    pub fn selected_label(&self) -> String {
        self.selected_file
            .as_ref()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "detail".to_string())
    }

    fn move_up(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    fn move_down(&mut self) {
        let total = self.flatten_visible().len();
        if total > 0 {
            self.selected_index = (self.selected_index + 1).min(total - 1);
        }
    }

    fn open_or_toggle_selected(&mut self) {
        let flat = self.flatten_visible();
        let Some(entry) = flat.get(self.selected_index).cloned() else {
            return;
        };
        if entry.is_dir {
            self.toggle_dir(&entry.path);
        } else {
            self.detail_open = true;
            self.detail_focused = false;
            self.load_detail(&entry.path);
        }
    }

    fn toggle_dir(&mut self, path: &Path) {
        let Some(children) = self.root_children.as_mut() else {
            return;
        };
        let Some(node) = find_node_mut(children, path) else {
            return;
        };

        if node.expanded {
            node.expanded = false;
            return;
        }

        match read_children(&node.path) {
            Ok(children) => {
                node.children = Some(children);
                node.expanded = true;
                self.error = None;
            }
            Err(err) => self.error = Some(format!("files: {}", err)),
        }
    }

    fn collapse_selected(&mut self) {
        let flat = self.flatten_visible();
        let Some(entry) = flat.get(self.selected_index).cloned() else {
            return;
        };

        if entry.is_dir && entry.expanded {
            if let Some(children) = self.root_children.as_mut() {
                if let Some(node) = find_node_mut(children, &entry.path) {
                    node.expanded = false;
                }
            }
            return;
        }

        if entry.depth == 0 {
            return;
        }

        let target_depth = entry.depth - 1;
        for index in (0..self.selected_index).rev() {
            if flat[index].depth == target_depth {
                self.selected_index = index;
                break;
            }
        }
    }

    fn load_detail(&mut self, path: &Path) {
        self.selected_file = Some(path.to_path_buf());
        self.selected_meta = None;
        self.selected_kind = None;
        self.detail_error = None;
        self.detail_scroll = 0;

        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(err) => {
                self.detail_error = Some(format!("files: {}", err));
                return;
            }
        };

        self.selected_meta = Some(FileMeta {
            size_bytes: metadata.len(),
            modified: metadata.modified().ok().map(format_time),
        });

        if metadata.len() > MAX_PREVIEW_BYTES {
            self.selected_kind = Some(DetailKind::TooLarge {
                size_bytes: metadata.len(),
            });
            return;
        }

        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.detail_error = Some(format!("files: {}", err));
                return;
            }
        };

        if bytes.contains(&0) {
            self.selected_kind = Some(DetailKind::Binary {
                reason: "binary file".to_string(),
            });
            return;
        }

        self.selected_kind = Some(DetailKind::Text {
            content: String::from_utf8_lossy(&bytes).into_owned(),
        });
    }

    fn close_detail(&mut self) {
        self.detail_open = false;
        self.detail_focused = false;
        self.detail_scroll = 0;
        self.selected_file = None;
        self.selected_meta = None;
        self.selected_kind = None;
        self.detail_error = None;
    }
}

fn flatten_node(node: &FileNode, depth: usize, out: &mut Vec<FlatEntry>) {
    out.push(FlatEntry {
        depth,
        name: node.name.clone(),
        path: node.path.clone(),
        is_dir: node.is_dir,
        expanded: node.expanded,
    });

    if node.expanded {
        if let Some(children) = &node.children {
            for child in children {
                flatten_node(child, depth + 1, out);
            }
        }
    }
}

fn find_node_mut<'a>(nodes: &'a mut [FileNode], target: &Path) -> Option<&'a mut FileNode> {
    for node in nodes {
        if node.path == target {
            return Some(node);
        }
        if let Some(children) = node.children.as_mut() {
            if let Some(found) = find_node_mut(children, target) {
                return Some(found);
            }
        }
    }
    None
}

fn read_children(path: &Path) -> std::io::Result<Vec<FileNode>> {
    let mut children = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        children.push(FileNode {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: entry.path(),
            is_dir: metadata.is_dir(),
            expanded: false,
            children: None,
        });
    }

    children.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    Ok(children)
}

fn format_time(time: std::time::SystemTime) -> String {
    DateTime::<Local>::from(time)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}
