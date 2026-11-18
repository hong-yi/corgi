//! Drawing: sidebar, status line (workspace badge, window tabs, messages),
//! panes with their separators, and overlays. Also records where clickable
//! things landed, for the mouse.

use super::{App, Conn, Mode, TabHit};
use crate::config::{Hints, Position, SidebarMode};
use crate::keyhints::KeyHints;
use crate::model::Window;
use crate::pane::{Pane, State};
use crate::sidebar;
use crate::status;
use crate::theme::Theme;
use crate::ui;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use std::collections::{HashMap, HashSet};
use unicode_width::UnicodeWidthStr;

/// Where the three parts of the screen go.
pub struct Areas {
    pub sidebar: Option<Rect>,
    /// The tab bar's whole area (with its box, if it has one)…
    pub bar: Rect,
    /// …and the row its contents go in.
    pub bar_row: Rect,
    /// The panes' whole area (with its box, if it has one)…
    pub body: Rect,
    /// …and where the panes themselves go.
    pub panes: Rect,
}

/// One window in the status line.
struct Tab {
    label: String,
    style: Style,
    /// Status glyph and its style.
    dot: Option<(String, Style)>,
    id: String,
}

/// Below this width the sidebar would crowd the panes out: it hides.
const SIDEBAR_MIN_SCREEN: u16 = 60;

impl App {
    pub(super) fn areas(&self) -> Areas {
        let s = self.screen;
        let tb = &self.config.tab_bar;
        // Cut the tab bar (and its padding) off the top or bottom of `area`.
        let cut_bar = |area: Rect| -> (Rect, Rect, Rect) {
            // A box needs room: on short or narrow terminals the bar is one plain row.
            let boxed = tb.style == crate::config::TabBarStyle::Box && area.height >= 8 && area.width >= 10;
            // Narrow: a two-row header instead.
            let bar_h = if self.narrow {
                2.min(area.height.saturating_sub(1))
            } else if boxed {
                3
            } else {
                1
            };
            let pad = tb.padding.min(area.height.saturating_sub(bar_h + 1));
            let h = area.height.saturating_sub(bar_h + pad);
            let (bar, rest) = match tb.position {
                Position::Top => (
                    Rect::new(area.x, area.y, area.width, bar_h),
                    Rect::new(area.x, area.y + bar_h + pad, area.width, h),
                ),
                Position::Bottom => (
                    Rect::new(area.x, area.y + h + pad, area.width, bar_h),
                    Rect::new(area.x, area.y, area.width, h),
                ),
            };
            let row = if boxed {
                Rect::new(bar.x + 1, bar.y + 1, bar.width - 2, 1)
            } else {
                bar
            };
            (bar, row, rest)
        };
        // `span = "window"`: the bar runs under (or over) the sidebar too.
        let full = tb.span == crate::config::TabBarSpan::Window;
        let (mut bar, mut bar_row, top) = if full {
            cut_bar(s)
        } else {
            (Rect::default(), Rect::default(), s)
        };

        let width = match self.sidebar_mode {
            SidebarMode::Hidden => 0,
            _ if s.width < SIDEBAR_MIN_SCREEN || self.narrow => 0,
            SidebarMode::Collapsed if self.config.sidebar.style == crate::config::SidebarStyle::Box => 4,
            SidebarMode::Collapsed => 3,
            SidebarMode::Show => self
                .sidebar_width
                .unwrap_or(self.config.sidebar.width)
                .clamp(12, s.width.saturating_sub(20).max(12)),
        };
        let sidebar = (width > 0).then(|| Rect::new(top.x, top.y, width, top.height));
        // The gap between sidebar and panes: left empty, in the terminal's own background.
        let gap = if width > 0 {
            self.config.sidebar.padding.min(top.width.saturating_sub(width + 1))
        } else {
            0
        };
        let left = width + gap;
        let right = Rect::new(top.x + left, top.y, top.width - left, top.height);
        let body = if full {
            right
        } else {
            let (b, r, rest) = cut_bar(right);
            (bar, bar_row) = (b, r);
            rest
        };
        let boxed = self.config.panes.style == crate::config::TabBarStyle::Box && body.width >= 6 && body.height >= 4;
        let panes = if boxed {
            Rect::new(body.x + 1, body.y + 1, body.width - 2, body.height - 2)
        } else {
            body
        };
        Areas {
            sidebar,
            bar,
            bar_row,
            body,
            panes,
        }
    }

