//! The sidebar: the workspace tree (workspaces › windows › panes, foldable)
//! on top, every agent across them in the bottom part (or straight below). Rows come from templates in the
//! config. The row formatter is shared with the tree chooser (C-b w).
//! Rendering is pure: it returns where each row landed, for mouse clicks.

use crate::config::{AgentSort, AgentsPlace, Indicators, Sidebar};
use crate::status::{self, Agent};
use crate::theme::Theme;
use crate::tree::{Row, Target};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};

#[derive(Clone, Debug, Default)]
pub struct AgentRow {
    pub ws: String,
    pub win: String,
    pub pane: String,
    pub row: Row,
    /// Position in workspace/window order, for `grouped`.
    pub order: usize,
}

impl AgentRow {
    /// Its pane: where clicking or Enter on it goes.
    pub fn target(&self) -> Target {
        Target::Pane {
            ws: self.ws.clone(),
            win: self.win.clone(),
            pane: self.pane.clone(),
        }
    }
}

/// The agents in the order the sidebar lists them (`sidebar.agent_sort`).
pub fn sort_agents(agents: &mut [AgentRow], sort: AgentSort) {
    match sort {
        AgentSort::Grouped => agents.sort_by_key(|a| a.order),
        AgentSort::Priority => agents.sort_by_key(|a| (std::cmp::Reverse(a.row.state), a.order)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hit {
    /// Show it.
    Go(Target),
    /// Open or close it (the ▾/▸ column).
    Fold(Target),
}

/// Fill a row template. Tokens with no value vanish along with doubled spaces.
fn expand(template: &str, state: Option<Span<'static>>, get: &dyn Fn(&str) -> Option<String>) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut text = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        text.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            text.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let token = &rest[start + 1..start + end];
        if token == "state" {
            if let Some(s) = &state {
                spans.push(Span::raw(std::mem::take(&mut text)));
                spans.push(s.clone());
            }
        } else if let Some(v) = get(token) {
            text.push_str(&v);
        }
        rest = &rest[start + end + 1..];
    }
    text.push_str(rest);
    spans.push(Span::raw(text));
    for s in spans.iter_mut() {
        let mut t = s.content.to_string();
        while t.contains("  ") {
            t = t.replace("  ", " ");
        }
        s.content = t.into();
    }
    if let Some(first) = spans.iter_mut().find(|s| !s.content.is_empty()) {
        first.content = first.content.trim_start().to_string().into();
    }
    spans.retain(|s| !s.content.is_empty());
    spans
}

pub fn glyph(state: Option<Agent>, indicators: Indicators, t: &Theme) -> Option<Span<'static>> {
    state.map(|a| Span::styled(status::glyph(a, indicators), Style::new().fg(status::color(a, t))))
}

/// The guide lines in front of each row, showing what's nested in what: `├─` for a row with
/// more siblings below it, `└─` for the last, `│ ` where an ancestor's siblings continue.
/// Two columns a level, like plain indent; top-level rows have none.
pub fn guides(depths: &[u8]) -> Vec<String> {
    // Backwards: is there a sibling further down (a row at the same depth before a shallower one)?
    let mut last = vec![true; depths.len()];
    let mut later: Vec<bool> = Vec::new();
    for (k, &d) in depths.iter().enumerate().rev() {
        let d = d as usize;
        later.resize(d + 1, false);
        last[k] = !later[d];
        later[d] = true;
    }
    // Forwards: which ancestors' lines run past this row.
    let mut open: Vec<bool> = Vec::new();
    depths
        .iter()
        .zip(&last)
        .map(|(&d, &last)| {
            let d = d as usize;
            open.resize(d + 1, false);
            let mut g = String::new();
            for &o in open.iter().take(d).skip(1) {
                g.push_str(if o { "│ " } else { "  " });
            }
            if d > 0 {
                g.push_str(if last { "└─" } else { "├─" });
            }
            open[d] = !last;
            g
        })
        .collect()
}

/// A tree row as text: its guide (from [`guides`]), fold arrow, then its template. Returns the
/// spans and the column (relative) of the fold arrow, if it has one.
pub fn tree_line(
    row: &Row,
    guide: &str,
    cfg: &Sidebar,
    t: &Theme,
    ind: Indicators,
) -> (Vec<Span<'static>>, Option<u16>) {
    let indent = Span::styled(guide.to_string(), Style::new().fg(t.muted));
    let col = guide.chars().count() as u16;
    let arrow = if row.foldable {
        if row.open { "▾" } else { "▸" }
    } else if guide.is_empty() {
        " "
    } else {
        // A leaf: its guide runs up to it.
        "─"
    };
    let template = match &row.target {
        Some(Target::Machine(_)) => {
            // A machine: its line's state (this machine has none), its name, and what's wrong.
            let mut spans = vec![
                indent.clone(),
                Span::styled(arrow, Style::new().fg(t.muted)),
                Span::raw(" "),
            ];
            let name = row.machine.clone().unwrap_or_default();
            match &row.link {
                Some(l) => {
                    spans.push(Span::styled(format!("{} ", l.glyph()), Style::new().fg(l.color(t))));
                    spans.push(Span::styled(name, Style::new().add_modifier(Modifier::BOLD)));
                    if !l.is_up() {
                        spans.push(Span::styled(format!(" {}", l.word()), Style::new().fg(l.color(t))));
                    }
                }
                None => {
                    spans.push(Span::styled("⌂ ", Style::new().fg(t.muted)));
                    spans.push(Span::styled(name, Style::new().add_modifier(Modifier::BOLD)));
                }
            }
            if let Some(g) = glyph(row.state, ind, t) {
                spans.push(Span::raw(" "));
                spans.push(g);
            }
            return (spans, row.foldable.then_some(col));
        }
        Some(Target::Workspace(_)) => &cfg.workspace_row,
        Some(Target::Window { .. }) => &cfg.window_row,
        _ => &cfg.pane_row,
    };
    let get = |k: &str| match k {
        "workspace" => Some(row.workspace.clone()),
        "branch" => row.branch.clone(),
        "machine" => row.machine.clone(),
        "window" => Some(row.window.clone()),
        "pane" => Some(row.pane.clone()),
        "agent" => Some(row.agent.clone()),
        "title" => row.title.clone(),
        k => k.strip_prefix("label:").and_then(|l| row.labels.get(l).cloned()),
    };
    let mut spans = vec![indent, Span::styled(arrow, Style::new().fg(t.muted)), Span::raw(" ")];
    // A worktree of the workspace above it.
    if row.worktree && matches!(row.target, Some(Target::Workspace(_))) {
        spans.push(Span::styled("↳ ", Style::new().fg(t.muted)));
    }
    let mut body = expand(template, glyph(row.state, ind, t), &get);
    // The branch reads as secondary.
    if let Some(b) = &row.branch
        && let Some(last) = body.last_mut()
        && last.content.ends_with(b.as_str())
    {
        let text = last.content.to_string();
        *last = Span::raw(text[..text.len() - b.len()].to_string());
        body.push(Span::styled(b.clone(), Style::new().fg(t.muted)));
    }
    spans.extend(body);
    (spans, row.foldable.then_some(col))
}

pub struct View<'a> {
    pub tree: &'a [Row],
    pub agents: &'a [AgentRow],
    pub config: &'a Sidebar,
    pub theme: &'a Theme,
    pub indicators: Indicators,
    /// Icons only.
    pub collapsed: bool,
    /// A rounded box around it (`sidebar.style = "box"`), or a line on its right edge.
    pub boxed: bool,
    /// The keyboard cursor's row, when the sidebar has focus.
    pub cursor: Option<Target>,
    /// The keyboard cursor's agent (its pane), when it's in the agents list instead.
    pub agent_cursor: Option<Target>,
    /// Has the keyboard (or is being resized): its border shows it.
    pub focused: bool,
}

