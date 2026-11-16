//! The key-hints dialog: after the prefix (or any key table that waits for a
//! second key), list what the next key can do. It's information only: the
//! next key still goes to the key table, and the dialog closes either way.
//!
//! Groups are kept whole where they fit. When the terminal is too small for
//! a dialog, a one-line strip is drawn at the bottom instead.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Widget};
use unicode_width::UnicodeWidthStr;

/// One binding: the key as the user types it after the prefix, and what it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hint {
    pub key: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub title: String,
    pub hints: Vec<Hint>,
}

impl Group {
    #[cfg(test)]
    pub fn new(title: &str, hints: &[(&str, &str)]) -> Group {
        Group {
            title: title.into(),
            hints: hints
                .iter()
                .map(|(k, l)| Hint {
                    key: (*k).into(),
                    label: (*l).into(),
                })
                .collect(),
        }
    }

    /// Title row plus one row per hint.
    fn height(&self) -> u16 {
        1 + self.hints.len() as u16
    }
}

const GAP: u16 = 3;
const MIN_W: u16 = 24;
const MIN_H: u16 = 6;

/// Where each group goes inside the dialog body: `columns[c]` lists group
/// indices top to bottom. Groups that don't fit are counted in `hidden`.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub col_w: u16,
    pub columns: Vec<Vec<usize>>,
    pub body_h: u16,
    pub hidden: usize,
}

/// Lay out groups into as few rows as possible within `max_w × max_h` (the body, without border).
pub fn plan(groups: &[Group], max_w: u16, max_h: u16) -> Plan {
    let key_w = groups
        .iter()
        .flat_map(|g| &g.hints)
        .map(|h| h.key.width())
        .max()
        .unwrap_or(1) as u16;
    let entry_w = groups
        .iter()
        .flat_map(|g| g.hints.iter().map(|h| key_w + 2 + h.label.width() as u16))
        .chain(groups.iter().map(|g| g.title.width() as u16))
        .max()
        .unwrap_or(MIN_W)
        .min(max_w);
    let max_cols = ((max_w + GAP) / (entry_w + GAP)).max(1) as usize;
    let tallest = groups.iter().map(Group::height).max().unwrap_or(0);
    let cols = max_cols.min(groups.len().max(1));
    // Few groups (as key tables have): try every way of putting whole groups into columns
    // (each column keeping their order) and keep the shortest, so one tall group doesn't leave
    // the others stacked beside it. Many: fill columns in order.
    let mut best = if groups.len() <= 8 {
        balanced(groups, cols).filter(|(_, h)| *h <= max_h)
    } else {
        None
    };
    if best.is_none() {
        // For each column count, start at an even share of the rows and let columns grow
        // until the groups (kept whole, in order) fit.
        let total: u16 = groups.iter().map(|g| g.height() + 1).sum::<u16>().saturating_sub(1);
        for cols in (1..=cols).rev() {
            let mut target = total.div_ceil(cols as u16).max(tallest);
            while target <= max_h {
                let (columns, h) = pack(groups, cols, target);
                if columns.iter().map(Vec::len).sum::<usize>() == groups.len() {
                    if best.as_ref().is_none_or(|(_, bh): &(Vec<Vec<usize>>, u16)| h < *bh) {
                        best = Some((columns, h));
                    }
                    break;
                }
                target += 1;
            }
        }
    }
    if let Some((columns, body_h)) = best {
        return Plan {
            col_w: entry_w,
            columns,
            body_h,
            hidden: 0,
        };
    }
    // Doesn't fit even with every column: fill what fits, count the rest.
    let (columns, body_h) = pack(groups, max_cols, max_h);
    let shown = columns.iter().map(Vec::len).sum::<usize>();
    Plan {
        col_w: entry_w,
        columns,
        body_h: body_h.min(max_h),
        hidden: groups.len() - shown,
    }
}