    pub(super) fn draw(&mut self, f: &mut ratatui::Frame) {
        self.screen = f.area();
        let Areas {
            sidebar,
            bar: bar_box,
            bar_row: bar,
            body: pane_box,
            panes: body,
        } = self.areas();
        let theme = self.theme;
        self.hits.sidebar.clear();
        if let Some(area) = sidebar {
            self.draw_sidebar(f, area);
        }
        if bar_box != bar {
            let border = Style::new().fg(theme.border).bg(theme.surface);
            let block = ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_type(theme.border_type())
                .border_style(border)
                .style(Style::new().bg(theme.surface));
            f.render_widget(block, bar_box);
        }
        if self.narrow {
            self.draw_header(f, bar);
        } else {
            self.draw_status_line(f, bar);
        }

        let rects = self.rects();
        let focus = self.window().map(|w| w.focus.clone()).unwrap_or_default();
        let zoomed = self.window().is_some_and(|w| w.zoomed);
        // Shared dividers only when panes share them; separate panes have their own boxes (or
        // blank space) between them.
        let tiling = self.tiling();
        let seps = match self.window() {
            Some(w) if !zoomed && !self.config.panes.separate => w.root.geometry_with(tiling.area, tiling.sep).1,
            _ => Vec::new(),
        };
        // The panes' edges light up beside the focused pane, unless the keyboard is elsewhere.
        let keys_here = self.sidebar_focus.is_none()
            && matches!(
                self.mode,
                Mode::Normal | Mode::Prefix(_) | Mode::Repeat(_) | Mode::Table(_) | Mode::Panes(_)
            );
        let active = rects
            .iter()
            .find(|(id, _)| *id == focus)
            .map(|(_, r)| *r)
            .filter(|_| keys_here);
        draw_separators(f.buffer_mut(), &seps, active, &theme);
        if tiling.boxes {
            // Each pane its own box, lit for the pane your keys go to.
            for (id, r) in self.frames() {
                let lit = keys_here && id == focus;
                let style = Style::new().fg(if lit { theme.border_active } else { theme.border });
                f.render_widget(
                    ratatui::widgets::Block::default()
                        .borders(ratatui::widgets::Borders::ALL)
                        .border_type(theme.border_type())
                        .border_style(style),
                    r,
                );
            }
        } else if pane_box != body {
            draw_box(f.buffer_mut(), pane_box, &seps, active, &theme);
        }
        let mut cursor = None;
        let copy_view = self.copy_view();
        for (id, r) in &rects {
            let Some(p) = self.panes.get_mut(id) else { continue };
            // Copy mode: shown from where it's looking, however much output came since.
            if let Some((pane, top)) = &copy_view
                && pane == id
            {
                p.vt.scroll_to(*top);
            }
            let Ok(frame) = p.vt.frame() else { continue };
            let c = ui::draw_frame(f.buffer_mut(), *r, frame);
            extend_into_inset(
                f.buffer_mut(),
                *r,
                self.config.panes.inset,
                self.config.panes.inset_rows,
            );
            if *id == focus && p.state == State::Live {
                cursor = c;
            }
            if p.state == State::Live && p.record.as_ref().is_some_and(crate::nesting::runs_corgi) {
                // Top row: the nested corgi's own messages sit at the bottom.
                let line = Rect::new(r.x, r.y, r.width, 1.min(r.height));
                let msg = format!(" ⚠ {id} is running corgi inside corgi: leave it (C-b C-b d) or quit it ");
                let style = Style::new()
                    .bg(theme.blocked)
                    .fg(theme.accent_fg)
                    .add_modifier(Modifier::BOLD);
                f.render_widget(Clear, line);
                f.render_widget(Paragraph::new(msg).style(style), line);
            }
            let down = crate::machines::of(id)
                .and_then(|name| self.machines.iter().find(|m| m.name == name))
                .filter(|m| !m.is_up())
                .map(|m| {
                    let mut say = format!("{} · its last screen", m.headline(crate::machines::now_ms()));
                    if let Some(w) = m.why.as_deref().filter(|w| !w.is_empty()) {
                        say.push_str(&format!(" · {w}"));
                    }
                    say
                });
            if down.is_some() {
                // What it showed last, greyed: its machine's line is down.
                f.buffer_mut()
                    .set_style(*r, Style::new().fg(theme.muted).add_modifier(Modifier::DIM));
            }
            if let Some(msg) = down.or_else(|| pane_message(p, self.conn)) {
                let line = Rect::new(r.x, r.bottom().saturating_sub(1), r.width, 1.min(r.height));
                f.render_widget(Clear, line);
                f.render_widget(
                    Paragraph::new(format!(" {msg}")).style(Style::new().bg(theme.surface).fg(theme.fg)),
                    line,
                );
            }
        }
        // The solid pill marks the pane your keys go to: none while they go elsewhere.
        let lit = if keys_here { focus.as_str() } else { "" };
        self.draw_pane_titles(f.buffer_mut(), lit, pane_box);
        let at = self.config.notifications.position;
        self.hits.toasts = self.toasts.render(body, at, f.buffer_mut(), &theme);
        if self.loaded && rects.is_empty() {
            let msg = if self.spawning > 0 {
                "starting a shell…"
            } else {
                "no windows"
            };
            f.render_widget(Paragraph::new(msg).style(Style::new().fg(theme.muted)), body);
        }
        // A popup over the panes (and under any dialog).
        if self.popup.is_some() {
            self.draw_popup(f);
        }
        match &self.mode {
            Mode::Prefix(_) | Mode::Table(_) if self.hints_open() => {
                let (table, title) = match &self.mode {
                    Mode::Table(t) => (t.clone(), t.clone()),
                    _ => ("prefix".to_string(), self.config.keys.prefix.clone()),
                };
                let groups = self.keymap.hints(&table);
                let prefix = title;
                let bottom = self.config.keys.hints_position == crate::config::HintsPosition::Bottom;
                // The whole window, not just the panes, whatever the layout; at the bottom, above
                // a bottom tab bar so the mode badge stays in sight.
                let mut area = self.screen;
                if bottom && self.config.tab_bar.position == Position::Bottom && bar_box.y > area.y {
                    area.height = bar_box.y - area.y;
                }
                f.render_widget(
                    KeyHints {
                        prefix: &prefix,
                        groups: &groups,
                        accent: theme.prefix,
                        border: theme.border_type(),
                        bottom,
                    },
                    area,
                );
            }
            Mode::Keys(_) => {
                let Mode::Keys(mut kl) = std::mem::replace(&mut self.mode, Mode::Normal) else {
                    unreachable!()
                };
                let rows = self.key_rows();
                kl.render(&rows, self.screen, f.buffer_mut(), &theme);
                self.mode = Mode::Keys(kl);
            }
            Mode::Panes(until) if std::time::Instant::now() < *until => {
                // tmux's display-panes: each pane's number, big enough to read at a glance.
                for (i, (_, r)) in rects.iter().enumerate() {
                    let label = format!(" {i} ");
                    let style = Style::new()
                        .bg(theme.mode_prefix)
                        .fg(theme.accent_fg)
                        .add_modifier(Modifier::BOLD);
                    let at = crate::ui::centered(*r, label.width() as u16, 1);
                    f.render_widget(Paragraph::new(label).style(style), at);
                }
            }
            Mode::Chooser(_) => {
                let Mode::Chooser(mut ch) = std::mem::replace(&mut self.mode, Mode::Normal) else {
                    unreachable!()
                };
                let rows = self.tree_rows(&ch.filter);
                // Narrow: the whole screen, like a phone's switcher.
                ch.full = self.narrow;
                ch.render(
                    &rows,
                    // Like every dialog: centred on the whole window.
                    self.screen,
                    f.buffer_mut(),
                    &self.config.sidebar,
                    &theme,
                    self.config.ui.indicators,
                );
                self.mode = Mode::Chooser(ch);
            }
            Mode::Menu(m, _) => m.render(self.screen, f.buffer_mut(), &theme),
            Mode::Copy(..) => self.draw_copy(f, &rects),
            Mode::Popup => {}
            Mode::Worktrees(_) => {
                let Mode::Worktrees(mut sh) = std::mem::replace(&mut self.mode, Mode::Normal) else {
                    unreachable!()
                };
                let open = self.worktrees_open(&sh);
                let home = crate::paths::home().display().to_string();
                let (list, folder) = (sh.list.clone(), sh.folder.clone());
                sh.view
                    .render(&list, &open, (&folder, &home), self.screen, f.buffer_mut(), &theme);
                self.mode = Mode::Worktrees(sh);
            }
            Mode::Machines(_) => {
                let Mode::Machines(mut v) = std::mem::replace(&mut self.mode, Mode::Normal) else {
                    unreachable!()
                };
                let counts = self.machine_workspaces();
                let now = crate::machines::now_ms();
                v.render(&self.machines, &counts, now, self.screen, f.buffer_mut(), &theme);
                self.mode = Mode::Machines(v);
            }
            Mode::Dialog(_) => {
                let Mode::Dialog(mut d) = std::mem::replace(&mut self.mode, Mode::Normal) else {
                    unreachable!()
                };
                // Like every dialog: centred on the whole window.
                if let Some(c) = d.form.render(self.screen, f.buffer_mut(), &theme) {
                    f.set_cursor_position(c);
                }
                self.mode = Mode::Dialog(d);
            }
            Mode::Prompt(p) => {
                let badge = self.mode_badge().0;
                let badge_w = if badge.is_empty() { 0 } else { badge.width() as u16 + 3 };
                let x = bar.x + badge_w + p.label.chars().count() as u16 + 2 + p.text.chars().count() as u16;
                f.set_cursor_position((x.min(bar.right().saturating_sub(1)), bar.y));
            }
            _ => {
                if let Some(c) = cursor {
                    f.set_cursor_position(c);
                }
            }
        }
    }

