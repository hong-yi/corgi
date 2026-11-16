//! The narrow layout's header (`ui.narrow_width`, a phone over ssh): two rows at the top in
//! place of the tab bar and sidebar. The first: the mode (when it isn't NORMAL), the workspace
//! with its state, the window (`2/3` of how many) and the pane (when there are several, as
//! only the focused one shows). The second: the agents (waiting, working, done), or a message,
//! and usage in what's left. On the right, a "switch" button (`C-b w`): the chooser, full screen.

use super::*;
use crate::status::{self, Agent};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

/// The switch button's width.
const BUTTON: u16 = 10;

impl App {
    pub(super) fn draw_header(&mut self, f: &mut ratatui::Frame, area: Rect) {
        let t = self.theme;
        let base = Style::new().bg(t.surface).fg(t.fg);
        let muted = Style::new().bg(t.surface).fg(t.muted);
        f.buffer_mut().set_style(area, base);
        self.hits.tabs.clear();
        if area.width == 0 || area.height == 0 {
            return;
        }
        let row1 = Rect::new(area.x, area.y, area.width, 1);
        // A prompt takes the first row, as it takes the tab bar.
        if matches!(self.mode, Mode::Prompt(_)) {
            self.draw_status_line(f, row1);
            return;
        }
        let button = BUTTON.min(area.width / 3);
        let left = Rect::new(area.x, area.y, area.width - button, 1);
        let btn = Rect::new(area.right() - button, area.y, button, area.height);

        // First row: mode, workspace, window, pane.
        let mut spans = Vec::new();
        let (label, color) = self.mode_badge();
        if !label.is_empty() && label != self.config.mode.normal {
            let style = Style::new().bg(color).fg(t.accent_fg).add_modifier(Modifier::BOLD);
            spans.push(Span::styled(format!(" {label} "), style));
        }
        let (ws, win) = self.active().unwrap_or_default();
        match self.doc.workspace(&ws) {
            Some(s) => {
                let terms = s
                    .windows
                    .iter()
                    .flat_map(|w| w.root.panes())
                    .filter_map(|p| self.terminals.get(p));
                if let Some(a) = status::rollup(terms) {
                    let g = status::glyph(a, self.config.ui.indicators);
                    spans.push(Span::styled(format!(" {g}"), base.fg(status::color(a, &t))));
                }
                spans.push(Span::styled(
                    format!(" {}", self.ws_label(s)),
                    base.add_modifier(Modifier::BOLD),
                ));
                if let Some(m) = &s.machine {
                    let down = self.machines.iter().any(|x| &x.name == m && !x.is_up());
                    spans.push(Span::styled(
                        format!(" @{m}"),
                        if down { base.fg(t.blocked) } else { muted },
                    ));
                }
                let i = s.windows.iter().position(|w| w.id == win).unwrap_or(0);
                if let Some(w) = s.windows.get(i) {
                    let name = w.name.clone().unwrap_or_else(|| self.pane_name(&w.focus));
                    let mut text = format!(" · {i}:{name}");
                    if s.windows.len() > 1 {
                        text.push_str(&format!(" {}/{}", i + 1, s.windows.len()));
                    }
                    let panes = crate::tree::made_order(w.root.panes());
                    if panes.len() > 1 {
                        let j = panes.iter().position(|p| **p == w.focus).unwrap_or(0);
                        text.push_str(&format!(" · pane {}/{}", j + 1, panes.len()));
                    }
                    spans.push(Span::styled(text, muted));
                }
            }
            None => spans.push(Span::styled(" no workspace", muted)),
        }
        f.render_widget(Paragraph::new(Line::from(spans)).style(base), left);

        // Second row: a message, else the agents; usage in what's left.
        if area.height > 1 {
            let row2 = Rect::new(area.x, area.y + 1, area.width - button, 1);
            let note = match (&self.note, self.conn) {
                (Some((n, at)), _) if at.elapsed() < super::NOTE_FOR => Some(n.clone()),
                (_, Conn::Reconnecting) => Some("core: reconnecting…".into()),
                _ => None,
            };
            let spans = match note {
                Some(n) => vec![Span::styled(format!(" {n}"), base)],
                None => self.agent_summary(&t),
            };
            let used: u16 = spans.iter().map(|s| s.content.width() as u16).sum();
            f.render_widget(Paragraph::new(Line::from(spans)).style(muted), row2);
            let free = row2.width.saturating_sub(used + 2) as usize;
            let placed = self.usage_placed(free);
            if !placed.is_empty() {
                use crate::usage::{GAP, PAD};
                let total: usize =
                    placed.iter().map(|(t, ..)| t.width() + PAD).sum::<usize>() + GAP * (placed.len() - 1);
                let mut x = row2.right().saturating_sub(total as u16 + 1);
                let start = x;
                for (i, (text, stale, style)) in placed.iter().enumerate() {
                    let text = if i > 0 {
                        format!("{}{text} ", " ".repeat(GAP + 1))
                    } else {
                        format!(" {text} ")
                    };
                    let style = if *stale {
                        style.add_modifier(Modifier::DIM)
                    } else {
                        *style
                    };
                    let w = text.width() as u16;
                    f.render_widget(Paragraph::new(text).style(style), Rect::new(x, row2.y, w, 1));
                    x += w;
                }
                self.hits
                    .tabs
                    .push((Rect::new(start, row2.y, x - start, 1), TabHit::Usage));
            }
        }

        // The switch button, marked when an agent waits for you.
        let bstyle = Style::new().bg(t.border).fg(t.fg).add_modifier(Modifier::BOLD);
        f.buffer_mut().set_style(btn, bstyle);
        let label = "switch";
        let lx = btn.x + btn.width.saturating_sub(label.width() as u16) / 2;
        f.buffer_mut().set_stringn(lx, btn.y, label, btn.width as usize, bstyle);
        if btn.height > 1 {
            let hint = format!("{} w", self.config.keys.prefix);
            let hx = btn.x + btn.width.saturating_sub(hint.width() as u16) / 2;
            f.buffer_mut().set_stringn(
                hx,
                btn.y + 1,
                &hint,
                btn.width as usize,
                Style::new().bg(t.border).fg(t.muted),
            );
        }
        let waiting = self
            .terminals
            .values()
            .any(|x| status::agent(x) == Some(Agent::Blocked));
        if waiting && btn.width > 0 {
            f.buffer_mut().set_stringn(
                btn.right() - 1,
                btn.y,
                status::glyph(Agent::Blocked, self.config.ui.indicators),
                1,
                Style::new().bg(t.border).fg(t.blocked),
            );
        }
        self.hits.tabs.push((btn, TabHit::Switch));
    }