/// Greedy: fill column by column, starting a new column when the next group would pass `limit`.
/// The shortest way to put every group, whole, into at most `cols` columns; groups keep their
/// order within a column. Ties go to fewer columns, then to the one closest to reading order.
fn balanced(groups: &[Group], cols: usize) -> Option<(Vec<Vec<usize>>, u16)> {
    let n = groups.len();
    if n == 0 {
        return None;
    }
    let mut best: Option<(u16, usize, usize, Vec<usize>)> = None;
    // Each assignment as a number in base `cols`: digit i is group i's column.
    let total = cols.checked_pow(n as u32)?;
    for code in 0..total {
        let mut at = vec![0usize; n];
        let mut c = code;
        for slot in at.iter_mut() {
            *slot = c % cols;
            c /= cols;
        }
        // Columns in use must be 0..k, in order of first use: the same layouts once each.
        let mut seen = 0;
        if at.iter().any(|&col| {
            if col > seen {
                return true;
            }
            if col == seen {
                seen += 1;
            }
            false
        }) {
            continue;
        }
        let used = seen;
        let mut heights = vec![0u16; used];
        for (i, &col) in at.iter().enumerate() {
            let h = groups[i].height();
            heights[col] = if heights[col] == 0 { h } else { heights[col] + 1 + h };
        }
        let h = heights.iter().copied().max().unwrap_or(0);
        // Reading order: groups later in the list in later columns.
        let disorder = at.windows(2).filter(|w| w[1] < w[0]).count();
        let key = (h, used, disorder);
        if best.as_ref().is_none_or(|(bh, bu, bd, _)| key < (*bh, *bu, *bd)) {
            best = Some((h, used, disorder, at));
        }
    }
    let (h, used, _, at) = best?;
    let mut columns = vec![Vec::new(); used];
    for (i, col) in at.into_iter().enumerate() {
        columns[col].push(i);
    }
    Some((columns, h))
}

fn pack(groups: &[Group], cols: usize, limit: u16) -> (Vec<Vec<usize>>, u16) {
    let mut columns: Vec<Vec<usize>> = vec![Vec::new()];
    let mut h = 0u16;
    let mut tallest = 0u16;
    for (i, g) in groups.iter().enumerate() {
        let need = if h == 0 { g.height() } else { h + 1 + g.height() };
        if need > limit && h > 0 {
            if columns.len() == cols {
                break;
            }
            columns.push(Vec::new());
            h = 0;
        }
        if g.height() > limit {
            break;
        }
        h = if h == 0 { g.height() } else { h + 1 + g.height() };
        tallest = tallest.max(h);
        columns.last_mut().unwrap().push(i);
    }
    (columns, tallest)
}

pub struct KeyHints<'a> {
    /// e.g. "C-b" — shown in the dialog title.
    pub prefix: &'a str,
    pub groups: &'a [Group],
    pub accent: Color,
    pub border: BorderType,
    /// Across the bottom of the area, its full width; else a dialog in its middle.
    pub bottom: bool,
}

impl KeyHints<'_> {
    fn strip(&self, area: Rect, buf: &mut Buffer) {
        let y = area.bottom().saturating_sub(1);
        let row = Rect::new(area.x, y, area.width, 1);
        Clear.render(row, buf);
        let mut spans = vec![Span::styled(
            format!(" {} ", self.prefix),
            Style::new().bg(self.accent).fg(Color::Black),
        )];
        for h in self.groups.iter().flat_map(|g| &g.hints) {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                h.key.clone(),
                Style::new().fg(self.accent).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(format!(":{}", h.label)));
        }
        Line::from(spans).render(row, buf);
    }
}