    /// Whether the key-hints dialog is up now: after the prefix (per `keys.hints`), and in other tables.
    fn hints_open(&self) -> bool {
        match &self.mode {
            Mode::Prefix(since) => self.hints_due(*since),
            Mode::Table(_) => self.config.keys.hints != Hints::Off,
            _ => false,
        }
    }

    /// When the key hints are due to open, while they're waiting to (`keys.hints = "delay"`,
    /// in prefix mode): nothing to wake for otherwise.
    pub(super) fn hint_deadline(&self) -> Option<std::time::Instant> {
        match (&self.mode, self.config.keys.hints) {
            (Mode::Prefix(since), Hints::Delay) if !self.hints_due(*since) => {
                Some(*since + self.config.keys.hint_delay)
            }
            _ => None,
        }
    }

    /// Whether the key-hints dialog should be up, `since` the prefix was pressed.
    pub(super) fn hints_due(&self, since: std::time::Instant) -> bool {
        match self.config.keys.hints {
            Hints::Always => true,
            Hints::Off => false,
            Hints::Delay => since.elapsed() >= self.config.keys.hint_delay,
        }
    }

    fn draw_sidebar(&mut self, f: &mut ratatui::Frame, area: Rect) {
        let tree = self.tree_rows("");
        let agents = self.sidebar_agents();
        let view = sidebar::View {
            tree: &tree,
            agents: &agents,
            config: &self.config.sidebar,
            theme: &self.theme,
            indicators: self.config.ui.indicators,
            collapsed: self.sidebar_mode == SidebarMode::Collapsed,
            boxed: self.config.sidebar.style == crate::config::SidebarStyle::Box,
            cursor: self
                .sidebar_focus
                .as_ref()
                .filter(|c| c.agent.is_none())
                .and_then(|c| c.at.clone()),
            agent_cursor: self.sidebar_focus.as_ref().and_then(|c| c.agent.clone()),
            focused: self.sidebar_focus.is_some() || self.dragging,
        };
        self.hits.sidebar = view.render(area, f.buffer_mut());
    }

