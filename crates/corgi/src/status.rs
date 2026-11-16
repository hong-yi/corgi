//! Agent status as corgi shows it, from what programs report (OSC 7501) and
//! whether you've looked since. Pure.
//!
//! Done vs idle: a program that ends a turn (`turn` goes up) while you look
//! elsewhere is *done* until you look at it; then it's *idle*. "Looked at" is
//! the `herdr.seen` label (the turn last seen), shared with the herdr app.

use crate::config::Indicators;
use crate::theme::Theme;
use hc_proto::Terminal;
use ratatui::style::Color;

pub const L_SEEN: &str = "herdr.seen";

/// In urgency order, most urgent last (so `max` is the rollup).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Agent {
    Unknown,
    Idle,
    Working,
    Done,
    Blocked,
}

pub fn seen(t: &Terminal) -> u64 {
    t.labels.get(L_SEEN).and_then(|s| s.parse().ok()).unwrap_or(0)
}

/// `None` when there's no status now: plain shells and editors that never reported, and
/// agents that cleared theirs on the way out (core keeps `reported` true for those, but
/// the pane is back at a shell; it isn't an agent any more).
pub fn agent(t: &Terminal) -> Option<Agent> {
    let s = t.status.as_ref()?;
    if back_at_shell(t) {
        return None;
    }
    Some(match s.state.as_str() {
        "blocked" | "error" => Agent::Blocked,
        "working" => Agent::Working,
        "done" | "idle" if t.turn > seen(t) => Agent::Done,
        "done" | "idle" => Agent::Idle,
        _ => Agent::Unknown,
    })
}

/// Shells, by program name: what a pane runs when no agent is in the foreground.
const SHELLS: &[&str] = &[
    "zsh",
    "bash",
    "sh",
    "dash",
    "fish",
    "nu",
    "pwsh",
    "powershell",
    "cmd",
    "elvish",
    "xonsh",
    "tcsh",
    "csh",
    "ksh",
    "ion",
];

/// A program name without its path, login dash (`-zsh`) or `.exe`.
fn program_name(s: &str) -> &str {
    let base = s.rsplit(['/', '\\']).next().unwrap_or(s);
    base.trim_start_matches('-').trim_end_matches(".exe")
}

/// The terminal's own shell is in the foreground again, so whatever agent reported its status
/// has exited, whether or not it cleared the status on the way out (any harness, killed or not).
/// A terminal started as an agent directly (`hc run codex`) isn't a shell, so it's unaffected.
fn back_at_shell(t: &Terminal) -> bool {
    let Some(f) = &t.foreground else { return false };
    let shell =
        SHELLS.contains(&program_name(&f.name)) || f.argv.first().is_some_and(|a| SHELLS.contains(&program_name(a)));
    shell && t.pid.is_some_and(|pid| pid == f.pid)
}

/// What a pane is doing: an agent's state, a program running in its shell, or the shell's prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Agent(Agent),
    Running,
    Prompt,
}

pub fn activity(t: &Terminal) -> Activity {
    if let Some(a) = agent(t) {
        return Activity::Agent(a);
    }
    match &t.foreground {
        // Something other than the terminal's own program is in front: a command running.
        Some(f) if t.pid.is_some_and(|pid| pid != f.pid) => Activity::Running,
        // Started as a program itself (not a shell), and still going.
        Some(f) if !SHELLS.contains(&program_name(&f.name)) => Activity::Running,
        _ => Activity::Prompt,
    }
}

/// The most urgent of several.
pub fn rollup<'a>(ts: impl IntoIterator<Item = &'a Terminal>) -> Option<Agent> {
    ts.into_iter().filter_map(agent).max()
}

/// What an agent is called: what it says it is, else what's running.
pub fn agent_name(t: &Terminal) -> String {
    t.status
        .as_ref()
        .and_then(|s| s.app.clone())
        .or_else(|| t.foreground.as_ref().map(|f| f.name.clone()))
        .or_else(|| t.argv.first().cloned())
        .unwrap_or_else(|| t.address.clone())
}

pub fn glyph(a: Agent, style: Indicators) -> &'static str {
    match (style, a) {
        (Indicators::Dots, Agent::Idle) => "○",
        (Indicators::Dots, Agent::Unknown) => "·",
        (Indicators::Dots, _) => "●",
        (Indicators::Symbols, Agent::Blocked) => "×",
        (Indicators::Symbols, Agent::Done) => "✓",
        (Indicators::Symbols, Agent::Working) => "◐",
        (Indicators::Symbols, Agent::Idle) => "○",
        (Indicators::Symbols, Agent::Unknown) => "·",
    }
}