impl Widget for KeyHints<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < MIN_W + 4 || area.height < MIN_H + 2 {
            return self.strip(area, buf);
        }
        // Body budget: border (2) + inner padding (2) horizontally; border + footer vertically.
        let max_w = if self.bottom {
            area.width - 4
        } else {
            (area.width - 4).min(160) - 2
        };
        let max_h = area.height - 2 - 3;
        let p = plan(self.groups, max_w, max_h);
        // Each column as wide as its own entries (keys right-aligned to its own widest key), so
        // a column of single-letter keys isn't padded out to fit `C-arrows` in another.
        let width = |s: &str| s.width() as u16;
        let cols: Vec<(u16, u16)> = p
            .columns
            .iter()
            .map(|col| {
                let groups = || col.iter().map(|&gi| &self.groups[gi]);
                let key_w = groups()
                    .flat_map(|g| &g.hints)
                    .map(|h| width(&h.key))
                    .max()
                    .unwrap_or(1);
                let w = groups()
                    .flat_map(|g| g.hints.iter().map(|h| key_w + 2 + width(&h.label)))
                    .chain(groups().map(|g| width(&g.title)))
                    .max()
                    .unwrap_or(1)
                    .min(p.col_w);
                (key_w, w)
            })
            .collect();
        let n = cols.len().max(1) as u16;
        let body_w = cols.iter().map(|(_, w)| w).sum::<u16>() + (n - 1) * GAP;
        let footer = if p.hidden > 0 {
            format!("Esc cancel · ? all keys · {} more groups", p.hidden)
        } else {
            "Esc cancel · ? all keys".to_string()
        };
        let w = (body_w.max(footer.width() as u16) + 4).min(area.width);
        let h = p.body_h + 4;
        let rect = if self.bottom {
            Rect::new(
                area.x,
                area.bottom() - h.min(area.height),
                area.width,
                h.min(area.height),
            )
        } else {
            // Centred in the area it's given (the whole window).
            crate::ui::centered(area, w, h)
        };
        Clear.render(rect, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(self.border)
            .border_style(Style::new().fg(self.accent))
            .title(Line::from(vec![
                Span::raw(" "),
                Span::styled(self.prefix, Style::new().fg(self.accent).add_modifier(Modifier::BOLD)),
                Span::raw(" … "),
            ]));
        let inner = block.inner(rect);
        block.render(rect, buf);

        // Across the bottom, the columns sit together in the middle.
        let mut cx = inner.x + 1 + inner.width.saturating_sub(2 + body_w) / 2;
        for (col, &(key_w, col_w)) in p.columns.iter().zip(&cols) {
            let key_w = key_w as usize;
            let mut cy = inner.y;
            for &gi in col {
                let g = &self.groups[gi];
                let title = Style::new().add_modifier(Modifier::BOLD | Modifier::DIM);
                buf.set_stringn(cx, cy, &g.title, col_w as usize, title);
                cy += 1;
                for hint in &g.hints {
                    let key = Style::new().fg(self.accent).add_modifier(Modifier::BOLD);
                    buf.set_stringn(cx, cy, format!("{:>key_w$}", hint.key), key_w, key);
                    let room = (col_w as usize).saturating_sub(key_w + 2);
                    buf.set_stringn(cx + key_w as u16 + 2, cy, &hint.label, room, Style::new());
                    cy += 1;
                }
                cy += 1;
            }
            cx += col_w + GAP;
        }
        let fy = inner.bottom() - 1;
        let fx = inner.x + (inner.width.saturating_sub(footer.width() as u16)) / 2;
        buf.set_stringn(
            fx,
            fy,
            &footer,
            inner.width as usize,
            Style::new().add_modifier(Modifier::DIM),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn groups() -> Vec<Group> {
        vec![
            Group::new(
                "Windows",
                &[
                    ("c", "new window"),
                    ("C", "new window…"),
                    ("n", "next"),
                    ("p", "previous"),
                ],
            ),
            Group::new(
                "Panes",
                &[
                    ("%", "split right"),
                    ("\"", "split down"),
                    ("z", "zoom"),
                    ("x", "kill pane"),
                ],
            ),
            Group::new(
                "Workspaces",
                &[("N", "new workspace…"), ("s", "choose"), ("$", "rename")],
            ),
            Group::new("Other", &[(":", "command"), ("?", "all keys"), ("d", "detach")]),
        ]
    }

    #[test]
    fn wide_screen_uses_several_columns_and_keeps_groups_whole() {
        let g = groups();
        let p = plan(&g, 100, 20);
        assert_eq!(p.hidden, 0);
        assert!(p.columns.len() >= 2, "{p:?}");
        let mut all: Vec<usize> = p.columns.concat();
        all.sort();
        assert_eq!(all, vec![0, 1, 2, 3]);
        assert_eq!(p.body_h, 5, "one row of groups: title + 4 hints");
    }

    #[test]
    fn one_tall_group_still_gets_columns_beside_it() {
        // The default prefix table: Panes is much taller than the rest.
        let km = crate::keys::Keymap::build(&crate::config::Keys::default()).unwrap();
        let g = km.hints("prefix");
        let tallest = g.iter().map(Group::height).max().unwrap();
        for (w, h) in [(100, 30), (160, 45), (200, 50)] {
            // The room the dialog really has across the bottom of a w×h screen: above the tab
            // bar, inside its border and padding, less the footer.
            let p = plan(&g, w - 4, h - 1 - 5);
            assert_eq!(p.hidden, 0, "{w}x{h}: every group shows: {p:?}");
            assert!(p.columns.len() >= 2, "{w}x{h}: {p:?}");
            if w >= 160 {
                assert_eq!(
                    p.body_h, tallest,
                    "{w}x{h}: room for every column: as short as the tallest group"
                );
            }
        }
    }

    #[test]
    fn tall_and_short_groups_balance() {
        let g = |name: &str, n: usize| {
            // Labels as long as real ones: three columns to a 96-cell row.
            let hints: Vec<(String, String)> = (0..n)
                .map(|i| (format!("k{i}"), "a label as long as real".to_string()))
                .collect();
            let refs: Vec<(&str, &str)> = hints.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
            Group::new(name, &refs)
        };
        // As the default prefix table is shaped: a tall second group, three short ones after.
        let groups = [
            g("Windows", 10),
            g("Panes", 16),
            g("Workspaces", 7),
            g("Agents", 3),
            g("Other", 8),
        ];
        // Only three columns' room (about 30 cells each).
        let p = plan(&groups, 96, 40);
        assert!(p.columns.len() <= 3, "{p:?}");
        // Panes alone (17), Windows + Agents (16), Workspaces + Other (18): the best there is,
        // where in order they'd be 11 + 17 = 29 rows.
        assert_eq!(p.body_h, 18, "{p:?}");
        assert!(p.columns.iter().any(|c| c == &vec![1]), "the tall one alone: {p:?}");
    }

    #[test]
    fn narrow_screen_stacks_into_one_column() {
        let p = plan(&groups(), 24, 40);
        assert_eq!(p.columns.len(), 1);
        assert_eq!(p.body_h, 5 + 1 + 5 + 1 + 4 + 1 + 4);
    }

    #[test]
    fn short_screen_reports_hidden_groups() {
        let p = plan(&groups(), 24, 11);
        assert_eq!(p.columns.concat(), vec![0, 1]);
        assert_eq!(p.hidden, 2);
    }

    fn render(w: u16, h: u16) -> Vec<String> {
        render_at(w, h, false)
    }

    fn render_at(w: u16, h: u16, bottom: bool) -> Vec<String> {
        let g = groups();
        let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
        KeyHints {
            prefix: "C-b",
            groups: &g,
            accent: Color::Yellow,
            border: BorderType::Plain,
            bottom,
        }
        .render(buf.area, &mut buf);
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>())
            .collect()
    }

    #[test]
    fn dialog_shows_title_keys_and_footer() {
        let screen = render(100, 24).join("\n");
        assert!(screen.contains("C-b …"), "{screen}");
        assert!(screen.contains("c  new window"), "{screen}");
        assert!(screen.contains("Workspaces"), "{screen}");
        assert!(screen.contains("Esc cancel · ? all keys"), "{screen}");
    }

    #[test]
    fn at_the_bottom_it_spans_the_width() {
        let s = render_at(120, 30, true);
        let top = s.iter().position(|l| l.contains('┌')).unwrap();
        assert_eq!(s.last().unwrap().chars().next(), Some('└'), "on the last row");
        assert!(
            s[top].starts_with('┌') && s[top].ends_with('┐'),
            "full width: {:?}",
            s[top]
        );
        assert!(top > 15, "only as tall as it needs: {top}");
        // The columns sit together, a few cells apart, in the middle.
        let at = |w: &str| s.iter().find_map(|l| l.find(w)).unwrap();
        let (first, last) = (at("Windows"), at("Other"));
        assert!(last - first < 70, "{first}..{last}");
        assert!(
            (first as i32 - (120 - last as i32 - "detach".len() as i32 - 3)).abs() < 12,
            "centred: {first}..{last}"
        );
    }

    #[test]
    fn dialog_is_centred_both_ways() {
        let screen = render(100, 30);
        let top = screen.iter().position(|l| l.contains('┌')).unwrap();
        let bottom = screen.iter().position(|l| l.contains('└')).unwrap();
        assert!(
            (top as i32 - (29 - bottom as i32)).abs() <= 1,
            "vertical: rows {top}..{bottom} of 30"
        );
        let left = screen[top].find('┌').unwrap();
        let right = screen[top].chars().count() - 1 - screen[top].chars().rev().position(|c| c == '┐').unwrap();
        let (l, r) = (screen[top][..left].chars().count(), 99 - right);
        assert!(
            (l as i32 - r as i32).abs() <= 1,
            "horizontal: {l} columns left, {r} right"
        );
    }

    #[test]
    fn tiny_terminal_falls_back_to_a_strip() {
        let screen = render(40, 5);
        assert!(screen[4].starts_with(" C-b  c:new window"), "{:?}", screen[4]);
        assert!(screen[..4].iter().all(|l| l.trim().is_empty()));
    }
}