    pub(super) fn draw_status_line(&mut self, f: &mut ratatui::Frame, bar: Rect) {
        let t = self.theme;
        let base = Style::new().bg(t.surface).fg(t.fg);
        f.buffer_mut().set_style(bar, base);
        self.hits.tabs.clear();
        // The mode badge first: NORMAL, PREFIX, SIDEBAR, … (configurable; "" hides it).
        let (label, color) = self.mode_badge();
        let mut lead = Vec::new();
        if !label.is_empty() {
            let style = Style::new().bg(color).fg(t.accent_fg).add_modifier(Modifier::BOLD);
            lead.push(Span::styled(format!(" {label} "), style));
            lead.push(Span::styled(" ", base));
        }
        let lead_w: u16 = lead.iter().map(|s| s.content.width() as u16).sum();
        if let Mode::Prompt(p) = &self.mode {
            let mut spans = lead;
            spans.push(Span::styled(
                format!("{} ", p.label),
                Style::new().fg(t.accent_fg).bg(t.prefix),
            ));
            spans.push(Span::raw(format!(" {}", p.text)));
            f.render_widget(Paragraph::new(Line::from(spans)).style(base), bar);
            return;
        }
        f.render_widget(Paragraph::new(Line::from(lead)).style(base), bar);
        let bar = Rect::new(
            bar.x + lead_w.min(bar.width),
            bar.y,
            bar.width.saturating_sub(lead_w),
            1,
        );

        // Right side first, so the tabs know how much room they have.
        let right = match (&self.note, self.conn) {
            (Some((n, at)), _) if at.elapsed() < super::NOTE_FOR => n.clone(),
            (_, Conn::Reconnecting) => "core: reconnecting…".into(),
            _ => {
                self.note = None;
                String::new()
            }
        };
        let right_w = if right.is_empty() { 0 } else { right.width() as u16 + 2 };

        let (ws_id, win_id) = self.active().unwrap_or_default();
        let mut x = bar.x;
        let put = |f: &mut ratatui::Frame, x: &mut u16, text: &str, style: Style| -> Rect {
            let w = (text.width() as u16).min(bar.right().saturating_sub(*x));
            let r = Rect::new(*x, bar.y, w, 1);
            f.render_widget(Paragraph::new(text.to_string()).style(style), r);
            *x += w;
            r
        };
        let Some(s) = self.doc.workspace(&ws_id).cloned() else {
            put(f, &mut x, &format!(" {right} "), Style::new().fg(t.muted).bg(t.surface));
            return;
        };

        // Window tabs, scrolled to keep the active one in view, then a "+".
        let base = self.config.keys.preset.base_index();
        let tabs: Vec<Tab> = s
            .windows
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let active = w.id == win_id;
                let name = w.name.clone().unwrap_or_else(|| self.pane_name(&w.focus));
                let flag = if w.zoomed {
                    "Z"
                } else if active {
                    "*"
                } else if s.last.as_deref() == Some(&w.id) {
                    "-"
                } else {
                    ""
                };
                let style = if active {
                    Style::new().bg(t.accent).fg(t.accent_fg).add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(t.muted).bg(t.surface)
                };
                // Inside the tab, on its background: the active tab's block takes it in too.
                let dot = self.window_state(w).map(|a| {
                    (
                        status::glyph(a, self.config.ui.indicators).to_string(),
                        style.fg(status::color(a, &t)),
                    )
                });
                Tab {
                    label: format!(" {}:{name}{flag}", i + base),
                    style,
                    dot,
                    id: w.id.clone(),
                }
            })
            .collect();
        let widths: Vec<u16> = tabs
            .iter()
            .map(|t| t.label.width() as u16 + 1 + t.dot.as_ref().map_or(0, |(g, _)| g.width() as u16 + 1))
            .collect();
        // Tabs come first: the right side only reserves a short minimum.
        let reserve = right_w.min(14);
        let avail = bar.right().saturating_sub(x).saturating_sub(reserve + 3);
        let active_i = s.windows.iter().position(|w| w.id == win_id).unwrap_or(0);
        let (start, end) = sidebar::visible_tabs(&widths, active_i, avail);
        if start > 0 {
            put(f, &mut x, "‹", Style::new().fg(t.muted).bg(t.surface));
        }
        for tab in &tabs[start..end] {
            let mut r = put(f, &mut x, &tab.label, tab.style);
            if let Some((g, gs)) = &tab.dot {
                r.width += put(f, &mut x, " ", tab.style).width;
                r.width += put(f, &mut x, g, *gs).width;
            }
            r.width += put(f, &mut x, " ", tab.style).width;
            self.hits.tabs.push((r, TabHit::Window(tab.id.clone())));
        }
        if end < tabs.len() {
            put(f, &mut x, "›", Style::new().fg(t.muted).bg(t.surface));
        }
        let plus = put(f, &mut x, " + ", Style::new().fg(t.muted).bg(t.surface));
        self.hits.tabs.push((plus, TabHit::NewWindow));

        // Usage, right-aligned before the message, in what the tabs leave (never over them).
        let right_x = bar.right().saturating_sub(right_w.min(bar.right().saturating_sub(x)));
        let free = right_x.saturating_sub(x + 2) as usize;
        let placed = self.usage_placed(free);
        if !placed.is_empty() {
            // Blocks, each on its own colours, a column apart.
            use crate::usage::{GAP, PAD};
            let total: usize = placed.iter().map(|(t, ..)| t.width() + PAD).sum::<usize>() + GAP * (placed.len() - 1);
            let mut ux = right_x.saturating_sub(total as u16 + 1);
            let start = ux;
            for (i, (text, stale, style)) in placed.iter().enumerate() {
                if i > 0 {
                    put(f, &mut ux, &" ".repeat(GAP), Style::new().bg(t.surface));
                }
                let style = if *stale {
                    style.add_modifier(Modifier::DIM)
                } else {
                    *style
                };
                put(f, &mut ux, &format!(" {text} "), style);
            }
            self.hits
                .tabs
                .push((Rect::new(start, bar.y, ux - start, 1), TabHit::Usage));
        }

        // Right-aligned in what the tabs left: the message, cut to fit.
        let room = bar.right().saturating_sub(x);
        let right_w = right_w.min(room);
        let mut rx = bar.right().saturating_sub(right_w).max(x);
        let fit = bar.right().saturating_sub(rx) as usize;
        if !right.is_empty() {
            let mut text = format!(" {right} ");
            if text.width() > fit && fit > 1 {
                text = text.chars().take(fit - 1).collect::<String>() + "…";
            }
            put(f, &mut rx, &text, Style::new().fg(t.muted).bg(t.surface));
        }
    }

    /// What the mode badge says, and its colour.
    pub(super) fn mode_badge(&self) -> (String, ratatui::style::Color) {
        let (m, t) = (&self.config.mode, &self.theme);
        match &self.mode {
            Mode::Prefix(_) => (m.prefix.clone(), t.mode_prefix),
            // Repeat keys still work without the prefix: the prefix badge, until the window lapses.
            Mode::Repeat(until) if std::time::Instant::now() < *until => (m.prefix.clone(), t.mode_prefix),
            Mode::Chooser(_) => (m.choose.clone(), t.mode_dialog),
            Mode::Prompt(p) if matches!(p.then, super::Then::Confirm(_)) => (m.confirm.clone(), t.mode_dialog),
            Mode::Prompt(p) if matches!(p.then, super::Then::Command { .. }) => (m.command.clone(), t.mode_dialog),
            Mode::Prompt(_) => (m.rename.clone(), t.mode_dialog),
            Mode::Keys(_) => (m.keys.clone(), t.mode_dialog),
            Mode::Menu(..) => (m.menu.clone(), t.mode_dialog),
            Mode::Copy(..) => (m.copy.clone(), t.mode_prefix),
            Mode::Popup => (m.popup.clone(), t.mode_dialog),
            Mode::Worktrees(_) | Mode::Machines(_) => (m.choose.clone(), t.mode_dialog),
            Mode::Dialog(d) => {
                use super::dialogs::Badge;
                let label = match d.does.badge() {
                    Badge::New => &m.new,
                    Badge::Rename => &m.rename,
                    Badge::Confirm => &m.confirm,
                    Badge::Info => &m.usage,
                };
                (label.clone(), t.mode_dialog)
            }
            Mode::Panes(_) => (m.panes.clone(), t.mode_prefix),
            Mode::Table(name) if name == "resize" => (m.resize.clone(), t.mode_prefix),
            Mode::Table(name) => (name.to_uppercase(), t.mode_prefix),
            Mode::Normal | Mode::Repeat(_) if self.sidebar_focus.is_some() => (m.sidebar.clone(), t.mode_sidebar),
            Mode::Normal | Mode::Repeat(_) => (m.normal.clone(), t.mode_normal),
        }
    }

    /// Each pane's tab, on the line above it (inside `outer`, the panes' whole area).
    fn draw_pane_titles(&self, buf: &mut ratatui::buffer::Buffer, focus: &str, outer: Rect) {
        // Titles sit by the border: where the panes are before their inset.
        let rects = &self.titled(self.frames());
        let inset = self.config.panes.inset;
        let mode = self.config.panes.title;
        if mode == crate::config::PaneTitle::Off {
            return;
        }
        let home = crate::paths::home().display().to_string();
        let crumbs = self.config.panes.title_shape == crate::config::TitleShape::Breadcrumbs;
        let frames: HashMap<String, Rect> = if crumbs {
            self.frames().into_iter().collect()
        } else {
            HashMap::new()
        };
        let boxed = self.tiling().boxes;
        for (id, r) in rects {
            if crumbs {
                // Its own box: the crumbs on the frame's first row, a line under them.
                let Some(f) = frames.get(id) else { continue };
                let top = f.y + u16::from(boxed);
                let rows = r.y.saturating_sub(top);
                if rows == 0 {
                    continue;
                }
                let t = self.terminals.get(id);
                let vt = self.panes.get(id).and_then(|p| p.title.as_deref());
                if let Some(c) = crate::pane_title::crumbs(mode, t, vt, &home) {
                    let circle =
                        t.and_then(|t| crate::pane_title::circle(status::activity(t), spin_frame(), &self.theme));
                    // In line with the terminal's text.
                    let ix = inset.min(r.width / 4);
                    crate::pane_title::render_crumbs(
                        buf,
                        (r.x + ix, top, r.width.saturating_sub(2 * ix)),
                        &c,
                        id == focus,
                        circle,
                        &self.theme,
                    );
                }
                if rows >= 2 {
                    let color = if id == focus {
                        self.theme.border_active
                    } else {
                        self.theme.border
                    };
                    draw_divider(buf, top + 1, r.x.saturating_sub(1), r.right(), color);
                }
                continue;
            }
            // No line above it: only when there was no room to give one (tiny panes).
            if r.y <= outer.y {
                continue;
            }
            let t = self.terminals.get(id);
            let vt = self.panes.get(id).and_then(|p| p.title.as_deref());
            let Some(text) = crate::pane_title::text(mode, t, vt, &home) else {
                continue;
            };

            if self.config.panes.title_shape == crate::config::TitleShape::Line {
                let circle = t.and_then(|t| crate::pane_title::circle(status::activity(t), spin_frame(), &self.theme));
                crate::pane_title::render_line(
                    buf,
                    (r.x + 1, r.y - 1, r.width.saturating_sub(1)),
                    &text,
                    id == focus,
                    circle,
                    &self.theme,
                );
                continue;
            }
            let glyph = t.and_then(status::agent).map(|a| {
                (
                    status::glyph(a, self.config.ui.indicators),
                    status::color(a, &self.theme),
                )
            });
            crate::pane_title::Tab {
                text: &text,
                glyph,
                focused: id == focus,
                shape: self.config.panes.title_shape,
            }
            .render(buf, r.x + 1, r.y - 1, r.width.saturating_sub(2), &self.theme);
        }
    }

    /// Whether a pane on screen has a working agent: its circle spins, so redraw now and then.
    pub(super) fn spinning(&self) -> bool {
        matches!(
            self.config.panes.title_shape,
            crate::config::TitleShape::Line | crate::config::TitleShape::Breadcrumbs
        ) && self.config.panes.title != crate::config::PaneTitle::Off
            && self.rects().iter().any(|(id, _)| {
                self.terminals
                    .get(id)
                    .is_some_and(|t| crate::pane_title::spins(status::activity(t)))
            })
    }

    /// What a pane is called: the name you gave it, else what it runs.
    pub(super) fn pane_name(&self, addr: &str) -> String {
        self.terminals
            .get(addr)
            .and_then(|t| t.labels.get(crate::model::L_NAME))
            .filter(|n| !n.is_empty())
            .cloned()
            .unwrap_or_else(|| self.auto_pane_name(addr))
    }

    /// What a pane runs (its title, the program in front, …), whatever it's named.
    pub(super) fn auto_pane_name(&self, addr: &str) -> String {
        if let Some(p) = self.panes.get(addr) {
            return p.name();
        }
        let t = self.terminals.get(addr);
        t.and_then(|t| t.title.clone().filter(|s| !s.is_empty()))
            .or_else(|| t.and_then(|t| t.foreground.as_ref().map(|f| f.name.clone())))
            .or_else(|| t.and_then(|t| t.argv.first().cloned()))
            .unwrap_or_else(|| addr.to_string())
    }

    /// The most urgent agent status among a window's panes.
    fn window_state(&self, w: &Window) -> Option<status::Agent> {
        status::rollup(w.root.panes().into_iter().filter_map(|p| self.terminals.get(p)))
    }
}

