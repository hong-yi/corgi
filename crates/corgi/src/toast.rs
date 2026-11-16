//! Toasts: short notices stacked over the panes (a corner, or the middle of the top or
//! bottom edge: `notifications.position`), for things that
//! happen where you aren't looking (an agent that needs you or finished, a program's
//! OSC 9/777 notification). Click one to go there. Pure, apart from the clock.

use crate::config::ToastPosition;
use crate::theme::Theme;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};
use std::time::{Duration, Instant};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// At most this many on screen; a new one pushes out the oldest.
const MAX: usize = 4;
const MIN_W: u16 = 24;
const MAX_W: u16 = 52;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An agent waits for you.
    Blocked,
    /// An agent finished while you looked elsewhere.
    Done,
    /// A program's own notification.
    Info,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    /// The pane it's about: clicking goes there, and looking there dismisses it.
    pub pane: Option<String>,
    pub kind: Kind,
    pub title: String,
    /// Where it's from (`workspace:window`), dimmed after the title.
    pub place: String,
    pub body: String,
    pub until: Instant,
}

#[derive(Default)]
pub struct Toasts {
    /// Oldest first; drawn newest on top.
    list: Vec<Toast>,
}

impl Toasts {
    /// Show `t` for `life`. A newer notice about the same pane replaces the older one, so an
    /// agent that reports both a status and an OSC 9 for the same moment shows once.
    pub fn push(&mut self, mut t: Toast, now: Instant, life: Duration) {
        t.title = clean(&t.title);
        t.body = clean(&t.body);
        t.place = clean(&t.place);
        t.until = now + life;
        if t.pane.is_some() {
            self.list.retain(|o| o.pane != t.pane);
        }
        self.list.push(t);
        if self.list.len() > MAX {
            self.list.remove(0);
        }
    }

    /// Drop the expired ones. True if any went.
    pub fn expire(&mut self, now: Instant) -> bool {
        let n = self.list.len();
        self.list.retain(|t| t.until > now);
        n != self.list.len()
    }

    /// You're looking at `pane` now: its notices have been seen.
    pub fn dismiss_pane(&mut self, pane: &str) -> bool {
        let n = self.list.len();
        self.list.retain(|t| t.pane.as_deref() != Some(pane));
        n != self.list.len()
    }