    /// Agents everywhere, by state: waiting, working, finished.
    fn agent_summary(&self, t: &crate::theme::Theme) -> Vec<Span<'static>> {
        let mut n = [0usize; 3];
        for s in &self.doc.workspaces {
            for p in s.windows.iter().flat_map(|w| w.root.panes()) {
                match self.terminals.get(p).and_then(status::agent) {
                    Some(Agent::Blocked) => n[0] += 1,
                    Some(Agent::Working) => n[1] += 1,
                    Some(Agent::Done) => n[2] += 1,
                    _ => {}
                }
            }
        }
        let ind = self.config.ui.indicators;
        let mut spans = Vec::new();
        for (count, a, word) in [
            (n[0], Agent::Blocked, "waiting"),
            (n[1], Agent::Working, "working"),
            (n[2], Agent::Done, "done"),
        ] {
            if count == 0 {
                continue;
            }
            let sep = if spans.is_empty() { " " } else { " · " };
            spans.push(Span::styled(
                format!("{sep}{} ", status::glyph(a, ind)),
                Style::new().bg(t.surface).fg(status::color(a, t)),
            ));
            spans.push(Span::styled(
                format!("{count} {word}"),
                Style::new().bg(t.surface).fg(t.fg),
            ));
        }
        if spans.is_empty() {
            spans.push(Span::styled(" no agents", Style::new().bg(t.surface).fg(t.muted)));
        }
        spans
    }
}