fn pane_message(p: &Pane, conn: Conn) -> Option<String> {
    match &p.state {
        State::Live => None,
        State::Attaching => Some(format!("attaching {}…", p.addr)),
        State::Exited(r) => Some(format!(
            "{} exited{}",
            p.addr,
            r.exit_code.map(|c| format!(" ({c})")).unwrap_or_default()
        )),
        State::Recovering(why) => Some(format!(
            "{} lost its program ({why}); waiting for its new shell…",
            p.addr
        )),
        State::Myself => Some(format!(
            "{}: corgi runs in this terminal, so it isn't shown here",
            p.addr
        )),
        State::Offline if conn == Conn::Reconnecting => Some("herdr-core connection lost · reconnecting…".into()),
        State::Offline => Some("not attached".into()),
    }
}

/// tmux-style borders: lines with proper junctions, the active pane's edges highlighted.
fn draw_separators(
    buf: &mut ratatui::buffer::Buffer,
    seps: &[crate::layout::Separator],
    active: Option<Rect>,
    theme: &Theme,
) {
    let mut cells: HashSet<(u16, u16)> = HashSet::new();
    for s in seps {
        for y in s.rect.top()..s.rect.bottom() {
            for x in s.rect.left()..s.rect.right() {
                cells.insert((x, y));
            }
        }
    }
    let touches = |x: u16, y: u16| {
        active.is_some_and(|r| {
            let near_x = x + 1 >= r.left() && x <= r.right();
            let near_y = y + 1 >= r.top() && y <= r.bottom();
            near_x && near_y
        })
    };
    for &(x, y) in &cells {
        let up = y > 0 && cells.contains(&(x, y - 1));
        let down = cells.contains(&(x, y + 1));
        let left = x > 0 && cells.contains(&(x - 1, y));
        let right = cells.contains(&(x + 1, y));
        let ch = match (up, down, left, right) {
            (true, true, true, true) => "┼",
            (false, true, true, true) => "┬",
            (true, false, true, true) => "┴",
            (true, true, false, true) => "├",
            (true, true, true, false) => "┤",
            (_, _, false, false) => "│",
            _ => "─",
        };
        let color = if touches(x, y) {
            theme.border_active
        } else {
            theme.border
        };
        if let Some(c) = buf.cell_mut((x, y)) {
            c.set_symbol(ch).set_style(Style::new().fg(color));
        }
    }
}

