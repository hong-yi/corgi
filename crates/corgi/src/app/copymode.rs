//! Copy mode in the app: entering it on the focused pane, its keys, the paste buffer, the
//! clipboard, and drawing the cursor and selection over the pane. The motions and selection
//! are `crate::copy`.

use super::*;
use crate::copy::{Copy, Outcome};
use ratatui::style::{Modifier, Style};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

impl App {
    /// `C-b [`: copy mode on the focused pane, the cursor where the pane's is; `up`: a screen up.
    pub(super) fn start_copy(&mut self, up: bool) {
        let Some(w) = self.window() else { return };
        let pane = w.focus.clone();
        let Some(r) = self.rects().into_iter().find(|(id, _)| *id == pane).map(|(_, r)| r) else {
            return;
        };
        let Some(p) = self.panes.get_mut(&pane) else { return };
        let lines = p.vt.screen_lines();
        let (_, top, _) = p.vt.scrollbar();
        let (cx, cy) =
            p.vt.frame()
                .ok()
                .and_then(|f| f.cursor.as_ref().map(|c| (c.x, c.y)))
                .unwrap_or((0, 0));
        let row = top + cy as usize;
        let col = char_at_column(lines.get(row).map_or("", String::as_str), cx as usize);
        let mut c = Copy::new(lines, r.height as usize, top, row, col);
        c.revision = p.vt.revision();
        if up {
            c.page(-(r.height as isize));
        }
        p.vt.scroll_to(c.top);
        self.mode = Mode::Copy(Box::new(c), pane);
    }

    pub(super) fn copy_key(&mut self, mut c: Box<Copy>, pane: String, k: KeyEvent) {
        // The prefix still works: copy mode ends, the prefix starts.
        if self.keymap.prefix.matches(&k) && c.typing.is_none() {
            self.leave_copy(&pane);
            self.mode = Mode::Prefix(Instant::now());
            return;
        }
        let Some(p) = self.panes.get_mut(&pane) else { return };
        // New output since (only then: reading it all is the costly part): the rows again, where
        // you are staying put.
        if p.vt.revision() != c.revision {
            c.refresh(p.vt.screen_lines());
            c.revision = p.vt.revision();
        }
        match c.key(&k) {
            Outcome::Stay => {
                p.vt.scroll_to(c.top);
                self.mode = Mode::Copy(c, pane);
            }
            Outcome::Exit => self.leave_copy(&pane),
            Outcome::Yank(text) => self.yank(&pane, text),
        }
    }

    /// Copied: to your terminal's clipboard (OSC 52) and the paste buffer; copy mode ends.
    fn yank(&mut self, pane: &str, text: String) {
        self.leave_copy(pane);
        let lines = text.lines().count().max(1);
        let chars = text.chars().count();
        crate::term::host_write(&crate::host_notify::clipboard(
            &text,
            std::env::var_os("TMUX").is_some(),
        ));
        self.paste = Some(text);
        self.notify(match lines {
            1 => format!("copied {chars} characters"),
            n => format!("copied {n} lines"),
        });
    }