    /// When the next one expires, to wake up and redraw then.
    pub fn deadline(&self) -> Option<Instant> {
        self.list.iter().map(|t| t.until).min()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    #[cfg(test)]
    pub fn iter(&self) -> impl Iterator<Item = &Toast> {
        self.list.iter()
    }

    /// Where each toast goes in `area`: newest nearest the edge `at` names, older ones
    /// further in (below it at the top, above it at the bottom).
    pub fn layout(&self, area: Rect, at: ToastPosition) -> Vec<(Rect, &Toast)> {
        use ToastPosition::*;
        let mut out = Vec::new();
        if area.width < MIN_W.min(20) || area.height < 3 {
            return out;
        }
        let top = matches!(at, TopLeft | TopCenter | TopRight);
        // Rows used so far, from the edge.
        let mut used = 0;
        for t in self.list.iter().rev() {
            // Glyph and title, ` · place`; or the body. Plus a space each side, inside the border.
            let head = 2 + t.title.width() + if t.place.is_empty() { 0 } else { t.place.width() + 3 };
            let text_w = head.max(t.body.width()) + 2;
            let w = (text_w as u16 + 2).clamp(MIN_W, MAX_W).min(area.width);
            let h = if t.body.is_empty() { 3 } else { 4 };
            if used + h > area.height {
                break;
            }
            let x = match at {
                TopLeft | BottomLeft => area.x,
                TopCenter | BottomCenter => area.x + (area.width - w) / 2,
                TopRight | BottomRight => area.right() - w,
            };
            let y = if top { area.y + used } else { area.bottom() - used - h };
            out.push((Rect::new(x, y, w, h), t));
            used += h;
        }
        out
    }

    /// Draw them over `area`; returns where each landed, for clicks.
    pub fn render(
        &self,
        area: Rect,
        at: ToastPosition,
        buf: &mut Buffer,
        theme: &Theme,
    ) -> Vec<(Rect, Option<String>)> {
        let mut hits = Vec::new();
        for (r, t) in self.layout(area, at) {
            let color = match t.kind {
                Kind::Blocked => theme.blocked,
                Kind::Done => theme.done,
                Kind::Info => theme.accent,
            };
            let glyph = match t.kind {
                Kind::Blocked => "●",
                Kind::Done => "✓",
                Kind::Info => "◆",
            };
            Clear.render(r, buf);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(theme.border_type())
                .border_style(Style::new().fg(color).bg(theme.surface))
                .style(Style::new().bg(theme.surface).fg(theme.fg));
            let inner = block.inner(r);
            block.render(r, buf);
            let room = inner.width.saturating_sub(2) as usize;
            let glyph_w = glyph.width() + 1;
            let title = cut(&t.title, room.saturating_sub(glyph_w));
            let mut head = vec![
                Span::styled(format!("{glyph} "), Style::new().fg(color)),
                Span::styled(title.clone(), Style::new().add_modifier(Modifier::BOLD)),
            ];
            let left = room.saturating_sub(glyph_w + title.width() + 3);
            if !t.place.is_empty() && left > 0 {
                head.push(Span::styled(
                    format!(" · {}", cut(&t.place, left)),
                    Style::new().fg(theme.muted),
                ));
            }
            let mut lines = vec![Line::from(head)];
            if !t.body.is_empty() {
                lines.push(Line::from(cut(&t.body, room)));
            }
            let text = Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), inner.height);
            Paragraph::new(lines).render(text, buf);
            hits.push((r, t.pane.clone()));
        }
        hits
    }
}