/// The rounded box around the panes. Separators meeting its edge join it (┬ ┴ ├ ┤), and the
/// edges beside the focused pane light up, like the separators.
fn draw_box(
    buf: &mut ratatui::buffer::Buffer,
    outer: Rect,
    seps: &[crate::layout::Separator],
    active: Option<Rect>,
    theme: &Theme,
) {
    let sep = |x: u16, y: u16| {
        seps.iter()
            .any(|s| s.rect.contains(ratatui::layout::Position::new(x, y)))
    };
    let near = |x: u16, y: u16| {
        active.is_some_and(|r| x + 1 >= r.left() && x <= r.right() && y + 1 >= r.top() && y <= r.bottom())
    };
    let (l, r, t, b) = (outer.left(), outer.right() - 1, outer.top(), outer.bottom() - 1);
    let [tl, tr, bl, br] = theme.corners();
    for y in t..=b {
        for x in l..=r {
            let ch = if (x, y) == (l, t) {
                tl
            } else if (x, y) == (r, t) {
                tr
            } else if (x, y) == (l, b) {
                bl
            } else if (x, y) == (r, b) {
                br
            } else if y == t {
                if sep(x, y + 1) { "┬" } else { "─" }
            } else if y == b {
                if sep(x, y - 1) { "┴" } else { "─" }
            } else if x == l {
                if sep(x + 1, y) { "├" } else { "│" }
            } else if x == r {
                if sep(x - 1, y) { "┤" } else { "│" }
            } else {
                continue;
            };
            let color = if near(x, y) { theme.border_active } else { theme.border };
            if let Some(c) = buf.cell_mut((x, y)) {
                c.set_symbol(ch).set_style(Style::new().fg(color));
            }
        }
    }
}