    /// The mouse in a pane: press and drag selects (copy mode starts), letting go copies; a
    /// double click copies the word, a triple click the line. True if it was that.
    pub(super) fn copy_mouse(&mut self, m: crossterm::event::MouseEvent) -> bool {
        let at = ratatui::layout::Position::new(m.column, m.row);
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let Some((pane, r)) = self.rects().into_iter().find(|(_, r)| r.contains(at)) else {
                    self.press = None;
                    return false;
                };
                let now = Instant::now();
                let clicks = match self.last_click {
                    Some((t, p, n)) if now.duration_since(t) < Duration::from_millis(400) && p == (m.column, m.row) => {
                        n % 3 + 1
                    }
                    _ => 1,
                };
                self.last_click = Some((now, (m.column, m.row), clicks));
                self.press = Some(Press {
                    pane: pane.clone(),
                    at: (m.column, m.row),
                    moved: false,
                });
                if let Mode::Copy(c, p) = &mut self.mode
                    && *p == pane
                {
                    // In copy mode already: the cursor goes there, a new selection can start.
                    let (row, col) = spot(c, r, at);
                    c.point(row, col);
                    c.anchor = None;
                } else if clicks == 1 {
                    // Not handled here: the click focuses the pane as usual.
                    return false;
                }
                if clicks > 1 {
                    self.focus_pane(&pane);
                    if !matches!(&self.mode, Mode::Copy(_, p) if *p == pane) {
                        self.start_copy(false);
                    }
                    if let Mode::Copy(c, p) = &mut self.mode {
                        let (row, col) = spot(c, r, at);
                        c.point(row, col);
                        if clicks == 2 {
                            c.select_word();
                        } else {
                            c.select_line();
                        }
                        let (text, p) = (c.selected(), p.clone());
                        self.press = None;
                        if let Some(text) = text.filter(|t| !t.is_empty()) {
                            self.yank(&p, text);
                        } else {
                            self.leave_copy(&p);
                        }
                    }
                }
                true
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(press) = self.press.clone() else { return false };
                let Some(r) = self
                    .rects()
                    .into_iter()
                    .find(|(id, _)| *id == press.pane)
                    .map(|(_, r)| r)
                else {
                    return false;
                };
                if !press.moved {
                    if press.at == (m.column, m.row) {
                        return true;
                    }
                    self.focus_pane(&press.pane);
                    if !matches!(&self.mode, Mode::Copy(_, p) if *p == press.pane) {
                        self.start_copy(false);
                    }
                    if let Mode::Copy(c, _) = &mut self.mode {
                        let (row, col) = spot(c, r, ratatui::layout::Position::new(press.at.0, press.at.1));
                        c.press(row, col);
                    }
                    if let Some(p) = &mut self.press {
                        p.moved = true;
                    }
                }
                if let Mode::Copy(c, pane) = &mut self.mode {
                    // Past the pane's top or bottom: it scrolls.
                    if m.row < r.y {
                        c.page(-1);
                    } else if m.row >= r.bottom() {
                        c.page(1);
                    }
                    let y = m.row.clamp(r.y, r.bottom().saturating_sub(1));
                    let (row, col) = spot(c, r, ratatui::layout::Position::new(m.column, y));
                    c.point(row, col);
                    let top = c.top;
                    if let Some(p) = self.panes.get_mut(pane.as_str()) {
                        p.vt.scroll_to(top);
                    }
                }
                true
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let Some(press) = self.press.take() else { return false };
                if !press.moved {
                    return false;
                }
                if let Mode::Copy(c, pane) = &self.mode
                    && let Some(text) = c.selected().filter(|t| !t.is_empty())
                {
                    let pane = pane.clone();
                    self.yank(&pane, text);
                }
                true
            }
            _ => false,
        }
    }

    /// Give a pane the keys (a click on it).
    pub(super) fn focus_pane(&mut self, pane: &str) {
        if let Some(w) = self.window_mut()
            && w.focus != pane
        {
            w.set_focus(pane);
            self.dirty = true;
        }
        self.sync();
    }

    fn leave_copy(&mut self, pane: &str) {
        if let Some(p) = self.panes.get_mut(pane) {
            p.vt.scroll_bottom();
        }
        self.mode = Mode::Normal;
    }

    /// `C-b ]`: what copy mode copied last, pasted into the focused pane.
    pub(super) fn paste_buffer(&mut self) {
        let Some(text) = self.paste.clone() else {
            self.notify("nothing copied yet (C-b [ to copy)".into());
            return;
        };
        if let Some(p) = self.focused_pane() {
            let bytes = p.vt.encode_paste(&text);
            p.input(&bytes);
        }
    }

    /// The pane in copy mode and where its view starts, so drawing shows it from there.
    pub(super) fn copy_view(&self) -> Option<(String, usize)> {
        match &self.mode {
            Mode::Copy(c, pane) => Some((pane.clone(), c.top)),
            _ => None,
        }
    }

    /// Over the pane in copy mode: the selection, the cursor, where you are, the search.
    pub(super) fn draw_copy(&self, f: &mut ratatui::Frame, rects: &[(String, Rect)]) {
        let Mode::Copy(c, pane) = &self.mode else { return };
        let Some(r) = rects.iter().find(|(id, _)| id == pane).map(|(_, r)| *r) else {
            return;
        };
        let t = &self.theme;
        let buf = f.buffer_mut();
        let selected = Style::new().bg(t.accent).fg(t.accent_fg);
        for y in 0..r.height {
            let row = c.top + y as usize;
            let Some(line) = c.lines.get(row) else { break };
            let mut x = 0u16;
            for (col, ch) in line.chars().enumerate() {
                let w = ch.width().unwrap_or(0) as u16;
                if c.is_selected(row, col) && x < r.width {
                    for k in 0..w.max(1) {
                        if let Some(cell) = buf.cell_mut((r.x + x + k, r.y + y)) {
                            cell.set_style(selected);
                        }
                    }
                }
                x += w;
            }
            // A selected empty line (lines mode, or between selected rows) still shows.
            if line.is_empty()
                && c.is_selected(row, 0)
                && let Some(cell) = buf.cell_mut((r.x, r.y + y))
            {
                cell.set_style(selected);
            }
        }
        // The cursor: on its cell, drawn so it shows whatever the pane's colours.
        let line = c.lines.get(c.row).map_or("", String::as_str);
        let cx = line
            .chars()
            .take(c.col)
            .map(|ch| ch.width().unwrap_or(0))
            .sum::<usize>() as u16;
        let cy = (c.row - c.top) as u16;
        if cx < r.width && cy < r.height {
            if let Some(cell) = buf.cell_mut((r.x + cx, r.y + cy)) {
                cell.set_style(Style::new().add_modifier(Modifier::REVERSED));
            }
            f.set_cursor_position((r.x + cx, r.y + cy));
        }
        let buf = f.buffer_mut();
        // Where you are, top right; the search being typed or the last word on it, bottom.
        let pos = c.position();
        let pw = pos.width() as u16;
        if pw < r.width {
            buf.set_stringn(
                r.right() - pw,
                r.y,
                &pos,
                pw as usize,
                Style::new().bg(t.mode_prefix).fg(t.accent_fg),
            );
        }
        let bottom = match (&c.typing, &c.note) {
            (Some((text, down)), _) => Some(format!("{}{text}", if *down { "/" } else { "?" })),
            (None, Some(n)) => Some(n.clone()),
            _ => None,
        };
        if let Some(text) = bottom {
            let row = Rect::new(r.x, r.bottom().saturating_sub(1), r.width, 1);
            buf.set_style(row, Style::new().bg(t.surface).fg(t.fg));
            buf.set_stringn(
                row.x,
                row.y,
                &text,
                row.width as usize,
                Style::new().bg(t.surface).fg(t.fg),
            );
            if c.typing.is_some() {
                let x = (text.width() as u16).min(r.width.saturating_sub(1));
                f.set_cursor_position((row.x + x, row.y));
            }
        }
    }
}

