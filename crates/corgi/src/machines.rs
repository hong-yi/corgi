//! Other machines (core's `machine.*`): what `machine.list` says about each one's line, how a
//! state looks, and the Machines dialog (`C-b M`): the list, its keys and drawing. Pure.

use crate::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line as TextLine, Span};
use ratatui::widgets::{Block, Borders, Clear, Widget};
use serde::Deserialize;
use unicode_width::UnicodeWidthStr;

/// A machine's line, as core reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    Connecting,
    Connected,
    /// ssh wants a password or code again: `hc machine login` in a terminal.
    NeedsLogin,
    /// Its hc is missing or too old: `hc machine update`.
    NeedsUpdate,
    Unreachable,
    Other(String),
}

impl Link {
    pub fn parse(s: &str) -> Link {
        match s {
            "connecting" => Link::Connecting,
            "connected" => Link::Connected,
            "needs_login" => Link::NeedsLogin,
            "needs_update" => Link::NeedsUpdate,
            "unreachable" => Link::Unreachable,
            s => Link::Other(s.to_string()),
        }
    }

    pub fn is_up(&self) -> bool {
        *self == Link::Connected
    }

    pub fn word(&self) -> &str {
        match self {
            Link::Connecting => "connecting",
            Link::Connected => "connected",
            Link::NeedsLogin => "needs login",
            Link::NeedsUpdate => "needs update",
            Link::Unreachable => "unreachable",
            Link::Other(s) => s,
        }
    }

    pub fn glyph(&self) -> &'static str {
        match self {
            Link::Connected => "●",
            Link::Connecting => "◌",
            Link::NeedsLogin | Link::NeedsUpdate => "!",
            Link::Unreachable | Link::Other(_) => "✕",
        }
    }

    pub fn color(&self, t: &Theme) -> Color {
        match self {
            Link::Connected => t.done,
            Link::Connecting => t.working,
            Link::NeedsLogin | Link::NeedsUpdate => t.blocked,
            Link::Unreachable | Link::Other(_) => t.muted,
        }
    }
}

/// One of `machine.list`'s machines (core's `MachineView`).
#[derive(Clone, Debug, Deserialize)]
pub struct Machine {
    pub name: String,
    /// What that machine calls itself.
    #[serde(default)]
    pub host: String,
    /// An ssh target, or (`via`) the command that reaches it.
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub via: bool,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default, deserialize_with = "link")]
    pub state: Option<Link>,
    /// Since when it isn't connected (ms since the epoch).
    #[serde(default)]
    pub since_ms: Option<u64>,
    #[serde(default)]
    pub temporary: bool,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub why: Option<String>,
}

fn link<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Link>, D::Error> {
    Ok(Option::<String>::deserialize(d)?.map(|s| Link::parse(&s)))
}

impl Machine {
    pub fn link(&self) -> Link {
        self.state.clone().unwrap_or(Link::Other("unknown".into()))
    }

    pub fn is_up(&self) -> bool {
        self.link().is_up()
    }

    /// Its state, and for how long: `vps unreachable for 3m`.
    pub fn headline(&self, now_ms: u64) -> String {
        let mut s = format!("{} {}", self.name, self.link().word());
        if let Some(t) = self.since_ms.filter(|&t| t > 0 && now_ms >= t) {
            s.push_str(&format!(" for {}", ago(now_ms - t)));
        }
        s
    }

    /// Why it's down, in a line: its state, then core's reason.
    pub fn trouble(&self, now_ms: u64) -> String {
        let mut s = self.headline(now_ms);
        if let Some(w) = self.why.as_deref().filter(|w| !w.is_empty()) {
            s.push_str(&format!(": {w}"));
        }
        s
    }
}

/// Two lists that would show the same (names, lines, reasons, since when, …).
pub fn same(a: &[Machine], b: &[Machine]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            (
                &x.name,
                &x.host,
                &x.target,
                x.via,
                &x.session,
                &x.state,
                x.since_ms,
                x.temporary,
                &x.version,
                &x.why,
            ) == (
                &y.name,
                &y.host,
                &y.target,
                y.via,
                &y.session,
                &y.state,
                y.since_ms,
                y.temporary,
                &y.version,
                &y.why,
            )
        })
}

/// `machine.list`'s answer.
pub fn parse(v: &serde_json::Value) -> Vec<Machine> {
    v["machines"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| serde_json::from_value(m.clone()).ok())
        .collect()
}

/// A duration, short: `40s`, `3m`, `2h`, `5d`.
pub fn ago(ms: u64) -> String {
    let s = ms / 1000;
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m", s / 60),
        3600..86400 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86400),
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The machine a terminal address is on (`vps/t3` → `vps`), `None` for this one.
pub fn of(addr: &str) -> Option<&str> {
    addr.rsplit_once('/').map(|(m, _)| m)
}