/// The blank cells between a pane's border and its terminal take the background of the
/// terminal's nearest cell (as Ghostty's `window-padding-color = extend`): a program that paints
/// its background (nvim, less) fills the pane to its border.
fn extend_into_inset(buf: &mut ratatui::buffer::Buffer, r: Rect, ix: u16, iy: u16) {
    if r.width == 0 || r.height == 0 {
        return;
    }
    let bg = |buf: &ratatui::buffer::Buffer, x: u16, y: u16| buf.cell((x, y)).map(|c| c.bg);
    let paint = |buf: &mut ratatui::buffer::Buffer, x: u16, y: u16, color: Option<ratatui::style::Color>| {
        if let (Some(c), Some(color)) = (buf.cell_mut((x, y)), color) {
            c.set_symbol(" ").set_bg(color);
        }
    };
    let (left, right) = (r.x.saturating_sub(ix), r.right() + ix);
    for y in r.top()..r.bottom() {
        let (l, rr) = (bg(buf, r.x, y), bg(buf, r.right() - 1, y));
        for x in left..r.x {
            paint(buf, x, y, l);
        }
        for x in r.right()..right {
            paint(buf, x, y, rr);
        }
    }
    // Rows above and below, the corners included, from the first and last rows.
    for k in 1..=iy {
        for x in left..right {
            let (top, bottom) = (bg(buf, x, r.y), bg(buf, x, r.bottom() - 1));
            if r.y >= k {
                paint(buf, x, r.y - k, top);
            }
            paint(buf, x, r.bottom() - 1 + k, bottom);
        }
    }
}

