//! The tab at the top of each pane: what it's working on (an agent's summary) or where
//! it is (the directory's name), drawn on the border line above the pane with powerline
//! caps when the font has them. Pure.

use crate::config::{PaneTitle, TitleShape};
use crate::status;
use crate::theme::Theme;
use hc_proto::Terminal;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier, Style};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// What the tab says, if anything.
///
/// - `cwd`: the directory's name (`~` at home).
/// - `agent`: what the agent says it's doing (its status title or message, else the terminal
///   title it set, e.g. Claude Code's task summary), else its name. Nothing for a plain shell.
/// - `auto`: the agent's, when one is running; the directory otherwise.
pub fn text(mode: PaneTitle, t: Option<&Terminal>, vt_title: Option<&str>, home: &str) -> Option<String> {
    let t = t?;
    // A pane you've named is called that, whatever runs in it.
    if let Some(name) = t.labels.get(crate::model::L_NAME).filter(|n| !n.is_empty())
        && mode != PaneTitle::Off
    {
        return Some(name.clone());
    }
    match mode {
        PaneTitle::Off => None,
        PaneTitle::Cwd => dir_name(t, home),
        PaneTitle::Agent => agent_summary(t, vt_title),
        PaneTitle::Auto => agent_summary(t, vt_title).or_else(|| dir_name(t, home)),
    }
}

/// The live directory (the foreground program's), else where the terminal started.
pub fn dir_name(t: &Terminal, home: &str) -> Option<String> {
    let cwd = t
        .foreground
        .as_ref()
        .map(|f| f.cwd.as_str())
        .filter(|c| !c.is_empty())
        .unwrap_or(&t.cwd);
    if cwd.is_empty() {
        return None;
    }
    let cwd = cwd.trim_end_matches(['/', '\\']);
    if cwd.is_empty() {
        return Some("/".into());
    }
    if !home.is_empty() && cwd == home.trim_end_matches(['/', '\\']) {
        return Some("~".into());
    }
    cwd.rsplit(['/', '\\']).next().map(str::to_string)
}

pub fn agent_summary(t: &Terminal, vt_title: Option<&str>) -> Option<String> {
    status::agent(t)?;
    let s = t.status.as_ref();
    let said = s
        .and_then(|s| s.title.as_deref())
        .or_else(|| s.and_then(|s| s.msg.as_deref()))
        .or(vt_title)
        .or(t.title.as_deref())
        .map(clean)
        .filter(|s| !s.is_empty());
    Some(said.unwrap_or_else(|| status::agent_name(t)))
}

/// Without the spinner or mark agents put before their title (`✳ Fix login`, `⠂ Fix login`).
fn clean(s: &str) -> String {
    s.trim_start_matches(|c: char| !c.is_alphanumeric() && !"~/.([{\"'`".contains(c))
        .trim()
        .to_string()
}

/// The two ends of the tab. Powerline glyphs need a Nerd Font (or a font with them).
/// A pane's title as breadcrumbs: its name if you gave it one; an agent's folder, name and
/// what it's doing; else the folder's path (`~ › src › corgi`).
pub fn crumbs(mode: PaneTitle, t: Option<&Terminal>, vt_title: Option<&str>, home: &str) -> Option<Vec<String>> {
    let t = t?;
    if mode == PaneTitle::Off {
        return None;
    }
    if let Some(name) = t.labels.get(crate::model::L_NAME).filter(|n| !n.is_empty()) {
        return Some(vec![name.clone()]);
    }
    if mode != PaneTitle::Cwd
        && let Some(summary) = agent_summary(t, vt_title)
    {
        let name = status::agent_name(t);
        let mut out: Vec<String> = dir_name(t, home).into_iter().collect();
        out.push(name.clone());
        if summary != name {
            out.push(summary);
        }
        return Some(out);
    }
    if mode == PaneTitle::Agent {
        return None;
    }
    path_crumbs(t, home)
}

/// The folder's path as crumbs: `~` for home, `/` for the root.
fn path_crumbs(t: &Terminal, home: &str) -> Option<Vec<String>> {
    let cwd = t
        .foreground
        .as_ref()
        .map(|f| f.cwd.as_str())
        .filter(|c| !c.is_empty())
        .unwrap_or(&t.cwd);
    if cwd.is_empty() {
        return None;
    }
    let home = home.trim_end_matches('/');
    let (first, rest) = match cwd.strip_prefix(home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => ("~", rest),
        _ => ("/", cwd),
    };
    let mut out = vec![first.to_string()];
    out.extend(rest.split('/').filter(|p| !p.is_empty()).map(str::to_string));
    Some(out)
}