#[derive(Debug, Default)]
pub struct View {
    pub at: usize,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Stay,
    Close,
    /// Dial it now (after a login, or not to wait for the next retry).
    Connect(usize),
    Add,
    Rename(usize),
    Remove(usize),
    /// `hc machine login` in a popup.
    Login(usize),
    /// `hc machine update` in a popup.
    Update(usize),
    /// A new workspace on it.
    NewWorkspace(usize),
}

impl View {
    pub fn key(&mut self, k: &KeyEvent, n: usize) -> Outcome {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let any = n > 0;
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return Outcome::Close,
            KeyCode::Char('c' | 'g') if ctrl => return Outcome::Close,
            KeyCode::Up | KeyCode::Char('k') => self.at = self.at.saturating_sub(1),
            KeyCode::Char('p') if ctrl => self.at = self.at.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.at = (self.at + 1).min(n.saturating_sub(1)),
            KeyCode::Char('n') if ctrl => self.at = (self.at + 1).min(n.saturating_sub(1)),
            KeyCode::Char('a' | 'n') => return Outcome::Add,
            KeyCode::Enter | KeyCode::Char('c') if any => return Outcome::Connect(self.at),
            KeyCode::Char('w') if any => return Outcome::NewWorkspace(self.at),
            KeyCode::Char('r') if any => return Outcome::Rename(self.at),
            KeyCode::Char('d') | KeyCode::Delete if any => return Outcome::Remove(self.at),
            KeyCode::Char('l') if any => return Outcome::Login(self.at),
            KeyCode::Char('u') if any => return Outcome::Update(self.at),
            _ => {}
        }
        Outcome::Stay
    }

    /// `workspaces`: how many workspaces each machine has here.
    pub fn render(
        &mut self,
        list: &[Machine],
        workspaces: &[usize],
        now_ms: u64,
        area: Rect,
        buf: &mut Buffer,
        t: &Theme,
    ) {
        self.at = self.at.min(list.len().saturating_sub(1));
        let footer = "Enter connect · n add · w workspace · r rename · d remove · l login · u update · Esc";
        let rows: Vec<(String, String, String)> = list
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let how = if m.via {
                    format!("via {}", m.target)
                } else {
                    m.target.clone()
                };
                let mut tags = Vec::new();
                if !m.host.is_empty() && m.host != m.name {
                    tags.push(format!("host {}", m.host));
                }
                if let Some(s) = &m.session {
                    tags.push(format!("session {s}"));
                }
                if m.temporary {
                    tags.push("temporary".into());
                }
                if !m.version.is_empty() {
                    tags.push(format!("hc {}", m.version));
                }
                match workspaces.get(i).copied().unwrap_or(0) {
                    0 => {}
                    1 => tags.push("1 workspace".into()),
                    n => tags.push(format!("{n} workspaces")),
                }
                (m.name.clone(), how, tags.join(" · "))
            })
            .collect();
        let nw = rows.iter().map(|r| r.0.width()).max().unwrap_or(4).max(4);
        let sw = list.iter().map(|m| m.link().word().width()).max().unwrap_or(9);
        let widest = rows
            .iter()
            .map(|r| nw + sw + r.1.width() + r.2.width() + 12)
            .max()
            .unwrap_or(0);
        let w = (widest.max(footer.width() + 4) as u16 + 2)
            .min(area.width.saturating_sub(4))
            .max(30.min(area.width));
        let h = (list.len().max(1) as u16 + 7).min(area.height);
        let rect = crate::ui::centered(area, w, h);
        Clear.render(rect, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(t.border_type())
            .border_style(Style::new().fg(t.prefix))
            .style(Style::new().bg(t.surface).fg(t.fg))
            .title(Span::styled(
                " machines ",
                Style::new().fg(t.prefix).add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(rect);
        block.render(rect, buf);
        if list.is_empty() {
            buf.set_stringn(
                inner.x + 1,
                inner.y + 1,
                "no other machines yet: n adds one (ssh, or a command that reaches it)",
                inner.width.saturating_sub(1) as usize,
                Style::new().fg(t.muted),
            );
        }
        for (i, (name, how, tags)) in rows.iter().enumerate() {
            let y = inner.y + 1 + i as u16;
            if y + 3 >= inner.bottom() {
                break;
            }
            let sel = i == self.at;
            let link = list[i].link();
            let base = if sel {
                Style::new().bg(t.accent).fg(t.accent_fg)
            } else {
                Style::new().fg(t.fg)
            };
            let dim = if sel { base } else { Style::new().fg(t.muted) };
            let row = Rect::new(inner.x, y, inner.width, 1);
            buf.set_style(row, base);
            let word = link.word().to_string();
            let mut spans = vec![
                Span::styled(
                    format!(" {} ", link.glyph()),
                    if sel { base } else { Style::new().fg(link.color(t)) },
                ),
                Span::styled(format!("{name:<nw$}  "), base.add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("{word:<sw$}  "),
                    if sel || link.is_up() {
                        dim
                    } else {
                        Style::new().fg(link.color(t))
                    },
                ),
                Span::styled(how.clone(), dim),
            ];
            if !tags.is_empty() {
                spans.push(Span::styled(format!("  {tags}"), dim));
            }
            TextLine::from(spans).render(row, buf);
        }
        // What's wrong with the one under the cursor.
        if let Some(m) = list.get(self.at).filter(|m| !m.is_up()) {
            let say = m.trouble(now_ms);
            buf.set_stringn(
                inner.x + 1,
                inner.bottom().saturating_sub(2),
                &say,
                inner.width.saturating_sub(2) as usize,
                Style::new().fg(m.link().color(t)),
            );
        }
        let fw = (footer.width() as u16).min(inner.width);
        buf.set_stringn(
            inner.x + (inner.width - fw) / 2,
            inner.bottom() - 1,
            footer,
            fw as usize,
            Style::new().fg(t.muted),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn list() -> Vec<Machine> {
        parse(&json!({"machines": [
            {"host":"box","id":"a1","name":"vps","session":null,"state":"connected","target":"me@vps",
             "temporary":false,"version":"0.1.19","via":false},
            {"host":"mac","id":"b2","name":"sandbox","session":"scratch","since_ms":1000,"state":"unreachable",
             "target":"fly ssh console -C","temporary":true,"version":"0.1.19","via":true,
             "why":"cannot reach: timed out"},
            {"name":"odd","target":"x","id":"c","host":"h","version":"1","state":"exploded"},
        ]}))
    }

    #[test]
    fn reads_the_list() {
        let m = list();
        assert_eq!(m.len(), 3);
        assert!(m[0].is_up() && !m[1].is_up());
        assert_eq!(m[1].link(), Link::Unreachable);
        assert_eq!(m[2].link(), Link::Other("exploded".into()));
        assert_eq!(
            m[1].trouble(61_000),
            "sandbox unreachable for 1m: cannot reach: timed out"
        );
        assert!(parse(&json!({})).is_empty());
        assert!(same(&m, &list()) && same(&[], &[]));
        assert!(!same(&m[..2], &m), "one gone");
        let mut changed = list();
        changed[0].state = Some(Link::Unreachable);
        assert!(!same(&m, &changed), "a line dropped");
        assert_eq!(of("vps/t3"), Some("vps"));
        assert_eq!(of("t3"), None);
        assert_eq!(ago(5_000), "5s");
        assert_eq!(ago(7_200_000), "2h");
    }

    #[test]
    fn keys_and_drawing() {
        let m = list();
        let mut v = View::default();
        let k = |c| KeyEvent::new(c, KeyModifiers::NONE);
        assert_eq!(v.key(&k(KeyCode::Char('j')), 3), Outcome::Stay);
        assert_eq!(v.key(&k(KeyCode::Enter), 3), Outcome::Connect(1));
        assert_eq!(v.key(&k(KeyCode::Char('d')), 3), Outcome::Remove(1));
        assert_eq!(v.key(&k(KeyCode::Char('l')), 3), Outcome::Login(1));
        assert_eq!(
            v.key(&k(KeyCode::Char('n')), 0),
            Outcome::Add,
            "adding needs no machine"
        );
        assert_eq!(v.key(&k(KeyCode::Enter), 0), Outcome::Stay, "nothing to connect");
        assert_eq!(v.key(&k(KeyCode::Esc), 3), Outcome::Close);
        let theme = crate::theme::builtin("terminal").unwrap();
        let area = Rect::new(0, 0, 110, 14);
        let mut buf = Buffer::empty(area);
        v.render(&m, &[2, 0, 0], 61_000, area, &mut buf, &theme);
        let text: Vec<String> = (0..14)
            .map(|y| (0..110).map(|x| buf[(x, y)].symbol().to_string()).collect())
            .collect();
        let all = text.join("\n");
        assert!(all.contains(" machines "), "{all}");
        assert!(
            text.iter().any(|l| l.contains("● vps")
                && l.contains("connected")
                && l.contains("host box · hc 0.1.19 · 2 workspaces")),
            "{all}"
        );
        assert!(
            text.iter().any(|l| l.contains("✕ sandbox")
                && l.contains("via fly ssh console")
                && l.contains("session scratch · temporary")),
            "{all}"
        );
        assert!(
            all.contains("sandbox unreachable for 1m: cannot reach: timed out"),
            "{all}"
        );
        let mut buf = Buffer::empty(area);
        View::default().render(&[], &[], 0, area, &mut buf, &theme);
        let all: String = (0..14)
            .map(|y| (0..110).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>())
            .collect();
        assert!(all.contains("no other machines yet"), "{all}");
    }
}
