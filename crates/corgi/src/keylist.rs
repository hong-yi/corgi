//! `C-b ?`: every key binding, by table, in a dialog. Typing filters (by key, command
//! or description); Enter runs the selected binding's command. Pure apart from drawing.

use crate::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Widget};

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub table: String,
    pub key: String,
    pub command: String,
    /// What it does, when the binding says (the key-hints label).
    pub note: Option<String>,
}

impl Row {
    fn matches(&self, f: &str) -> bool {
        let f = f.to_lowercase();
        [&self.table, &self.key, &self.command]
            .iter()
            .any(|s| s.to_lowercase().contains(&f))
            || self.note.as_ref().is_some_and(|n| n.to_lowercase().contains(&f))
    }
}

#[derive(Clone, Debug, Default)]
pub struct KeyList {
    pub filter: String,
    /// Cursor, within the filtered rows.
    pub at: usize,
    top: usize,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Stay,
    Close,
    /// Run this command (the selected binding's).
    Run(String),
}

impl KeyList {
    pub fn filtered<'a>(&self, rows: &'a [Row]) -> Vec<&'a Row> {
        rows.iter()
            .filter(|r| self.filter.is_empty() || r.matches(&self.filter))
            .collect()
    }

    pub fn key(&mut self, k: &KeyEvent, rows: &[Row]) -> Outcome {
        let n = self.filtered(rows).len();
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Esc if !self.filter.is_empty() => {
                self.filter.clear();
                self.at = 0;
            }
            KeyCode::Esc => return Outcome::Close,
            KeyCode::Char('c' | 'g') if ctrl => return Outcome::Close,
            KeyCode::Enter => {
                return match self.filtered(rows).get(self.at) {
                    Some(r) => Outcome::Run(r.command.clone()),
                    None => Outcome::Close,
                };
            }
            KeyCode::Up => self.at = self.at.saturating_sub(1),
            KeyCode::Char('p' | 'k') if ctrl => self.at = self.at.saturating_sub(1),
            KeyCode::Down => self.at = (self.at + 1).min(n.saturating_sub(1)),
            KeyCode::Char('n' | 'j') if ctrl => self.at = (self.at + 1).min(n.saturating_sub(1)),
            KeyCode::PageUp => self.at = self.at.saturating_sub(10),
            KeyCode::PageDown => self.at = (self.at + 10).min(n.saturating_sub(1)),
            KeyCode::Home => self.at = 0,
            KeyCode::End => self.at = n.saturating_sub(1),
            KeyCode::Backspace => {
                self.filter.pop();
                self.at = 0;
            }
            KeyCode::Char('u') if ctrl => {
                self.filter.clear();
                self.at = 0;
            }
            KeyCode::Char(c) if !ctrl => {
                self.filter.push(c);
                self.at = 0;
            }
            _ => {}
        }
        Outcome::Stay
    }

    pub fn render(&mut self, rows: &[Row], area: Rect, buf: &mut Buffer, t: &Theme) {
        let shown = self.filtered(rows);
        let w = area.width.saturating_sub(4).min(100);
        let h = area.height.saturating_sub(2).max(6);
        let rect = crate::ui::centered(area, w, h);
        Clear.render(rect, buf);
        let title = Line::from(vec![
            Span::raw(" "),
            Span::styled("keys", Style::new().fg(t.prefix).add_modifier(Modifier::BOLD)),
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
        let filter = if self.filter.is_empty() {
            Line::from(Span::styled(
                format!("type to filter · {} bindings", rows.len()),
                Style::new().fg(t.muted),
            ))
        } else {
            Line::from(vec![
                Span::styled("filter: ", Style::new().fg(t.muted)),
                Span::raw(self.filter.clone()),
                Span::styled(
                    format!("  ({} of {})", shown.len(), rows.len()),
                    Style::new().fg(t.muted),
                ),
            ])
        };
        filter.render(Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), 1), buf);
        let list = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 2);
        let n = list.height as usize;
        self.at = self.at.min(shown.len().saturating_sub(1));
        if self.at < self.top {
            self.top = self.at;
        } else if self.at >= self.top + n {
            self.top = self.at + 1 - n;
        }
        let kw = shown.iter().map(|r| r.key.chars().count()).max().unwrap_or(1).max(3);
        for (i, r) in shown.iter().enumerate().skip(self.top).take(n) {
            let y = list.y + (i - self.top) as u16;
            let line = Rect::new(list.x, y, list.width, 1);
            let sel = i == self.at;
            let base = if sel {
                Style::new().bg(t.accent).fg(t.accent_fg)
            } else {
                Style::new().fg(t.fg)
            };
            buf.set_style(line, base);
            let mut spans = vec![
                Span::styled(
                    format!(" {:<7}", r.table),
                    if sel { base } else { Style::new().fg(t.muted) },
                ),
                Span::styled(format!("{:>kw$}  ", r.key), base.add_modifier(Modifier::BOLD)),
            ];
            match &r.note {
                Some(note) if *note != r.command => {
                    spans.push(Span::styled(format!("{note}  "), base));
                    spans.push(Span::styled(
                        r.command.clone(),
                        if sel { base } else { Style::new().fg(t.muted) },
                    ));
                }
                _ => spans.push(Span::styled(r.command.clone(), base)),
            }
            Line::from(spans).render(line, buf);
        }
        if shown.is_empty() {
            Line::from(Span::styled(" nothing matches", Style::new().fg(t.muted)))
                .render(Rect::new(list.x, list.y, list.width, 1), buf);
        }
        let footer = "↑↓ move · Enter run · type to filter · Esc close";
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

    fn rows() -> Vec<Row> {
        let r = |table: &str, key: &str, command: &str, note: Option<&str>| Row {
            table: table.into(),
            key: key.into(),
            command: command.into(),
            note: note.map(Into::into),
        };
        vec![
            r("prefix", "%", "split-window -h", Some("split right")),
            r("prefix", "\"", "split-window -v", Some("split down")),
            r("prefix", "c", "new-window", Some("new window")),
            r("root", "M-h", "select-pane -L", None),
        ]
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    #[test]
    fn filter_move_and_run() {
        let rows = rows();
        let mut kl = KeyList::default();
        for c in "split".chars() {
            kl.key(&key(KeyCode::Char(c)), &rows);
        }
        assert_eq!(kl.filtered(&rows).len(), 2);
        kl.key(&key(KeyCode::Down), &rows);
        assert_eq!(
            kl.key(&key(KeyCode::Enter), &rows),
            Outcome::Run("split-window -v".into())
        );
        // By description and by table too.
        kl.filter = "new window".into();
        assert_eq!(kl.filtered(&rows).len(), 1);
        kl.filter = "root".into();
        assert_eq!(kl.filtered(&rows)[0].key, "M-h");
        assert_eq!(
            kl.key(&key(KeyCode::Esc), &rows),
            Outcome::Stay,
            "Esc clears the filter first"
        );
        assert_eq!(kl.key(&key(KeyCode::Esc), &rows), Outcome::Close);
    }

    #[test]
    fn renders_rows_with_notes() {
        let rows = rows();
        let theme = crate::theme::builtin("terminal").unwrap();
        let area = Rect::new(0, 0, 80, 12);
        let mut buf = Buffer::empty(area);
        KeyList::default().render(&rows, area, &mut buf, &theme);
        let text: Vec<String> = (0..12)
            .map(|y| (0..80).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>())
            .collect();
        assert!(
            text.iter()
                .any(|l| l.contains("prefix") && l.contains("%  split right  split-window -h")),
            "{text:#?}"
        );
        assert!(text.iter().any(|l| l.contains("M-h  select-pane -L")));
        assert!(
            text.iter().any(|l| l.contains("4 bindings")),
            "the filter line counts them"
        );
    }
}