/// Breadcrumbs on their own row: `x`, `y`, `width`. The last crumb stands out (lit when the
/// pane is focused); when they don't fit, the first ones give way to `…`. The circle goes at
/// the row's end, as on a title line.
pub fn render_crumbs(
    buf: &mut Buffer,
    (x, y, width): (u16, u16, u16),
    crumbs: &[String],
    focused: bool,
    circle: Option<(&str, Color)>,
    t: &Theme,
) {
    const SEP: &str = " › ";
    let room = width.saturating_sub(if circle.is_some() { 4 } else { 1 } + 1) as usize;
    let fits = |c: &[String], lead: bool| {
        c.iter().map(|s| s.width()).sum::<usize>() + SEP.len() * (c.len() - 1) + if lead { 1 + SEP.len() } else { 0 }
            <= room
    };
    let mut start = 0;
    while start + 1 < crumbs.len() && !fits(&crumbs[start..], start > 0) {
        start += 1;
    }
    let shown = &crumbs[start..];
    let mut at = x + 1;
    let muted = Style::new().fg(t.muted);
    let mut put = |at: &mut u16, s: &str, style: Style| {
        let left = (x + 1 + room as u16).saturating_sub(*at);
        let s = truncate(s, left);
        buf.set_stringn(*at, y, &s, left as usize, style);
        *at += s.width() as u16;
    };
    if start > 0 {
        put(&mut at, "…", muted);
        put(&mut at, SEP, Style::new().fg(t.border));
    }
    for (i, c) in shown.iter().enumerate() {
        if i > 0 {
            put(&mut at, SEP, Style::new().fg(t.border));
        }
        let last = i + 1 == shown.len();
        let style = match (last, focused) {
            (true, true) => Style::new().fg(t.border_active).add_modifier(Modifier::BOLD),
            (true, false) => Style::new().fg(t.fg).add_modifier(Modifier::BOLD),
            _ => muted,
        };
        put(&mut at, c, style);
    }
    if let Some((c, color)) = circle
        && width >= 4
    {
        buf.set_stringn(x + width - 3, y, format!(" {c} "), 3, Style::new().fg(color));
    }
}

/// The spinner's frames, one per `SPIN_STEP`.
pub const SPIN: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// The indicator at the end of a pane's title, and its colour: what the pane is doing. Something
/// at work (an agent working, a command running) spins (`frame` steps it); a state is a solid
/// circle in its colour (waiting for you, finished unseen, idle); a shell at its prompt has none.
pub fn circle(a: status::Activity, frame: usize, t: &Theme) -> Option<(&'static str, Color)> {
    use status::{Activity, Agent};
    let spin = SPIN[frame % SPIN.len()];
    Some(match a {
        Activity::Agent(Agent::Blocked) => ("●", t.blocked),
        Activity::Agent(Agent::Working) => (spin, t.working),
        Activity::Agent(Agent::Done) => ("●", t.done),
        Activity::Agent(Agent::Idle) => ("●", t.idle),
        Activity::Agent(Agent::Unknown) => ("●", t.unknown),
        Activity::Running => (spin, t.muted),
        Activity::Prompt => return None,
    })
}

/// Whether `a`'s indicator spins.
pub fn spins(a: status::Activity) -> bool {
    matches!(
        a,
        status::Activity::Agent(status::Agent::Working) | status::Activity::Running
    )
}

/// The title as one line of text from `x` (` corgi `), and the circle at the line's end
/// (` ● `), on row `y` in `width` cells. The focused pane's title is lit.
pub fn render_line(
    buf: &mut Buffer,
    (x, y, width): (u16, u16, u16),
    text: &str,
    focused: bool,
    circle: Option<(&str, Color)>,
    t: &Theme,
) {
    let style = if focused {
        Style::new().fg(t.border_active).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(t.muted)
    };
    // The circle's three cells at the end; the title in what's left before them.
    let room = width.saturating_sub(3 + 2 + 1);
    if room >= 2 {
        let shown = format!(" {} ", truncate(text, room));
        buf.set_stringn(x, y, &shown, (room + 2) as usize, style);
    }
    if let Some((c, color)) = circle
        && width >= 4
    {
        buf.set_stringn(x + width - 3, y, format!(" {c} "), 3, Style::new().fg(color));
    }
}

