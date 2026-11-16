//! The tree chooser dialog (C-b w, C-b s): the sidebar's tree in a modal,
//! with a cursor, folding and type-to-filter, and a last row that makes a new
//! workspace (named after the filter, when it matches nothing). Key handling is pure;
//! the rows come from `tree::rows` each time, so folding here folds the sidebar too.

use crate::config::{Indicators, Sidebar};
use crate::sidebar::tree_line;
use crate::theme::Theme;
use crate::tree::{Fold, Row, Target};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Widget};

#[derive(Clone, Debug, Default)]
pub struct Chooser {
    pub filter: String,
    /// The row under the cursor, kept by identity so it survives folding and filtering.
    pub at: Option<Target>,
    /// First row shown (scrolling).
    top: usize,
    /// Sidebar mode: `j k h l` move and fold (vi style), and typing doesn't filter.
    pub nav: bool,
    /// On the "+ New workspace" row, after the tree (not in sidebar mode).
    pub on_new: bool,
    /// Take the whole of the area it's given (the narrow layout).
    pub full: bool,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Stay,
    Close,
    Go(Target),
    /// Make a new workspace; the filter, if any, as its name.
    NewWorkspace(Option<String>),
}

impl Chooser {
    pub fn new(at: Option<Target>) -> Chooser {
        Chooser {
            at,
            ..Default::default()
        }
    }

    /// For the focused sidebar: vi keys, no filtering.
    pub fn sidebar(at: Option<Target>) -> Chooser {
        Chooser {
            at,
            nav: true,
            ..Default::default()
        }
    }

    fn index(&self, rows: &[Row]) -> usize {
        rows.iter().position(|r| r.target == self.at).unwrap_or(0)
    }

    /// `rows`: the tree as currently shown (with this chooser's filter).
    pub fn key(&mut self, k: &KeyEvent, rows: &[Row], fold: &mut Fold) -> Outcome {
        let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let vi = |code| KeyEvent::new(code, KeyModifiers::NONE);
        let translated;
        let k = match k.code {
            KeyCode::Char('j') if self.nav && plain => {
                translated = vi(KeyCode::Down);
                &translated
            }
            KeyCode::Char('k') if self.nav && plain => {
                translated = vi(KeyCode::Up);
                &translated
            }
            KeyCode::Char('h') if self.nav && plain => {
                translated = vi(KeyCode::Left);
                &translated
            }
            KeyCode::Char('l') if self.nav && plain => {
                translated = vi(KeyCode::Right);
                &translated
            }
            KeyCode::Char(_) | KeyCode::Backspace if self.nav => return Outcome::Stay,
            _ => k,
        };
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        // The "+ New workspace" row: below the last row; where the cursor is when nothing matches.
        if !self.nav {
            let on_new = self.on_new || rows.is_empty();
            let up = matches!(k.code, KeyCode::Up) || (ctrl && matches!(k.code, KeyCode::Char('p' | 'k')));
            let down = matches!(k.code, KeyCode::Down) || (ctrl && matches!(k.code, KeyCode::Char('n' | 'j')));
            let last = rows.len().saturating_sub(1);
            match k.code {
                KeyCode::Enter if on_new => {
                    return Outcome::NewWorkspace(Some(self.filter.trim().to_string()).filter(|f| !f.is_empty()));
                }
                KeyCode::End => {
                    self.on_new = true;
                    return Outcome::Stay;
                }
                _ if down && !rows.is_empty() && self.index(rows) == last => {
                    self.on_new = true;
                    return Outcome::Stay;
                }
                _ if up && on_new => {
                    self.on_new = false;
                    self.at = rows.last().and_then(|r| r.target.clone());
                    return Outcome::Stay;
                }
                _ if down && on_new => return Outcome::Stay,
                KeyCode::Home | KeyCode::Char(_) | KeyCode::Backspace => self.on_new = false,
                _ => {}
            }
        }
        let i = self.index(rows);
        let row = rows.get(i);
        let target = row.and_then(|r| r.target.clone());
        let mut move_to = |j: usize| self.at = rows.get(j).and_then(|r| r.target.clone());
        match k.code {
            KeyCode::Esc if !self.filter.is_empty() => self.filter.clear(),
            KeyCode::Esc => return Outcome::Close,
            KeyCode::Char('c' | 'g') if ctrl => return Outcome::Close,
            KeyCode::Enter => return target.map_or(Outcome::Close, Outcome::Go),
            KeyCode::Up => move_to(i.saturating_sub(1)),
            KeyCode::Char('p' | 'k') if ctrl => move_to(i.saturating_sub(1)),
            KeyCode::Down => move_to((i + 1).min(rows.len().saturating_sub(1))),
            KeyCode::Char('n' | 'j') if ctrl => move_to((i + 1).min(rows.len().saturating_sub(1))),
            KeyCode::Home => move_to(0),
            KeyCode::End => move_to(rows.len().saturating_sub(1)),
            KeyCode::Right | KeyCode::Tab | KeyCode::Left => {
                let Some(r) = row else { return Outcome::Stay };
                let want_open = match k.code {
                    KeyCode::Right => true,
                    KeyCode::Left => false,
                    _ => !r.open,
                };
                if r.foldable && r.open != want_open {
                    fold.set(r.target.as_ref().unwrap(), want_open);
                } else if k.code == KeyCode::Left && r.depth > 0 {
                    // Already closed (or a leaf): go up to the parent row.
                    let parent = rows[..i].iter().rposition(|p| p.depth < r.depth);
                    if let Some(p) = parent {
                        move_to(p);
                    }
                }
            }
            KeyCode::Backspace => {
                self.filter.pop();
            }
            KeyCode::Char('u') if ctrl => self.filter.clear(),
            KeyCode::Char(c) if !ctrl => {
                self.filter.push(c);
            }
            _ => {}
        }
        Outcome::Stay
    }