impl View<'_> {
    pub fn render(self, area: Rect, buf: &mut Buffer) -> Vec<(Rect, Hit)> {
        let t = self.theme;
        let mut hits = Vec::new();
        if area.width < 2 || area.height == 0 {
            return hits;
        }
        let surface = Style::new().bg(t.surface).fg(t.fg);
        let header = Style::new().fg(t.muted).bg(t.surface).add_modifier(Modifier::BOLD);
        let muted = Style::new().fg(t.muted).bg(t.surface);
        let border = Style::new()
            .fg(if self.focused { t.border_active } else { t.border })
            .bg(t.surface);
        buf.set_style(area, surface);
        let boxed = self.boxed && area.width >= 3 && area.height >= 3;

        // `agents = "split"`: the tree on top, the agents in the bottom part, each in its own
        // box. Too short for two: the agents follow the tree, as with `"below"`.
        let split = self.config.agents == AgentsPlace::Split && area.height >= 10;
        let (top, bottom) = if split {
            let h = (area.height as u32 * self.config.agents_height as u32 / 100) as u16;
            let h = h.clamp(4, area.height - 4);
            (
                Rect::new(area.x, area.y, area.width, area.height - h),
                Some(Rect::new(area.x, area.bottom() - h, area.width, h)),
            )
        } else {
            (area, None)
        };
        // A section's box (or, unboxed, the line on its right edge); returns what's inside.
        let in_row = self.config.header == crate::config::SidebarHeader::Row;
        let frame = |buf: &mut Buffer, r: Rect, name: &str| -> Rect {
            if boxed {
                let title = if self.collapsed || in_row {
                    Line::default()
                } else {
                    Line::from(Span::styled(format!(" {name} "), header))
                };
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(t.border_type())
                    .border_style(border)
                    .title(title);
                let inner = block.inner(r);
                block.render(r, buf);
                // The name on a row of its own, a line under it joined to the box's sides.
                if in_row && !self.collapsed && inner.height >= 3 {
                    buf.set_stringn(
                        inner.x + 1,
                        inner.y,
                        name,
                        inner.width.saturating_sub(1) as usize,
                        header,
                    );
                    let y = inner.y + 1;
                    for x in r.x..r.right() {
                        let sym = if x == r.x {
                            "├"
                        } else if x + 1 == r.right() {
                            "┤"
                        } else {
                            "─"
                        };
                        if let Some(c) = buf.cell_mut((x, y)) {
                            c.set_symbol(sym).set_style(border);
                        }
                    }
                    return Rect::new(inner.x, inner.y + 2, inner.width, inner.height - 2);
                }
                inner
            } else {
                for y in r.top()..r.bottom() {
                    if let Some(c) = buf.cell_mut((r.right() - 1, y)) {
                        c.set_symbol("│").set_style(border);
                    }
                }
                Rect::new(r.x, r.y, r.width - 1, r.height)
            }
        };
        let tree_area = frame(buf, top, "Workspaces");
        let agent_area = bottom.map(|b| frame(buf, b, "Agents"));

        // One row of `within`: draw it, record what clicking it does.
        let mut put = |buf: &mut Buffer,
                       within: Rect,
                       y: &mut u16,
                       spans: Vec<Span<'static>>,
                       style: Style,
                       go: Option<Target>,
                       fold: Option<u16>| {
            if *y >= within.bottom() {
                return;
            }
            let r = Rect::new(within.x, *y, within.width, 1);
            buf.set_style(r, style);
            Line::from(spans)
                .style(style)
                .render(Rect::new(r.x + 1, r.y, r.width.saturating_sub(1), 1), buf);
            if let Some(target) = go {
                if let Some(col) = fold {
                    hits.push((Rect::new(r.x + 1 + col, r.y, 1, 1), Hit::Fold(target.clone())));
                }
                hits.push((r, Hit::Go(target)));
            }
            *y += 1;
        };

        let mut y = tree_area.y;
        if !self.collapsed && !boxed {
            put(
                buf,
                tree_area,
                &mut y,
                vec![Span::raw("Workspaces")],
                header,
                None,
                None,
            );
        }
        let rows: Vec<&Row> = self
            .tree
            .iter()
            .filter(|r| !self.collapsed || matches!(r.target, Some(Target::Workspace(_))))
            .collect();
        // Longer than its part: scrolled to keep the cursor (or where you are) in sight.
        let room = tree_area.bottom().saturating_sub(y) as usize;
        let keep = rows
            .iter()
            .position(|r| self.cursor.is_some() && r.target == self.cursor)
            .or_else(|| rows.iter().rposition(|r| r.current));
        let skip = if split {
            keep.map_or(0, |i| (i + 1).saturating_sub(room))
        } else {
            0
        };
        // Where you are: the deepest current row shown (the focused pane, or the window or
        // workspace it's folded into). The rows above it on the way there are bold.
        let here = rows.iter().rposition(|r| r.current);
        let guide = guides(&rows.iter().map(|r| r.depth).collect::<Vec<_>>());
        for (k, row) in rows.into_iter().enumerate().skip(skip) {
            let style = if self.cursor.is_some() && row.target == self.cursor {
                Style::new().bg(t.prefix).fg(t.accent_fg).add_modifier(Modifier::BOLD)
            } else if Some(k) == here {
                Style::new().bg(t.accent).fg(t.accent_fg).add_modifier(Modifier::BOLD)
            } else if row.current {
                surface.add_modifier(Modifier::BOLD)
            } else if row.down && !matches!(row.target, Some(Target::Machine(_))) {
                // Its machine is down: what it shows is what was last seen.
                muted
            } else {
                surface
            };
            let (spans, fold) = if self.collapsed {
                (
                    vec![glyph(row.state, self.indicators, t).unwrap_or(Span::raw("·"))],
                    None,
                )
            } else {
                tree_line(row, &guide[k], self.config, t, self.indicators)
            };
            put(buf, tree_area, &mut y, spans, style, row.target.clone(), fold);
        }

        let mut agents: Vec<AgentRow> = self.agents.to_vec();
        sort_agents(&mut agents, self.config.agent_sort);
        let within = match agent_area {
            Some(a) => {
                y = a.y;
                if !self.collapsed && !boxed {
                    put(buf, a, &mut y, vec![Span::raw("Agents")], header, None, None);
                }
                if agents.is_empty() && !self.collapsed {
                    put(buf, a, &mut y, vec![Span::raw("no agents running")], muted, None, None);
                }
                a
            }
            // Below the tree.
            None if !agents.is_empty() && boxed => {
                // A divider joined to the box: ├─ Agents ───┤ (or the name on a row of its own
                // between two dividers).
                let divider = |buf: &mut Buffer, y: u16| {
                    for x in area.left()..area.right() {
                        let sym = if x == area.left() {
                            "├"
                        } else if x + 1 == area.right() {
                            "┤"
                        } else {
                            "─"
                        };
                        if let Some(c) = buf.cell_mut((x, y)) {
                            c.set_symbol(sym).set_style(border);
                        }
                    }
                };
                let rows_ok = in_row && !self.collapsed && y + 3 < tree_area.bottom();
                if rows_ok {
                    y += 1;
                    divider(buf, y);
                    y += 1;
                    put(buf, tree_area, &mut y, vec![Span::raw("Agents")], header, None, None);
                    divider(buf, y);
                    y += 1;
                } else if y + 1 < tree_area.bottom() {
                    y += 1;
                    divider(buf, y);
                    if !self.collapsed {
                        Line::from(Span::styled(" Agents ", header))
                            .render(Rect::new(area.x + 2, y, area.width.saturating_sub(4), 1), buf);
                    }
                    y += 1;
                }
                tree_area
            }
            None => {
                if !agents.is_empty() {
                    y += 1;
                    if !self.collapsed {
                        put(buf, tree_area, &mut y, vec![Span::raw("Agents")], header, None, None);
                    }
                }
                tree_area
            }
        };
        // More than fit: the last row says how many more, scrolled to keep the cursor in sight.
        let room = within.bottom().saturating_sub(y) as usize;
        let at = self
            .agent_cursor
            .as_ref()
            .and_then(|c| agents.iter().position(|a| a.target() == *c));
        let (start, shown) = agent_window(agents.len(), room, at);
        let more = agents.len() - start - shown;
        for a in &agents[start..start + shown] {
            let spans = if self.collapsed {
                vec![glyph(a.row.state, self.indicators, t).unwrap_or(Span::raw("·"))]
            } else {
                let r = &a.row;
                let get = |k: &str| match k {
                    "agent" => Some(r.agent.clone()),
                    "workspace" => Some(r.workspace.clone()),
                    "window" => Some(r.window.clone()),
                    "pane" => Some(r.pane.clone()),
                    "title" => r.title.clone(),
                    k => k.strip_prefix("label:").and_then(|l| r.labels.get(l).cloned()),
                };
                expand(&self.config.agent_row, glyph(r.state, self.indicators, t), &get)
            };
            let target = a.target();
            let style = if self.agent_cursor.as_ref() == Some(&target) {
                Style::new().bg(t.prefix).fg(t.accent_fg).add_modifier(Modifier::BOLD)
            } else {
                surface
            };
            put(buf, within, &mut y, spans, style, Some(target), None);
        }
        if more > 0 && room > 0 {
            let text = if self.collapsed {
                "…".to_string()
            } else {
                format!("+{more} more")
            };
            put(buf, within, &mut y, vec![Span::raw(text)], muted, None, None);
        }
        hits
    }
}