/// Filled for the focused pane; the thin outline versions (a hollow pill) for the rest.
pub fn caps(shape: TitleShape, filled: bool) -> (&'static str, &'static str) {
    match (shape, filled) {
        (TitleShape::Rounded, true) => ("\u{e0b6}", "\u{e0b4}"),
        (TitleShape::Rounded, false) => ("\u{e0b7}", "\u{e0b5}"),
        (TitleShape::Angled, true) => ("\u{e0b2}", "\u{e0b0}"),
        (TitleShape::Angled, false) => ("\u{e0b3}", "\u{e0b1}"),
        (TitleShape::Slant, true) => ("\u{e0ba}", "\u{e0bc}"),
        (TitleShape::Slant, false) => ("\u{e0bb}", "\u{e0bb}"),
        (TitleShape::Plain | TitleShape::Line | TitleShape::Breadcrumbs, _) => ("", ""),
    }
}

pub struct Tab<'a> {
    pub text: &'a str,
    /// The agent's status glyph and colour.
    pub glyph: Option<(&'static str, Color)>,
    pub focused: bool,
    pub shape: TitleShape,
}

impl Tab<'_> {
    /// Draw on row `y`, from `x`, in at most `width` cells. Returns the cells used.
    pub fn render(&self, buf: &mut Buffer, x: u16, y: u16, width: u16, t: &Theme) -> u16 {
        let (l, r) = caps(self.shape, self.focused);
        let (cl, cr) = (l.width() as u16, r.width() as u16);
        // Focused: a solid pill. The rest: hollow, an outline in the border's colour, no fill.
        let (body, cap) = if self.focused {
            (
                Style::new()
                    .bg(t.border_active)
                    .fg(t.accent_fg)
                    .add_modifier(Modifier::BOLD),
                Style::new().fg(t.border_active).bg(Color::Reset),
            )
        } else {
            (
                Style::new().bg(Color::Reset).fg(t.muted),
                Style::new().fg(t.border).bg(Color::Reset),
            )
        };
        // Plain tabs pad themselves; capped tabs get their padding from the caps' curve.
        let pad = if self.shape == TitleShape::Plain { " " } else { "" };
        let glyph_w = self.glyph.map_or(0, |(g, _)| g.width() as u16 + 1);
        let room = width.saturating_sub(cl + cr + 2 * pad.len() as u16 + glyph_w);
        if room < 2 {
            return 0;
        }
        let text = truncate(self.text, room);
        let mut at = x;
        let mut put = |at: &mut u16, s: &str, style: Style| {
            for ch in s.chars() {
                let w = ch.width().unwrap_or(0) as u16;
                if w == 0 {
                    continue;
                }
                if let Some(c) = buf.cell_mut((*at, y)) {
                    c.reset();
                    c.set_char(ch).set_style(style);
                }
                for k in 1..w {
                    if let Some(c) = buf.cell_mut((*at + k, y)) {
                        c.reset();
                        c.set_symbol("").set_style(style);
                    }
                }
                *at += w;
            }
        };
        put(&mut at, l, cap);
        put(&mut at, pad, body);
        if let Some((g, color)) = self.glyph {
            let gs = if self.focused { body } else { body.fg(color) };
            put(&mut at, g, gs);
            put(&mut at, " ", body);
        }
        put(&mut at, &text, body);
        put(&mut at, pad, body);
        put(&mut at, r, cap);
        at - x
    }
}