    /// After the rows change (filter, fold, layout): keep the cursor on a row that exists.
    pub fn settle(&mut self, rows: &[Row]) {
        if !rows.iter().any(|r| r.target == self.at) {
            self.at = rows.first().and_then(|r| r.target.clone());
        }
    }

    pub fn render(&mut self, rows: &[Row], area: Rect, buf: &mut Buffer, cfg: &Sidebar, t: &Theme, ind: Indicators) {
        // About three quarters of the screen (more if the tree needs it), within a margin.
        let w = (area.width * 3 / 4)
            .clamp(40, 120)
            .min(area.width.saturating_sub(4))
            .max(20.min(area.width));
        let room = area.height.saturating_sub(2).max(7);
        let h = (rows.len() as u16 + 5)
            .max(area.height * 3 / 4)
            .clamp(7, room)
            .min(area.height);
        let rect = if self.full {
            area
        } else {
            crate::ui::centered(area, w, h)
        };
        Clear.render(rect, buf);
        let title = Line::from(vec![
            Span::raw(" "),
            Span::styled("choose", Style::new().fg(t.prefix).add_modifier(Modifier::BOLD)),
            Span::raw(" "),
        ]);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(t.border_type())
            .border_style(Style::new().fg(t.prefix))
            .title(title);
        let inner = block.inner(rect);
        block.render(rect, buf);
        if inner.height < 3 {
            return;
        }
        // Filter line, the rows, then a footer.
        let filter = if self.filter.is_empty() {
            Line::from(Span::styled("type to filter", Style::new().fg(t.muted)))
        } else {
            Line::from(vec![
                Span::styled("filter: ", Style::new().fg(t.muted)),
                Span::raw(self.filter.clone()),
            ])
        };
        filter.render(Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), 1), buf);
        // The tree, then the "+ New workspace" row, pinned above the footer.
        let list = Rect::new(inner.x, inner.y + 1, inner.width, inner.height.saturating_sub(3));
        let on_new = self.on_new || rows.is_empty();
        let i = self.index(rows);
        let n = list.height as usize;
        if i < self.top {
            self.top = i;
        } else if i >= self.top + n {
            self.top = i + 1 - n;
        }
        for (k, row) in rows.iter().enumerate().skip(self.top).take(n) {
            let y = list.y + (k - self.top) as u16;
            let r = Rect::new(list.x, y, list.width, 1);
            let style = if k == i && !on_new {
                Style::new().bg(t.accent).fg(t.accent_fg).add_modifier(Modifier::BOLD)
            } else if row.down && !matches!(row.target, Some(Target::Machine(_))) {
                Style::new().fg(t.muted)
            } else {
                Style::new().fg(t.fg)
            };
            buf.set_style(r, style);
            let (spans, _) = tree_line(row, cfg, t, ind);
            Line::from(spans)
                .style(style)
                .render(Rect::new(r.x + 1, y, r.width.saturating_sub(2), 1), buf);
        }
        if rows.is_empty() {
            Line::from(Span::styled("nothing matches", Style::new().fg(t.muted)))
                .render(Rect::new(list.x + 1, list.y, list.width.saturating_sub(2), 1), buf);
        }
        let new_row = Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 1);
        let label = match self.filter.trim() {
            "" => "+ New workspace…".to_string(),
            f => format!("+ New workspace “{f}”…"),
        };
        let style = if on_new {
            Style::new().bg(t.accent).fg(t.accent_fg).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(t.muted)
        };
        buf.set_style(new_row, style);
        Line::from(Span::styled(label, style)).render(
            Rect::new(new_row.x + 1, new_row.y, new_row.width.saturating_sub(2), 1),
            buf,
        );
        let footer = "↑↓ move · → ← fold · Enter go · Esc close";
        let fw = (footer.chars().count() as u16).min(inner.width);
        Line::from(Span::styled(footer, Style::new().fg(t.muted))).render(
            Rect::new(inner.x + (inner.width - fw) / 2, inner.bottom() - 1, fw, 1),
            buf,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(fold: &Fold) -> Vec<Row> {
        let w = |ws: &str, open| Row {
            target: Some(Target::Workspace(ws.into())),
            foldable: true,
            open,
            workspace: ws.into(),
            ..Default::default()
        };
        let win = |ws: &str, id: &str, panes: bool| Row {
            target: Some(Target::Window {
                ws: ws.into(),
                win: id.into(),
            }),
            depth: 1,
            foldable: panes,
            open: panes
                && fold.is_open(&Target::Window {
                    ws: ws.into(),
                    win: id.into(),
                }),
            window: id.into(),
            ..Default::default()
        };
        let mut out = vec![w("s1", fold.is_open(&Target::Workspace("s1".into())))];
        if out[0].open {
            out.push(win("s1", "w1", true));
            if out[1].open {
                for p in ["t1", "t2"] {
                    out.push(Row {
                        target: Some(Target::Pane {
                            ws: "s1".into(),
                            win: "w1".into(),
                            pane: p.into(),
                        }),
                        depth: 2,
                        pane: p.into(),
                        ..Default::default()
                    });
                }
            }
            out.push(win("s1", "w2", false));
        }
        out.push(w("s2", true));
        out
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    #[test]
    fn the_last_row_makes_a_new_workspace() {
        let mut fold = Fold::default();
        let mut ch = Chooser::new(Some(Target::Workspace("s1".into())));
        let r = rows(&fold);
        let last = r.len() - 1;
        for _ in 0..last {
            ch.key(&key(KeyCode::Down), &r, &mut fold);
        }
        assert!(!ch.on_new);
        ch.key(&key(KeyCode::Down), &r, &mut fold);
        assert!(ch.on_new, "below the last row");
        assert_eq!(ch.key(&key(KeyCode::Enter), &r, &mut fold), Outcome::NewWorkspace(None));
        ch.key(&key(KeyCode::Up), &r, &mut fold);
        assert!(!ch.on_new);
        assert_eq!(ch.at, r[last].target, "back on the last row");
        // A filter that matches nothing: Enter makes one named after it.
        let mut ch = Chooser::new(None);
        for c in "notes".chars() {
            ch.key(&key(KeyCode::Char(c)), &[], &mut fold);
        }
        assert_eq!(
            ch.key(&key(KeyCode::Enter), &[], &mut fold),
            Outcome::NewWorkspace(Some("notes".into()))
        );
        // Not in the sidebar.
        let mut sb = Chooser::sidebar(Some(Target::Workspace("s1".into())));
        let r = rows(&fold);
        for _ in 0..10 {
            sb.key(&key(KeyCode::Down), &r, &mut fold);
        }
        assert!(!sb.on_new);
    }

    #[test]
    fn move_fold_and_go() {
        let mut fold = Fold::default();
        let mut ch = Chooser::new(Some(Target::Window {
            ws: "s1".into(),
            win: "w1".into(),
        }));
        // → opens the window: its panes appear below it.
        assert_eq!(ch.key(&key(KeyCode::Right), &rows(&fold), &mut fold), Outcome::Stay);
        assert_eq!(rows(&fold).len(), 6);
        ch.key(&key(KeyCode::Down), &rows(&fold), &mut fold);
        assert_eq!(
            ch.at,
            Some(Target::Pane {
                ws: "s1".into(),
                win: "w1".into(),
                pane: "t1".into()
            })
        );
        // ← on a pane goes to its window; ← again closes it; ← again goes to the workspace.
        ch.key(&key(KeyCode::Left), &rows(&fold), &mut fold);
        assert_eq!(
            ch.at,
            Some(Target::Window {
                ws: "s1".into(),
                win: "w1".into()
            })
        );
        ch.key(&key(KeyCode::Left), &rows(&fold), &mut fold);
        assert_eq!(rows(&fold).len(), 4, "closed");
        ch.key(&key(KeyCode::Left), &rows(&fold), &mut fold);
        assert_eq!(ch.at, Some(Target::Workspace("s1".into())));
        // Closing the workspace folds the sidebar too (same Fold).
        ch.key(&key(KeyCode::Left), &rows(&fold), &mut fold);
        assert!(!fold.is_open(&Target::Workspace("s1".into())));
        ch.key(&key(KeyCode::Down), &rows(&fold), &mut fold);
        assert_eq!(
            ch.key(&key(KeyCode::Enter), &rows(&fold), &mut fold),
            Outcome::Go(Target::Workspace("s2".into()))
        );
    }

    #[test]
    fn typing_filters_and_esc_clears_then_closes() {
        let mut fold = Fold::default();
        let mut ch = Chooser::new(None);
        for c in "w2".chars() {
            ch.key(&key(KeyCode::Char(c)), &rows(&fold), &mut fold);
        }
        assert_eq!(ch.filter, "w2");
        ch.key(&key(KeyCode::Backspace), &rows(&fold), &mut fold);
        assert_eq!(ch.filter, "w");
        assert_eq!(ch.key(&key(KeyCode::Esc), &rows(&fold), &mut fold), Outcome::Stay);
        assert!(ch.filter.is_empty());
        assert_eq!(ch.key(&key(KeyCode::Esc), &rows(&fold), &mut fold), Outcome::Close);
    }

    #[test]
    fn sidebar_mode_uses_vi_keys_and_never_filters() {
        let mut fold = Fold::default();
        let mut ch = Chooser {
            nav: true,
            ..Chooser::new(Some(Target::Workspace("s1".into())))
        };
        ch.key(&key(KeyCode::Char('j')), &rows(&fold), &mut fold);
        assert_eq!(
            ch.at,
            Some(Target::Window {
                ws: "s1".into(),
                win: "w1".into()
            })
        );
        ch.key(&key(KeyCode::Char('l')), &rows(&fold), &mut fold);
        assert!(fold.is_open(&Target::Window {
            ws: "s1".into(),
            win: "w1".into()
        }));
        ch.key(&key(KeyCode::Char('k')), &rows(&fold), &mut fold);
        ch.key(&key(KeyCode::Char('h')), &rows(&fold), &mut fold);
        assert!(!fold.is_open(&Target::Workspace("s1".into())));
        ch.key(&key(KeyCode::Char('x')), &rows(&fold), &mut fold);
        assert!(ch.filter.is_empty(), "no filtering in the sidebar");
    }

    #[test]
    fn settle_keeps_the_cursor_on_a_real_row() {
        let fold = Fold::default();
        let mut ch = Chooser::new(Some(Target::Pane {
            ws: "s9".into(),
            win: "w9".into(),
            pane: "t9".into(),
        }));
        ch.settle(&rows(&fold));
        assert_eq!(ch.at, Some(Target::Workspace("s1".into())));
    }
}