/// Which of `n` agents to show in `room` rows, keeping `at` (the cursor) in sight: `(start, count)`.
/// When some don't fit, the last row is left for "+N more" (those below).
fn agent_window(n: usize, room: usize, at: Option<usize>) -> (usize, usize) {
    if n <= room {
        return (0, n);
    }
    let fit = room.saturating_sub(1);
    let start = at.map_or(0, |i| (i + 1).saturating_sub(fit));
    if start + fit >= n {
        // Scrolled to the end: nothing below, so the "more" row isn't needed.
        (n - room, room)
    } else {
        (start, fit)
    }
}

/// Which window tabs fit in `avail` columns, always including `active`. `(start, end)`, end exclusive.
/// When not all fit, one column on each clipped side is left for a `‹`/`›` marker.
pub fn visible_tabs(widths: &[u16], active: usize, avail: u16) -> (usize, usize) {
    let total: u16 = widths.iter().sum();
    if total <= avail || widths.is_empty() {
        return (0, widths.len());
    }
    let active = active.min(widths.len() - 1);
    let (mut start, mut end) = (active, active + 1);
    let mut used = widths[active];
    let room = avail.saturating_sub(2);
    loop {
        let mut grew = false;
        if end < widths.len() && used + widths[end] <= room {
            used += widths[end];
            end += 1;
            grew = true;
        }
        if start > 0 && used + widths[start - 1] <= room {
            start -= 1;
            used += widths[start];
            grew = true;
        }
        if !grew {
            return (start, end);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::builtin;

    fn text(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn templates_drop_missing_tokens() {
        let get = |k: &str| (k == "workspace").then(|| "proj".to_string());
        let s = expand("{state} {workspace} {branch} {label:x}", None, &get);
        assert_eq!(s.iter().map(|s| s.content.as_ref()).collect::<String>(), "proj ");
    }

    fn ws(id: &str, name: &str, open: bool, current: bool) -> Row {
        Row {
            target: Some(Target::Workspace(id.into())),
            foldable: true,
            open,
            current,
            workspace: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn renders_the_tree_and_agents_and_reports_hits() {
        let theme = builtin("catppuccin-mocha").unwrap();
        let mut kelpie = ws("s1", "kelpie", true, true);
        kelpie.branch = Some("main".into());
        kelpie.state = Some(Agent::Working);
        let tree = [
            kelpie,
            Row {
                target: Some(Target::Window {
                    ws: "s1".into(),
                    win: "w1".into(),
                }),
                depth: 1,
                foldable: true,
                open: true,
                current: true,
                window: "0:edit".into(),
                state: Some(Agent::Working),
                ..Default::default()
            },
            Row {
                target: Some(Target::Pane {
                    ws: "s1".into(),
                    win: "w1".into(),
                    pane: "t3".into(),
                }),
                depth: 2,
                agent: "claude".into(),
                state: Some(Agent::Working),
                ..Default::default()
            },
            ws("s2", "api", false, false),
        ];
        let agents = [AgentRow {
            ws: "s1".into(),
            win: "w1".into(),
            pane: "t3".into(),
            row: Row {
                agent: "claude".into(),
                workspace: "kelpie".into(),
                window: "0:edit".into(),
                state: Some(Agent::Working),
                ..Default::default()
            },
            order: 0,
        }];
        let cfg = Sidebar {
            agents: AgentsPlace::Below,
            ..Default::default()
        };
        let area = Rect::new(0, 0, 28, 10);
        let mut buf = Buffer::empty(area);
        let view = View {
            tree: &tree,
            agents: &agents,
            config: &cfg,
            theme: &theme,
            indicators: Indicators::Dots,
            collapsed: false,
            boxed: false,
            cursor: None,
            agent_cursor: None,
            focused: false,
        };
        let hits = view.render(area, &mut buf);
        assert_eq!(text(&buf, 0), " Workspaces                │");
        assert_eq!(text(&buf, 1), " ▾ ● kelpie main           │");
        assert_eq!(text(&buf, 2), " └─▾ ● 0:edit              │");
        assert_eq!(text(&buf, 3), "   └── ● claude            │");
        assert_eq!(text(&buf, 4), " ▸ api                     │");
        assert_eq!(text(&buf, 6), " Agents                    │");
        assert_eq!(text(&buf, 7), " ● claude kelpie:0:edit    │");
        // Clicking the arrow folds; anywhere else on the row goes there.
        assert_eq!(
            hits[0],
            (Rect::new(1, 1, 1, 1), Hit::Fold(Target::Workspace("s1".into())))
        );
        assert_eq!(
            hits[1],
            (Rect::new(0, 1, 27, 1), Hit::Go(Target::Workspace("s1".into())))
        );
        assert_eq!(hits[2].0, Rect::new(3, 2, 1, 1), "the window's arrow, indented");

        let mut buf = Buffer::empty(Rect::new(0, 0, 3, 10));
        let view = View {
            tree: &tree,
            agents: &agents,
            config: &cfg,
            theme: &theme,
            indicators: Indicators::Dots,
            collapsed: true,
            boxed: false,
            cursor: None,
            agent_cursor: None,
            focused: false,
        };
        view.render(Rect::new(0, 0, 3, 10), &mut buf);
        assert_eq!(text(&buf, 0), " ●│", "collapsed: workspace icons only");
        assert_eq!(text(&buf, 1), " ·│");
    }

    #[test]
    fn guides_show_nesting() {
        // ws, window (2 panes), window, ws, ws (1 window)
        let g = guides(&[0, 1, 2, 2, 1, 0, 0, 1]);
        assert_eq!(g, ["", "├─", "│ ├─", "│ └─", "└─", "", "", "└─"]);
        // Under machines: an ancestor's line runs past its children.
        let g = guides(&[0, 1, 2, 3, 1, 0, 1]);
        assert_eq!(g, ["", "├─", "│ └─", "│   └─", "└─", "", "└─"]);
        assert!(guides(&[]).is_empty());
    }

    #[test]
    fn highlights_the_focused_pane_not_its_workspace() {
        let theme = builtin("catppuccin-mocha").unwrap();
        let win = |win: &str, current: bool| Row {
            target: Some(Target::Window {
                ws: "s1".into(),
                win: win.into(),
            }),
            depth: 1,
            foldable: true,
            open: true,
            current,
            window: win.into(),
            ..Default::default()
        };
        let pane = |pane: &str, current: bool| Row {
            target: Some(Target::Pane {
                ws: "s1".into(),
                win: "w1".into(),
                pane: pane.into(),
            }),
            depth: 2,
            current,
            agent: pane.into(),
            ..Default::default()
        };
        let render = |tree: &[Row]| {
            let cfg = Sidebar {
                agents: AgentsPlace::Below,
                ..Default::default()
            };
            let area = Rect::new(0, 0, 20, 8);
            let mut buf = Buffer::empty(area);
            View {
                tree,
                agents: &[],
                config: &cfg,
                theme: &theme,
                indicators: Indicators::Dots,
                collapsed: false,
                boxed: false,
                cursor: None,
                agent_cursor: None,
                focused: false,
            }
            .render(area, &mut buf);
            // Which rows (under the header) have the accent background.
            (1..=tree.len() as u16)
                .filter(|&y| buf[(5, y)].bg == theme.accent)
                .map(|y| y as usize - 1)
                .collect::<Vec<_>>()
        };
        let tree = [
            ws("s1", "kelpie", true, true),
            win("w1", true),
            pane("t1", false),
            pane("t2", true),
            win("w2", false),
        ];
        assert_eq!(render(&tree), [3], "the focused pane");
        // Its window folded: the window.
        let mut folded = tree[..2].to_vec();
        folded[1].open = false;
        folded.push(tree[4].clone());
        assert_eq!(render(&folded), [1]);
        // The workspace folded: the workspace.
        let mut ws_only = ws("s1", "kelpie", false, true);
        ws_only.open = false;
        assert_eq!(render(&[ws_only, ws("s2", "api", false, false)]), [0]);
    }

    #[test]
    fn boxed_sidebar_has_a_border_title_and_divider() {
        let theme = builtin("catppuccin-mocha").unwrap();
        let tree = [ws("s1", "kelpie", false, true)];
        let agents = [AgentRow {
            ws: "s1".into(),
            win: "w1".into(),
            pane: "t1".into(),
            row: Row {
                agent: "claude".into(),
                workspace: "kelpie".into(),
                window: "0:edit".into(),
                state: Some(Agent::Done),
                ..Default::default()
            },
            order: 0,
        }];
        // 7 rows: too short to split, so agents follow.
        let cfg = Sidebar {
            header: crate::config::SidebarHeader::Border,
            ..Sidebar::default()
        };
        let area = Rect::new(0, 0, 24, 7);
        let mut buf = Buffer::empty(area);
        let hits = View {
            tree: &tree,
            agents: &agents,
            config: &cfg,
            theme: &theme,
            indicators: Indicators::Dots,
            collapsed: false,
            boxed: true,
            cursor: None,
            agent_cursor: None,
            focused: false,
        }
        .render(area, &mut buf);
        assert_eq!(text(&buf, 0), "┌ Workspaces ──────────┐");
        assert_eq!(text(&buf, 1), "│ ▸ kelpie             │");
        assert_eq!(text(&buf, 3), "├─ Agents ─────────────┤");
        assert_eq!(text(&buf, 4), "│ ● claude kelpie:0:edi│");
        assert_eq!(text(&buf, 6), "└──────────────────────┘");
        assert_eq!(
            hits[1],
            (Rect::new(1, 1, 22, 1), Hit::Go(Target::Workspace("s1".into()))),
            "clicks land inside the box"
        );
    }

    fn agent(pane: &str, name: &str, order: usize) -> AgentRow {
        AgentRow {
            ws: "s1".into(),
            win: "w1".into(),
            pane: pane.into(),
            row: Row {
                agent: name.into(),
                workspace: "kelpie".into(),
                window: "0:edit".into(),
                state: Some(Agent::Working),
                ..Default::default()
            },
            order,
        }
    }

    fn render_split(tree: &[Row], agents: &[AgentRow], boxed: bool, h: u16) -> (Vec<String>, Vec<(Rect, Hit)>) {
        render_with(tree, agents, boxed, h, crate::config::SidebarHeader::Border)
    }

    fn render_with(
        tree: &[Row],
        agents: &[AgentRow],
        boxed: bool,
        h: u16,
        header: crate::config::SidebarHeader,
    ) -> (Vec<String>, Vec<(Rect, Hit)>) {
        let theme = builtin("catppuccin-mocha").unwrap();
        let cfg = Sidebar {
            header,
            ..Sidebar::default()
        };
        let area = Rect::new(0, 0, 24, h);
        let mut buf = Buffer::empty(area);
        let hits = View {
            tree,
            agents,
            config: &cfg,
            theme: &theme,
            indicators: Indicators::Dots,
            collapsed: false,
            boxed,
            cursor: None,
            agent_cursor: None,
            focused: false,
        }
        .render(area, &mut buf);
        ((0..h).map(|y| text(&buf, y)).collect(), hits)
    }

    #[test]
    fn split_sidebar_puts_agents_in_the_bottom_half() {
        let tree = [ws("s1", "kelpie", false, true), ws("s2", "api", false, false)];
        let agents = [agent("t1", "claude", 0), agent("t2", "codex", 1)];
        let (s, hits) = render_split(&tree, &agents, true, 20);
        assert_eq!(s[0], "┌ Workspaces ──────────┐");
        assert_eq!(s[1], "│ ▸ kelpie             │");
        assert_eq!(s[9], "└──────────────────────┘", "top box: the upper half");
        assert_eq!(s[10], "┌ Agents ──────────────┐");
        assert_eq!(s[11], "│ ● claude kelpie:0:edi│");
        assert_eq!(s[12], "│ ● codex kelpie:0:edit│");
        assert_eq!(s[19], "└──────────────────────┘");
        assert!(
            hits.iter()
                .any(|(r, h)| r.y == 12 && matches!(h, Hit::Go(Target::Pane { pane, .. }) if pane == "t2"))
        );

        // No agents: the bottom box says so.
        let (s, _) = render_split(&tree, &[], true, 20);
        assert_eq!(s[11], "│ no agents running    │");
        // Unboxed: headers instead of boxes.
        let (s, _) = render_split(&tree, &agents, false, 20);
        assert_eq!(s[0], " Workspaces            │");
        assert_eq!(s[10], " Agents                │");
        assert_eq!(s[11], " ● claude kelpie:0:edit│");
    }

    #[test]
    fn split_sidebar_scrolls_the_tree_and_counts_hidden_agents() {
        // 12 workspaces in a 6-row top box (4 inside): you're on the last one.
        let tree: Vec<Row> = (0..12)
            .map(|i| ws(&format!("s{i}"), &format!("ws{i}"), false, i == 11))
            .collect();
        let agents: Vec<AgentRow> = (0..9).map(|i| agent(&format!("t{i}"), &format!("a{i}"), i)).collect();
        let (s, _) = render_split(&tree, &agents, true, 12);
        assert!(s[4].contains("ws11"), "the current workspace stays in sight: {s:#?}");
        assert!(!s.iter().any(|l| l.contains("ws0 ")), "{s:#?}");
        // Bottom box: 6 rows, 4 inside: 3 agents and a count of the rest.
        assert!(s[9].contains("a2"), "{s:#?}");
        assert_eq!(s[10], "│ +6 more              │");
    }

    #[test]
    fn the_agents_list_scrolls_to_keep_the_cursor() {
        // All fit.
        assert_eq!(agent_window(3, 5, Some(2)), (0, 3));
        // 9 in 4 rows: 3 and "+N more"; the cursor past them scrolls.
        assert_eq!(agent_window(9, 4, None), (0, 3));
        assert_eq!(agent_window(9, 4, Some(2)), (0, 3));
        assert_eq!(agent_window(9, 4, Some(4)), (2, 3));
        // At the end: nothing below, so all 4 rows are agents.
        assert_eq!(agent_window(9, 4, Some(8)), (5, 4));
        assert_eq!(agent_window(9, 0, Some(8)), (9, 0));
    }

    #[test]
    fn tabs_scroll_to_keep_the_active_one() {
        let w = [10, 10, 10, 10, 10];
        assert_eq!(visible_tabs(&w, 0, 60), (0, 5), "all fit");
        assert_eq!(visible_tabs(&w, 0, 32), (0, 3));
        assert_eq!(visible_tabs(&w, 4, 32), (2, 5));
        assert_eq!(visible_tabs(&w, 2, 22), (2, 4), "grows right first, then left");
        assert_eq!(visible_tabs(&w, 2, 5), (2, 3), "the active one even if it doesn't fit");
        assert_eq!(visible_tabs(&[], 0, 10), (0, 0));
    }

    #[test]
    fn headers_on_rows_of_their_own() {
        use crate::config::SidebarHeader;
        let tree = [ws("s1", "kelpie", false, true), ws("s2", "api", false, false)];
        let agents = [agent("t1", "claude", 0)];
        let (s, hits) = render_with(&tree, &agents, true, 20, SidebarHeader::Row);
        assert_eq!(s[0], "┌──────────────────────┐");
        assert_eq!(s[1], "│ Workspaces           │");
        assert_eq!(
            s[2], "├──────────────────────┤",
            "a line under the name, joined to the box"
        );
        assert_eq!(s[3], "│ ▸ kelpie             │");
        assert_eq!(s[10], "┌──────────────────────┐");
        assert_eq!(s[11], "│ Agents               │");
        assert_eq!(s[12], "├──────────────────────┤");
        assert_eq!(s[13], "│ ● claude kelpie:0:edi│");
        assert!(
            hits.iter()
                .any(|(r, h)| r.y == 3 && *h == Hit::Go(Target::Workspace("s1".into()))),
            "clicks land on the rows under it"
        );
        // Too short to split: the agents follow, their name between two lines.
        let theme = builtin("catppuccin-mocha").unwrap();
        let cfg = Sidebar {
            agents: AgentsPlace::Below,
            ..Sidebar::default()
        };
        let area = Rect::new(0, 0, 24, 12);
        let mut buf = Buffer::empty(area);
        View {
            tree: &tree,
            agents: &agents,
            config: &cfg,
            theme: &theme,
            indicators: Indicators::Dots,
            collapsed: false,
            boxed: true,
            cursor: None,
            agent_cursor: None,
            focused: false,
        }
        .render(area, &mut buf);
        let s: Vec<String> = (0..12).map(|y| text(&buf, y)).collect();
        assert_eq!(
            s[..10],
            [
                "┌──────────────────────┐",
                "│ Workspaces           │",
                "├──────────────────────┤",
                "│ ▸ kelpie             │",
                "│ ▸ api                │",
                "│                      │",
                "├──────────────────────┤",
                "│ Agents               │",
                "├──────────────────────┤",
                "│ ● claude kelpie:0:edi│",
            ],
            "{s:#?}"
        );
    }
}