/// At most `w` cells, with `…` when cut.
fn truncate(s: &str, w: u16) -> String {
    if s.width() as u16 <= w {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0) as u16;
        if used + cw + 1 > w {
            break;
        }
        out.push(ch);
        used += cw;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_proto::{Foreground, Status};

    fn shell(cwd: &str) -> Terminal {
        Terminal {
            address: "t1".into(),
            state: "live".into(),
            cwd: "/start".into(),
            pid: Some(10),
            foreground: Some(Foreground {
                pid: 10,
                name: "zsh".into(),
                argv: vec!["zsh".into()],
                cwd: cwd.into(),
            }),
            ..Default::default()
        }
    }

    fn agent(cwd: &str, title: Option<&str>) -> Terminal {
        let mut t = shell(cwd);
        t.foreground = Some(Foreground {
            pid: 11,
            name: "claude".into(),
            argv: vec!["claude".into()],
            cwd: cwd.into(),
        });
        t.status = Some(Status {
            state: "working".into(),
            app: Some("claude-code".into()),
            title: title.map(Into::into),
            ..Default::default()
        });
        t
    }

    #[test]
    fn directory_names() {
        let home = "/Users/me";
        assert_eq!(
            dir_name(&shell("/Users/me/src/kelpie"), home).as_deref(),
            Some("kelpie")
        );
        assert_eq!(dir_name(&shell("/Users/me/"), home).as_deref(), Some("~"));
        assert_eq!(dir_name(&shell("/"), home).as_deref(), Some("/"));
        let mut t = shell("");
        t.foreground.as_mut().unwrap().cwd.clear();
        assert_eq!(
            dir_name(&t, home).as_deref(),
            Some("start"),
            "where it started, without a live one"
        );
    }

    #[test]
    fn auto_prefers_the_agents_summary() {
        let home = "/Users/me";
        let a = agent("/Users/me/src/kelpie", Some("Fix the sidebar"));
        assert_eq!(
            text(PaneTitle::Auto, Some(&a), None, home).as_deref(),
            Some("Fix the sidebar")
        );
        assert_eq!(text(PaneTitle::Cwd, Some(&a), None, home).as_deref(), Some("kelpie"));
        // Without a status title: the terminal title it set, without its spinner.
        let a = agent("/x", None);
        assert_eq!(
            text(PaneTitle::Agent, Some(&a), Some("✳ Refactor keys"), home).as_deref(),
            Some("Refactor keys")
        );
        assert_eq!(
            text(PaneTitle::Agent, Some(&a), None, home).as_deref(),
            Some("claude-code")
        );
        // A shell: the directory for auto, nothing for agent.
        let s = shell("/Users/me/src");
        assert_eq!(
            text(PaneTitle::Auto, Some(&s), Some("me@host:~/src"), home).as_deref(),
            Some("src")
        );
        assert_eq!(text(PaneTitle::Agent, Some(&s), None, home), None);
        assert_eq!(text(PaneTitle::Off, Some(&s), None, home), None);
        // Named: the name, in every mode but off.
        let mut named = agent("/x", Some("Fixing"));
        named.labels.insert(crate::model::L_NAME.into(), "reviewer".into());
        assert_eq!(
            text(PaneTitle::Auto, Some(&named), None, home).as_deref(),
            Some("reviewer")
        );
        assert_eq!(
            text(PaneTitle::Cwd, Some(&named), None, home).as_deref(),
            Some("reviewer")
        );
        assert_eq!(text(PaneTitle::Off, Some(&named), None, home), None);
    }

    #[test]
    fn breadcrumbs_of_folders_and_agents() {
        let home = "/Users/me";
        assert_eq!(
            crumbs(PaneTitle::Auto, Some(&shell("/Users/me/src/kelpie")), None, home).unwrap(),
            ["~", "src", "kelpie"]
        );
        assert_eq!(
            crumbs(PaneTitle::Auto, Some(&shell("/usr/local")), None, home).unwrap(),
            ["/", "usr", "local"]
        );
        assert_eq!(
            crumbs(PaneTitle::Auto, Some(&shell("/Users/me")), None, home).unwrap(),
            ["~"]
        );
        let a = agent("/Users/me/src/kelpie", Some("Fix the sidebar"));
        assert_eq!(
            crumbs(PaneTitle::Auto, Some(&a), None, home).unwrap(),
            ["kelpie", "claude-code", "Fix the sidebar"]
        );
        assert_eq!(
            crumbs(PaneTitle::Cwd, Some(&a), None, home).unwrap(),
            ["~", "src", "kelpie"]
        );
        assert_eq!(crumbs(PaneTitle::Agent, Some(&shell("/x")), None, home), None);
        let mut named = shell("/x");
        named.labels.insert(crate::model::L_NAME.into(), "build".into());
        assert_eq!(crumbs(PaneTitle::Auto, Some(&named), None, home).unwrap(), ["build"]);
    }

    #[test]
    fn breadcrumbs_render_and_give_way_from_the_left() {
        let theme = crate::theme::builtin("terminal").unwrap();
        let c: Vec<String> = ["~", "workspace", "kelpie"].map(String::from).to_vec();
        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, 40, 1));
        render_crumbs(&mut buf, (0, 0, 40), &c, true, Some(("◐", theme.working)), &theme);
        let row: String = (0..40).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert!(row.starts_with(" ~ › workspace › kelpie"), "{row:?}");
        assert!(row.ends_with(" ◐ "), "{row:?}");
        let kelpie = row.find("kelpie").map(|b| row[..b].chars().count()).unwrap() as u16;
        assert_eq!(buf[(kelpie, 0)].fg, theme.border_active, "the last crumb, lit");
        assert_eq!(buf[(1, 0)].fg, theme.muted, "the others muted");
        // Narrow: the first ones go.
        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, 20, 1));
        render_crumbs(&mut buf, (0, 0, 20), &c, false, None, &theme);
        let row: String = (0..20).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert_eq!(row.trim_end(), " … › kelpie");
    }

    #[test]
    fn a_line_with_a_circle_at_its_end() {
        let theme = crate::theme::builtin("terminal").unwrap();
        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, 30, 1));
        render_line(&mut buf, (1, 0, 28), "kelpie", true, Some(("●", theme.blocked)), &theme);
        let row: String = (0..30).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert_eq!(row, format!("  kelpie{}●  ", " ".repeat(19)), "the circle at column 27");
        assert_eq!(buf[(3, 0)].fg, theme.border_active, "focused: lit");
        assert_eq!(buf[(27, 0)].fg, theme.blocked);
        // Long titles stop before the circle.
        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, 16, 1));
        render_line(
            &mut buf,
            (0, 0, 16),
            "a long title to cut",
            false,
            Some(("◐", theme.working)),
            &theme,
        );
        let row: String = (0..16).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert_eq!(row, " a long ti…   ◐ ");
        assert_eq!(buf[(1, 0)].fg, theme.muted);
    }

    #[test]
    fn circles_by_activity() {
        use status::{Activity, Agent};
        let t = crate::theme::builtin("terminal").unwrap();
        assert_eq!(circle(Activity::Agent(Agent::Blocked), 0, &t), Some(("●", t.blocked)));
        assert_eq!(circle(Activity::Agent(Agent::Done), 0, &t), Some(("●", t.done)));
        assert_eq!(
            circle(Activity::Agent(Agent::Idle), 0, &t),
            Some(("●", t.idle)),
            "solid"
        );
        assert_eq!(circle(Activity::Prompt, 0, &t), None);
        // At work: a spinner, a step per frame; an agent in the working colour, a command muted.
        let spin: Vec<_> = (0..3)
            .map(|f| circle(Activity::Agent(Agent::Working), f, &t).unwrap())
            .collect();
        assert_eq!(spin, [("⠋", t.working), ("⠙", t.working), ("⠹", t.working)]);
        assert_eq!(circle(Activity::Running, 1, &t), Some(("⠙", t.muted)));
        assert!(spins(Activity::Running) && spins(Activity::Agent(Agent::Working)) && !spins(Activity::Prompt));
    }

    #[test]
    fn renders_caps_and_truncates() {
        let theme = crate::theme::builtin("terminal").unwrap();
        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, 20, 1));
        let tab = Tab {
            text: "kelpie",
            glyph: None,
            focused: true,
            shape: TitleShape::Rounded,
        };
        assert_eq!(tab.render(&mut buf, 1, 0, 18, &theme), 8);
        let row: String = (0..20).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert_eq!(row.trim_end(), " \u{e0b6}kelpie\u{e0b4}");
        assert_eq!(buf[(1, 0)].fg, theme.border_active, "the cap is the tab's colour");
        assert_eq!(buf[(2, 0)].bg, theme.border_active);

        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, 12, 1));
        let tab = Tab {
            text: "a long summary of work",
            glyph: Some(("●", Color::Yellow)),
            focused: false,
            shape: TitleShape::Plain,
        };
        let used = tab.render(&mut buf, 0, 0, 12, &theme);
        let row: String = (0..12).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert_eq!(used, 12);
        assert_eq!(row, " ● a long … ");
        assert_eq!(buf[(1, 0)].fg, Color::Yellow);

        // Out of focus: the outline caps, no fill.
        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, 20, 1));
        let tab = Tab {
            text: "kelpie",
            glyph: None,
            focused: false,
            shape: TitleShape::Rounded,
        };
        tab.render(&mut buf, 0, 0, 20, &theme);
        let row: String = (0..20).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert_eq!(row.trim_end(), "\u{e0b7}kelpie\u{e0b5}");
        assert_eq!(buf[(0, 0)].fg, theme.border);
        assert_eq!(buf[(1, 0)].bg, Color::Reset, "hollow");
    }
}
