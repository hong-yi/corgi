//! Action menus (`C-b m`, `m` on a sidebar row, right-click): what you can do to a pane, its
//! window and its workspace, by section. Made for the keyboard: each entry has a key (the
//! same as its prefix key where there is one) that runs it at once; `j`/`k` or the arrows
//! move, Enter runs, Esc closes. Entries are commands, so they do what the keys do. Pure.

use crate::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Widget};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// The key that runs it from the menu: one character.
    pub key: char,
    pub label: String,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub title: String,
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug)]
pub struct Menu {
    pub title: String,
    pub sections: Vec<Section>,
    /// The cursor, over all entries in order.
    pub at: usize,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Stay,
    Close,
    Run(String),
}

/// Keys the menu moves with, so no entry may use them.
const MOVES: &[char] = &['j', 'k'];

impl Menu {
    pub fn new(title: &str, sections: Vec<Section>) -> Menu {
        debug_assert!(
            sections
                .iter()
                .flat_map(|s| &s.entries)
                .all(|e| !MOVES.contains(&e.key)),
            "j and k move"
        );
        Menu {
            title: title.into(),
            sections,
            at: 0,
        }
    }

    fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.sections.iter().flat_map(|s| &s.entries)
    }

    pub fn key(&mut self, k: &KeyEvent) -> Outcome {
        let n = self.entries().count();
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Esc => return Outcome::Close,
            KeyCode::Char('c' | 'g') if ctrl => return Outcome::Close,
            KeyCode::Up | KeyCode::BackTab => self.at = (self.at + n.max(1) - 1) % n.max(1),
            KeyCode::Char('k') if !ctrl => self.at = (self.at + n.max(1) - 1) % n.max(1),
            KeyCode::Char('p') if ctrl => self.at = (self.at + n.max(1) - 1) % n.max(1),
            KeyCode::Down | KeyCode::Tab => self.at = (self.at + 1) % n.max(1),
            KeyCode::Char('j') if !ctrl => self.at = (self.at + 1) % n.max(1),
            KeyCode::Char('n') if ctrl => self.at = (self.at + 1) % n.max(1),
            KeyCode::Home => self.at = 0,
            KeyCode::End => self.at = n.saturating_sub(1),
            KeyCode::Enter => {
                return self
                    .entries()
                    .nth(self.at)
                    .map_or(Outcome::Close, |e| Outcome::Run(e.command.clone()));
            }
            KeyCode::Char(c) if !ctrl => {
                if let Some(e) = self.entries().find(|e| e.key == c) {
                    return Outcome::Run(e.command.clone());
                }
            }
            _ => {}
        }
        Outcome::Stay
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, t: &Theme) {
        let label_w = self.entries().map(|e| e.label.width()).max().unwrap_or(10) as u16;
        let titles_w = self.sections.iter().map(|s| s.title.width()).max().unwrap_or(0) as u16;
        let footer = "j/k move · Enter or its key: run · Esc close";
        let w = (label_w + 8)
            .max(titles_w + 4)
            .max(footer.width() as u16 + 4)
            .max(self.title.width() as u16 + 6)
            .min(area.width);
        let rows = self.entries().count() as u16 + 2 * self.sections.len() as u16;
        let h = (rows + 3).min(area.height);
        let rect = crate::ui::centered(area, w, h);
        Clear.render(rect, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(t.border_type())
            .border_style(Style::new().fg(t.prefix))
            .style(Style::new().bg(t.surface).fg(t.fg))
            .title(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    self.title.clone(),
                    Style::new().fg(t.prefix).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
            ]));
        let inner = block.inner(rect);
        block.render(rect, buf);
        let header = Style::new().fg(t.muted).bg(t.surface).add_modifier(Modifier::BOLD);
        let mut y = inner.y;
        let mut i = 0;
        for (si, s) in self.sections.iter().enumerate() {
            if si > 0 {
                y += 1;
            }
            if y + 1 >= inner.bottom() {
                break;
            }
            buf.set_stringn(inner.x + 1, y, &s.title, inner.width.saturating_sub(2) as usize, header);
            y += 1;
            for e in &s.entries {
                if y + 1 >= inner.bottom() {
                    break;
                }
                let row = Rect::new(inner.x, y, inner.width, 1);
                let sel = i == self.at;
                let (base, key) = if sel {
                    let s = Style::new().bg(t.accent).fg(t.accent_fg);
                    (s, s.add_modifier(Modifier::BOLD))
                } else {
                    (
                        Style::new().fg(t.fg),
                        Style::new().fg(t.prefix).add_modifier(Modifier::BOLD),
                    )
                };
                buf.set_style(row, base);
                Line::from(vec![
                    Span::styled(format!("  {}  ", e.key), key),
                    Span::styled(e.label.clone(), base),
                ])
                .render(row, buf);
                y += 1;
                i += 1;
            }
        }
        let fw = (footer.width() as u16).min(inner.width);
        buf.set_stringn(
            inner.x + (inner.width - fw) / 2,
            inner.bottom() - 1,
            footer,
            fw as usize,
            Style::new().fg(t.muted).bg(t.surface),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> Menu {
        let e = |key, label: &str, command: &str| Entry {
            key,
            label: label.into(),
            command: command.into(),
        };
        Menu::new(
            "t3",
            vec![
                Section {
                    title: "Pane".into(),
                    entries: vec![
                        e('%', "split right", "split-window -h"),
                        e('x', "close pane", "kill-pane"),
                    ],
                },
                Section {
                    title: "Window".into(),
                    entries: vec![e(',', "rename window…", "rename-window")],
                },
            ],
        )
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    #[test]
    fn keys_run_entries_at_once_or_by_cursor() {
        let mut m = menu();
        assert_eq!(
            m.key(&key(KeyCode::Char('x'))),
            Outcome::Run("kill-pane".into()),
            "its key"
        );
        assert_eq!(m.key(&key(KeyCode::Char('?'))), Outcome::Stay, "no such entry");
        m.key(&key(KeyCode::Char('j')));
        m.key(&key(KeyCode::Char('j')));
        assert_eq!(
            m.key(&key(KeyCode::Enter)),
            Outcome::Run("rename-window".into()),
            "across sections"
        );
        m.key(&key(KeyCode::Char('j')));
        assert_eq!(m.at, 0, "wraps");
        m.key(&key(KeyCode::Char('k')));
        assert_eq!(m.at, 2);
        assert_eq!(m.key(&key(KeyCode::Esc)), Outcome::Close);
    }

    #[test]
    fn renders_sections_keys_and_the_cursor() {
        let theme = crate::theme::builtin("terminal").unwrap();
        let area = Rect::new(0, 0, 70, 16);
        let mut buf = Buffer::empty(area);
        let mut m = menu();
        m.at = 1;
        m.render(area, &mut buf, &theme);
        let text: Vec<String> = (0..16)
            .map(|y| (0..70).map(|x| buf[(x, y)].symbol().to_string()).collect())
            .collect();
        let all = text.join("\n");
        assert!(all.contains(" t3 "), "{all}");
        let pane = text.iter().position(|l| l.contains("Pane")).unwrap();
        let window = text.iter().position(|l| l.contains("Window")).unwrap();
        assert!(text[pane + 1].contains("%  split right"));
        let x_row = text.iter().position(|l| l.contains("x  close pane")).unwrap();
        assert!(pane < x_row && x_row < window, "{all}");
        let col = text[x_row]
            .find("close")
            .map(|b| text[x_row][..b].chars().count())
            .unwrap() as u16;
        assert_eq!(buf[(col, x_row as u16)].bg, theme.accent, "the cursor's row");
        assert!(all.contains("Enter or its key: run"));
    }
}