/// The line under a pane's breadcrumbs, from column `x0` to `x1` (its frame's edges): joined to
/// the border where there is one (`├`, `┤`, `┼` where two meet), plain elsewhere.
fn draw_divider(buf: &mut ratatui::buffer::Buffer, y: u16, x0: u16, x1: u16, color: ratatui::style::Color) {
    for x in x0..=x1 {
        let Some(c) = buf.cell_mut((x, y)) else { continue };
        let edge = x == x0 || x == x1;
        let sym = match (edge, c.symbol()) {
            (false, _) => "─",
            (true, "│") if x == x0 => "├",
            (true, "│") => "┤",
            (true, "├" | "┤" | "┼") => "┼",
            // No border at the edge (plain panes): leave it.
            _ => continue,
        };
        c.set_symbol(sym).set_style(Style::new().fg(color));
    }
}

/// How long each step of the working spinner shows.
pub(super) const SPIN_STEP: std::time::Duration = std::time::Duration::from_millis(100);

/// The working spinner's step now.
fn spin_frame() -> usize {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    (ms / SPIN_STEP.as_millis()) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Dir, Node};
    use ratatui::buffer::Buffer;

    #[test]
    fn pane_box_joins_separators_and_lights_the_focused_side() {
        let theme = crate::theme::builtin("terminal").unwrap();
        let mut n = Node::pane("t1");
        n.split("t1", "t2".into(), Dir::Horizontal, 50);
        let outer = Rect::new(0, 0, 13, 5);
        let inner = Rect::new(1, 1, 11, 3);
        let (rects, seps) = n.geometry(inner);
        let mut buf = Buffer::empty(outer);
        let active = rects.iter().find(|(id, _)| id == "t2").map(|(_, r)| *r);
        draw_separators(&mut buf, &seps, active, &theme);
        draw_box(&mut buf, outer, &seps, active, &theme);
        let row = |y: u16| (0..13).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>();
        assert_eq!(row(0), "┌─────┬─────┐");
        assert_eq!(row(2), "│     │     │");
        assert_eq!(row(4), "└─────┴─────┘");
        assert_eq!(buf[(1, 0)].fg, theme.border, "above t1: not lit");
        assert_eq!(buf[(9, 0)].fg, theme.border_active, "above t2");
        assert_eq!(buf[(12, 2)].fg, theme.border_active, "right of t2");
    }

    #[test]
    fn rounded_corners_when_asked() {
        let theme = crate::theme::Theme {
            rounded: true,
            ..crate::theme::builtin("terminal").unwrap()
        };
        let outer = Rect::new(0, 0, 6, 3);
        let mut buf = Buffer::empty(outer);
        draw_box(&mut buf, outer, &[], None, &theme);
        let row = |y: u16| (0..6).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>();
        assert_eq!((row(0), row(2)), ("╭────╮".to_string(), "╰────╯".to_string()));
    }

    #[test]
    fn separators_join_and_highlight_the_active_pane() {
        let theme = crate::theme::builtin("terminal").unwrap();
        let mut n = Node::pane("t1");
        n.split("t1", "t2".into(), Dir::Horizontal, 50);
        n.split("t2", "t3".into(), Dir::Vertical, 50);
        let area = Rect::new(0, 0, 11, 5);
        let (rects, seps) = n.geometry(area);
        let mut buf = Buffer::empty(area);
        let active = rects.iter().find(|(id, _)| id == "t3").map(|(_, r)| *r);
        draw_separators(&mut buf, &seps, active, &theme);
        let row = |y: u16| (0..11).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>();
        assert_eq!(row(2), "     ├─────");
        assert_eq!(row(0), "     │     ");
        assert_eq!(buf[(5, 0)].fg, theme.border, "t1/t2 edge: not active");
        assert_eq!(buf[(5, 4)].fg, theme.border_active, "t3's left edge");
        assert_eq!(buf[(8, 2)].fg, theme.border_active, "t3's top edge");
    }
}