pub fn color(a: Agent, t: &Theme) -> Color {
    match a {
        Agent::Blocked => t.blocked,
        Agent::Done => t.done,
        Agent::Working => t.working,
        Agent::Idle => t.idle,
        Agent::Unknown => t.unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_proto::Status;

    fn t(state: Option<&str>, turn: u64, seen: Option<u64>) -> Terminal {
        let mut t = Terminal {
            status: state.map(|s| Status {
                state: s.into(),
                ..Default::default()
            }),
            reported: state.is_some(),
            turn,
            ..Default::default()
        };
        if let Some(s) = seen {
            t.labels.insert(L_SEEN.into(), s.to_string());
        }
        t
    }

    #[test]
    fn states_and_done_until_seen() {
        assert_eq!(agent(&t(None, 0, None)), None, "never reported: not an agent");
        assert_eq!(agent(&t(Some("working"), 3, None)), Some(Agent::Working));
        assert_eq!(agent(&t(Some("blocked"), 3, None)), Some(Agent::Blocked));
        assert_eq!(agent(&t(Some("error"), 3, None)), Some(Agent::Blocked));
        assert_eq!(
            agent(&t(Some("idle"), 4, Some(3))),
            Some(Agent::Done),
            "a turn ended unseen"
        );
        assert_eq!(agent(&t(Some("idle"), 4, Some(4))), Some(Agent::Idle), "seen");
        assert_eq!(agent(&t(Some("done"), 0, None)), Some(Agent::Idle), "no turn yet");
        assert_eq!(agent(&t(Some("thinking"), 0, None)), Some(Agent::Unknown));
        // Claude Code exits and clears its status; the shell is back. Core still says it reported.
        let mut cleared = t(None, 2, None);
        cleared.reported = true;
        assert_eq!(agent(&cleared), None, "gone, not unknown");
    }

    fn fg(t: &mut Terminal, pid: i64, argv0: &str) {
        t.foreground = Some(hc_proto::Foreground {
            pid,
            name: program_name(argv0).into(),
            argv: vec![argv0.into()],
            cwd: "/".into(),
        });
    }

    #[test]
    fn any_agent_is_gone_once_its_shell_is_back_in_front() {
        // An agent running in the pane's shell (pid 10): it's in the foreground, it counts.
        let mut x = t(Some("idle"), 1, Some(1));
        x.pid = Some(10);
        fg(&mut x, 77, "/usr/local/bin/codex");
        assert_eq!(agent(&x), Some(Agent::Idle));
        // It quits without clearing its status (killed, or a harness that never clears):
        // the shell is in front again, so the left-over status no longer counts.
        fg(&mut x, 10, "-zsh");
        assert_eq!(agent(&x), None);
        fg(&mut x, 10, "/bin/bash");
        assert_eq!(agent(&x), None);
        // A shell the agent started itself (a different pid) doesn't hide it.
        fg(&mut x, 88, "/bin/sh");
        assert_eq!(agent(&x), Some(Agent::Idle));
        // A terminal that *is* the agent (`hc run claude`): its own pid, but not a shell.
        let mut direct = t(Some("working"), 0, None);
        direct.pid = Some(20);
        fg(&mut direct, 20, "claude");
        assert_eq!(agent(&direct), Some(Agent::Working));
    }

    #[test]
    fn rollup_is_the_most_urgent() {
        let ts = [
            t(Some("idle"), 1, Some(1)),
            t(Some("working"), 0, None),
            t(None, 0, None),
        ];
        assert_eq!(rollup(&ts), Some(Agent::Working));
        let ts = [
            t(Some("working"), 0, None),
            t(Some("idle"), 2, Some(1)),
            t(Some("blocked"), 0, None),
        ];
        assert_eq!(rollup(&ts), Some(Agent::Blocked));
        assert_eq!(rollup(&[t(None, 0, None)]), None);
    }

    #[test]
    fn glyphs() {
        assert_eq!(glyph(Agent::Done, Indicators::Symbols), "✓");
        assert_eq!(glyph(Agent::Idle, Indicators::Dots), "○");
        assert_eq!(glyph(Agent::Blocked, Indicators::Dots), "●");
    }
}