/// One line of plain text: control characters (escapes, newlines) become spaces, runs of
/// whitespace one space. Programs send what they like; none of it reaches the screen raw.
pub fn clean(s: &str) -> String {
    s.split(|c: char| c.is_control() || c.is_whitespace())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `s` in at most `w` columns, with `…` if it was cut.
fn cut(s: &str, w: usize) -> String {
    if s.width() <= w {
        return s.to_string();
    }
    if w == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if used + cw + 1 > w {
            break;
        }
        out.push(c);
        used += cw;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toast(pane: Option<&str>, title: &str, body: &str) -> Toast {
        Toast {
            pane: pane.map(Into::into),
            kind: Kind::Info,
            title: title.into(),
            place: "s:w".into(),
            body: body.into(),
            until: Instant::now(),
        }
    }

    #[test]
    fn same_pane_replaces_and_old_ones_go() {
        let now = Instant::now();
        let mut ts = Toasts::default();
        let life = Duration::from_secs(5);
        ts.push(toast(Some("t1"), "a", ""), now, life);
        ts.push(toast(Some("t1"), "b", ""), now, life);
        assert_eq!(ts.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["b"]);
        for i in 0..6 {
            ts.push(toast(None, &format!("n{i}"), ""), now, life);
        }
        assert_eq!(ts.iter().count(), MAX, "capped");
        assert_eq!(ts.iter().next().unwrap().title, "n2", "oldest dropped first");
        assert!(!ts.expire(now + Duration::from_secs(4)));
        assert_eq!(ts.deadline(), Some(now + life));
        assert!(ts.expire(now + life));
        assert!(ts.is_empty());
    }

    #[test]
    fn looking_at_a_pane_dismisses_its_toasts() {
        let now = Instant::now();
        let mut ts = Toasts::default();
        ts.push(toast(Some("t1"), "a", ""), now, Duration::from_secs(5));
        ts.push(toast(Some("t2"), "b", ""), now, Duration::from_secs(5));
        assert!(ts.dismiss_pane("t1"));
        assert!(!ts.dismiss_pane("t1"));
        assert_eq!(ts.iter().count(), 1);
    }

    #[test]
    fn text_is_cleaned() {
        assert_eq!(clean("  a\x1b[31m\n b\x07\tc "), "a [31m b c");
        assert_eq!(cut("hello world", 6), "hello…");
        assert_eq!(cut("hi", 6), "hi");
        assert_eq!(cut("中文字", 4), "中…");
    }

    #[test]
    fn stacked_newest_on_top_in_the_corner() {
        let now = Instant::now();
        let mut ts = Toasts::default();
        ts.push(toast(Some("t1"), "first", "body"), now, Duration::from_secs(5));
        ts.push(toast(Some("t2"), "second", ""), now, Duration::from_secs(5));
        let area = Rect::new(10, 2, 100, 30);
        let l = ts.layout(area, ToastPosition::TopRight);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].1.title, "second");
        assert_eq!(l[0].0, Rect::new(110 - MIN_W, 2, MIN_W, 3));
        assert_eq!(l[1].0.y, 5, "below the newer one");
        assert_eq!(l[1].0.height, 4, "a body line");
        // Too short for the second: it waits.
        assert_eq!(ts.layout(Rect::new(0, 0, 100, 5), ToastPosition::TopRight).len(), 1);
        assert!(
            ts.layout(Rect::new(0, 0, 10, 30), ToastPosition::TopRight).is_empty(),
            "too narrow"
        );
    }

    #[test]
    fn every_position() {
        use ToastPosition::*;
        let now = Instant::now();
        let mut ts = Toasts::default();
        ts.push(toast(Some("t1"), "older", "body"), now, Duration::from_secs(5));
        ts.push(toast(Some("t2"), "newer", ""), now, Duration::from_secs(5));
        let area = Rect::new(10, 2, 100, 30);
        let (w, right, mid) = (MIN_W, 110 - MIN_W, 10 + (100 - MIN_W) / 2);
        let at = |p| {
            ts.layout(area, p)
                .into_iter()
                .map(|(r, t)| (t.title.clone(), r))
                .collect::<Vec<_>>()
        };
        let r = |x, y, h| Rect::new(x, y, w, h);
        // The newer one (3 rows) nearest the edge, the older (4 rows: a body) further in.
        assert_eq!(
            at(TopLeft),
            [("newer".into(), r(10, 2, 3)), ("older".into(), r(10, 5, 4))]
        );
        assert_eq!(
            at(TopCenter),
            [("newer".into(), r(mid, 2, 3)), ("older".into(), r(mid, 5, 4))]
        );
        assert_eq!(
            at(TopRight),
            [("newer".into(), r(right, 2, 3)), ("older".into(), r(right, 5, 4))]
        );
        assert_eq!(
            at(BottomLeft),
            [("newer".into(), r(10, 29, 3)), ("older".into(), r(10, 25, 4))]
        );
        assert_eq!(
            at(BottomCenter),
            [("newer".into(), r(mid, 29, 3)), ("older".into(), r(mid, 25, 4))]
        );
        assert_eq!(
            at(BottomRight),
            [("newer".into(), r(right, 29, 3)), ("older".into(), r(right, 25, 4))]
        );
    }

    #[test]
    fn renders_title_place_and_body() {
        let now = Instant::now();
        let mut ts = Toasts::default();
        ts.push(
            toast(Some("t1"), "claude needs you", "Allow Bash?"),
            now,
            Duration::from_secs(5),
        );
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);
        let theme = crate::theme::builtin("terminal").unwrap();
        let hits = ts.render(area, ToastPosition::TopRight, &mut buf, &theme);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1.as_deref(), Some("t1"));
        let r = hits[0].0;
        let row = |y: u16| {
            (r.x..r.right())
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        };
        assert!(row(r.y + 1).contains("◆ claude needs you · s:w"), "{:?}", row(r.y + 1));
        assert!(row(r.y + 2).contains("Allow Bash?"), "{:?}", row(r.y + 2));
    }
}
