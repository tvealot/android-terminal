use serde::{Deserialize, Serialize};

use crate::panel::PanelId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutCell {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub panel: PanelId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutGrid {
    pub cols: u16,
    pub rows: u16,
    pub cells: Vec<LayoutCell>,
}

impl Default for LayoutGrid {
    fn default() -> Self {
        Self {
            cols: 12,
            rows: 12,
            cells: Vec::new(),
        }
    }
}

impl LayoutGrid {
    pub fn cell_at(&self, x: u16, y: u16) -> Option<usize> {
        self.cells
            .iter()
            .position(|c| x >= c.x && x < c.x + c.w && y >= c.y && y < c.y + c.h)
    }

    pub fn remove_at(&mut self, x: u16, y: u16) -> bool {
        if let Some(idx) = self.cell_at(x, y) {
            self.cells.remove(idx);
            return true;
        }
        false
    }

    pub fn prune_out_of_bounds(&mut self) {
        self.cells
            .retain(|c| c.x + c.w <= self.cols && c.y + c.h <= self.rows);
    }

    pub fn visible_panels(&self) -> Vec<PanelId> {
        self.cells.iter().map(|c| c.panel).collect()
    }
}

pub struct LayoutEditor {
    pub grid: LayoutGrid,
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub sel_start: Option<(u16, u16)>,
    pub message: Option<String>,
}

impl LayoutEditor {
    pub fn new(grid: LayoutGrid) -> Self {
        Self {
            grid,
            cursor_x: 0,
            cursor_y: 0,
            sel_start: None,
            message: None,
        }
    }

    pub fn selection_rect(&self) -> (u16, u16, u16, u16) {
        let (sx, sy) = self.sel_start.unwrap_or((self.cursor_x, self.cursor_y));
        let x0 = sx.min(self.cursor_x);
        let y0 = sy.min(self.cursor_y);
        let x1 = sx.max(self.cursor_x);
        let y1 = sy.max(self.cursor_y);
        (x0, y0, x1 - x0 + 1, y1 - y0 + 1)
    }

    pub fn clamp_cursor(&mut self) {
        if self.cursor_x >= self.grid.cols {
            self.cursor_x = self.grid.cols.saturating_sub(1);
        }
        if self.cursor_y >= self.grid.rows {
            self.cursor_y = self.grid.rows.saturating_sub(1);
        }
    }

    pub fn move_cursor(&mut self, dx: i32, dy: i32) {
        let nx = (self.cursor_x as i32 + dx).clamp(0, self.grid.cols as i32 - 1);
        let ny = (self.cursor_y as i32 + dy).clamp(0, self.grid.rows as i32 - 1);
        self.cursor_x = nx as u16;
        self.cursor_y = ny as u16;
    }

    pub fn toggle_selection(&mut self) {
        if self.sel_start.is_some() {
            self.sel_start = None;
        } else {
            self.sel_start = Some((self.cursor_x, self.cursor_y));
        }
    }

    pub fn assign(&mut self, panel: PanelId) {
        let (x, y, w, h) = self.selection_rect();
        self.grid.cells.retain(|c| {
            let overlap = !(x + w <= c.x || c.x + c.w <= x || y + h <= c.y || c.y + c.h <= y);
            !overlap
        });
        self.grid.cells.push(LayoutCell { x, y, w, h, panel });
        self.sel_start = None;
        self.message = Some(format!("assigned {:?}", panel));
    }

    pub fn delete_at_cursor(&mut self) {
        let cx = self.cursor_x;
        let cy = self.cursor_y;
        if self.grid.remove_at(cx, cy) {
            self.message = Some("deleted cell".to_string());
        }
    }

    pub fn resize_cols(&mut self, delta: i32) {
        let n = (self.grid.cols as i32 + delta).clamp(1, 48) as u16;
        self.grid.cols = n;
        self.grid.prune_out_of_bounds();
        self.clamp_cursor();
    }

    pub fn resize_rows(&mut self, delta: i32) {
        let n = (self.grid.rows as i32 + delta).clamp(1, 48) as u16;
        self.grid.rows = n;
        self.grid.prune_out_of_bounds();
        self.clamp_cursor();
    }

    pub fn clear(&mut self) {
        self.grid.cells.clear();
        self.sel_start = None;
        self.message = Some("cleared".to_string());
    }
}

pub fn cell_rect(area: ratatui::layout::Rect, grid: &LayoutGrid, cx: u16, cy: u16, cw: u16, ch: u16) -> ratatui::layout::Rect {
    let x0 = (area.width as u32 * cx as u32 / grid.cols as u32) as u16;
    let y0 = (area.height as u32 * cy as u32 / grid.rows as u32) as u16;
    let x1 = (area.width as u32 * (cx + cw) as u32 / grid.cols as u32) as u16;
    let y1 = (area.height as u32 * (cy + ch) as u32 / grid.rows as u32) as u16;
    ratatui::layout::Rect {
        x: area.x + x0,
        y: area.y + y0,
        width: x1.saturating_sub(x0),
        height: y1.saturating_sub(y0),
    }
}