/// The mouse pressed in a pane, maybe to select.
#[derive(Clone, Debug)]
pub(super) struct Press {
    pub pane: String,
    pub at: (u16, u16),
    /// Dragged since: a selection.
    pub moved: bool,
}

/// Where a point on screen is in copy mode's text: (row, char).
fn spot(c: &Copy, r: Rect, at: ratatui::layout::Position) -> (usize, usize) {
    let row = (c.top + at.y.saturating_sub(r.y) as usize).min(c.lines.len().saturating_sub(1));
    let col = char_at_column(
        c.lines.get(row).map_or("", String::as_str),
        at.x.saturating_sub(r.x) as usize,
    );
    (row, col)
}

/// The char index at display column `x` of `line` (wide characters take two columns).
fn char_at_column(line: &str, x: usize) -> usize {
    let mut at = 0;
    for (i, ch) in line.chars().enumerate() {
        let w = ch.width().unwrap_or(0);
        if at + w > x {
            return i;
        }
        at += w;
    }
    line.chars().count()
}

#[cfg(test)]
mod tests {
    #[test]
    fn columns_to_chars() {
        assert_eq!(super::char_at_column("hello", 3), 3);
        assert_eq!(super::char_at_column("日本語", 2), 1, "wide characters");
        assert_eq!(super::char_at_column("ab", 9), 2);
    }
}
