//! The tab bar's usage segments: one loop per segment reads its source off the UI task
//! every `interval` and sends the reading here; `agents` is counted from status as drawn.
//! A reload starts them over (a new generation; the old loops see it and stop).

use super::*;
use crate::status::Agent;
use crate::usage::{Reading, fill};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Where segments that follow the workspace read: this machine, another (and the connection
/// to reach it through), or another whose line is down (nothing to read there now).
#[derive(Clone)]
pub(super) enum Where {
    Here,
    There(String, Client),
    Down(String),
}

impl Where {
    fn same(&self, other: &Where) -> bool {
        match (self, other) {
            (Where::Here, Where::Here) => true,
            (Where::There(a, c), Where::There(b, _)) => a == b && c.is_open(),
            (Where::Down(a), Where::Down(b)) => a == b,
            _ => false,
        }
    }
}

pub(super) struct State {
    generation: Arc<AtomicU64>,
    /// The active workspace's machine, with a revision: watched by the segments that follow it
    /// (a change mid-read isn't missed), and on readings, so one from before a change is dropped.
    target: tokio::sync::watch::Sender<(u64, Where)>,
    segments: Vec<crate::config::Segment>,
    /// Each segment's latest reading and when it came.
    readings: Vec<Option<(Reading, Instant)>>,
}

impl Default for State {
    fn default() -> State {
        State {
            generation: Default::default(),
            target: tokio::sync::watch::channel((0, Where::Here)).0,
            segments: Vec::new(),
            readings: Vec::new(),
        }
    }
}

impl App {
    /// (Re)start every segment's loop, for the config as it is now.
    pub(super) fn start_usage(&mut self) {
        let generation = self.usage.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let segments = self.config.usage.segments();
        self.usage.readings = vec![None; segments.len()];
        for (i, seg) in segments.iter().enumerate() {
            let Some(mut source) = crate::usage_sources::Source::new(seg) else {
                continue;
            };
            let (out, current, every) = (self.out.clone(), self.usage.generation.clone(), seg.interval);
            let mut target = seg.follows_workspace().then(|| self.usage.target.subscribe());
            let secs = seg.timeout.as_secs().max(1);
            tokio::spawn(async move {
                while current.load(Ordering::SeqCst) == generation {
                    let (rev, at) = match &mut target {
                        Some(rx) => rx.borrow_and_update().clone(),
                        None => (0, Where::Here),
                    };
                    let remote = source.remote_line();
                    // `None`: the target changed meanwhile, read again for the new one.
                    let reading = match (&at, remote) {
                        (Where::There(m, c), Some(line)) => {
                            let rx = target.as_mut().expect("follows the workspace");
                            tokio::select! {
                                r = super::worktrees::exec_for(c, &line, Some(m), secs) => {
                                    Some(r.ok().and_then(|o| source.remote_reading(&o)).map(|mut r| {
                                        r.detail.insert(0, (format!("on {m}"), false));
                                        r
                                    }))
                                }
                                ch = rx.changed() => {
                                    if ch.is_err() {
                                        return;
                                    }
                                    None
                                }
                            }
                        }
                        // Its line is down: nothing to read there; what was read last goes stale.
                        (Where::Down(_), Some(_)) => {
                            let rx = target.as_mut().expect("follows the workspace");
                            if rx.changed().await.is_err() {
                                return;
                            }
                            continue;
                        }
                        _ => {
                            let Ok((s, reading)) = tokio::task::spawn_blocking(move || {
                                let r = source.read(std::time::SystemTime::now());
                                (source, r)
                            })
                            .await
                            else {
                                return;
                            };
                            source = s;
                            Some(reading)
                        }
                    };
                    let Some(reading) = reading else { continue };
                    if out.send(Msg::Usage(generation, i, rev, reading)).is_err() {
                        return;
                    }
                    match &mut target {
                        Some(rx) => {
                            tokio::select! {
                                _ = tokio::time::sleep(every) => {}
                                ch = rx.changed() => if ch.is_err() { return },
                            }
                        }
                        None => tokio::time::sleep(every).await,
                    }
                }
            });
        }
        self.usage.segments = segments;
        self.usage_target();
    }

    /// Point the segments that follow the workspace at its machine: this one, another, or
    /// another whose line is down.
    pub(super) fn usage_target(&self) {
        let machine = self
            .active()
            .and_then(|(ws, _)| self.doc.workspace(&ws).and_then(|s| s.machine.clone()));
        let at = match machine {
            None => Where::Here,
            Some(m) if self.machines.iter().any(|x| x.name == m && x.is_up()) => Where::There(m, self.client.clone()),
            Some(m) => Where::Down(m),
        };
        let rev = {
            let now = self.usage.target.borrow();
            if now.1.same(&at) {
                return;
            }
            now.0 + 1
        };
        self.usage.target.send_replace((rev, at));
    }

    pub(super) fn usage_arrived(&mut self, generation: u64, i: usize, rev: u64, r: Option<Reading>) {
        if generation != self.usage.generation.load(Ordering::SeqCst) {
            return;
        }
        let Some(seg) = self.usage.segments.get(i) else { return };
        // Read for a machine the workspace was on before: not this one's.
        if seg.follows_workspace() && rev != self.usage.target.borrow().0 {
            return;
        }
        let r = r.map(|mut r| {
            if let Some(f) = &seg.format {
                r.full = fill(f, &r.fields);
            }
            if let Some(s) = &seg.short {
                r.short = fill(s, &r.fields);
            }
            (r, Instant::now())
        });
        if let Some(slot) = self.usage.readings.get_mut(i) {
            *slot = r;
        }
    }

    /// Agents by state, across every workspace: working, waiting for you, finished unseen.
    fn agents_reading(&self) -> Option<Reading> {
        let (mut working, mut blocked, mut done, mut idle) = (0, 0, 0, 0);
        for s in &self.doc.workspaces {
            for w in &s.windows {
                for p in w.root.panes() {
                    match self.terminals.get(p).and_then(crate::status::agent) {
                        Some(Agent::Working) => working += 1,
                        Some(Agent::Blocked) => blocked += 1,
                        Some(Agent::Done) => done += 1,
                        Some(Agent::Idle | Agent::Unknown) => idle += 1,
                        None => {}
                    }
                }
            }
        }
        if working + blocked + done + idle == 0 {
            return None;
        }
        let parts = |pairs: &[(usize, String)], sep: &str| {
            pairs
                .iter()
                .filter(|(n, _)| *n > 0)
                .map(|(_, s)| s.clone())
                .collect::<Vec<_>>()
                .join(sep)
        };
        let full = parts(
            &[
                (blocked, format!("{blocked} waiting")),
                (working, format!("{working} working")),
                (done, format!("{done} done")),
            ],
            " · ",
        );
        let short = parts(
            &[
                (blocked, format!("!{blocked}")),
                (working, format!("●{working}")),
                (done, format!("✓{done}")),
            ],
            " ",
        );
        let n = |v: usize| v.to_string();
        Some(Reading {
            fields: [
                ("working".to_string(), n(working)),
                ("waiting".to_string(), n(blocked)),
                ("done".to_string(), n(done)),
                ("idle".to_string(), n(idle)),
            ]
            .into(),
            full: if full.is_empty() { format!("{idle} idle") } else { full },
            short,
            icon: format!("●{}", working + blocked + done + idle),
            detail: vec![(
                format!("{blocked} waiting for you · {working} working · {done} finished unseen · {idle} idle"),
                false,
            )],
        })
    }

    /// Each segment's reading now (agents counted afresh), and whether it's stale.
    fn usage_now(&self) -> Vec<(&crate::config::Segment, Option<(Reading, bool)>)> {
        let agents = self.agents_reading();
        self.usage
            .segments
            .iter()
            .enumerate()
            .map(|(i, seg)| {
                let r = if seg.provider == "agents" {
                    agents.clone().map(|mut r| {
                        if let Some(f) = &seg.format {
                            r.full = fill(f, &r.fields);
                        }
                        if let Some(s) = &seg.short {
                            r.short = fill(s, &r.fields);
                        }
                        (r, false)
                    })
                } else {
                    self.usage
                        .readings
                        .get(i)
                        .cloned()
                        .flatten()
                        .map(|(r, at)| (r, at.elapsed() > seg.interval * 3))
                };
                (seg, r)
            })
            .collect()
    }

    /// What fits in `free` columns: each placed segment's text, whether it's stale, its colours.
    pub(super) fn usage_placed(&self, free: usize) -> Vec<(String, bool, ratatui::style::Style)> {
        let now = self.usage_now();
        let segs: Vec<(i32, Option<&Reading>)> = now
            .iter()
            .map(|(s, r)| (s.priority, r.as_ref().map(|(r, _)| r)))
            .collect();
        crate::usage::place(free, &segs)
            .into_iter()
            .map(|(i, text)| {
                let (seg, r) = &now[i];
                (
                    text,
                    r.as_ref().is_some_and(|(_, stale)| *stale),
                    self.segment_style(seg),
                )
            })
            .collect()
    }

    /// A segment's block colours: its own `bg`/`fg`, else its provider's.
    fn segment_style(&self, seg: &crate::config::Segment) -> ratatui::style::Style {
        use ratatui::style::{Color, Style};
        let t = &self.theme;
        let parse = |c: &Option<String>| c.as_deref().and_then(crate::theme::parse_color);
        if seg.bg.as_deref() == Some("none") {
            return Style::new().bg(t.surface).fg(parse(&seg.fg).unwrap_or(t.muted));
        }
        let bg = parse(&seg.bg).unwrap_or(match seg.provider.as_str() {
            "agents" => t.accent,
            // Claude's own coral.
            "claude" => Color::Rgb(0xd9, 0x77, 0x57),
            "codex" => t.badge,
            "command" => t.border_active,
            _ => t.border,
        });
        let fg = parse(&seg.fg).unwrap_or(if bg == t.border { t.fg } else { t.accent_fg });
        Style::new().bg(bg).fg(fg)
    }

    /// `C-b u`: everything each segment knows, in a dialog.
    pub(super) fn open_usage(&mut self) {
        let mut lines: Vec<(String, bool)> = Vec::new();
        for (seg, r) in self.usage_now() {
            if !lines.is_empty() {
                lines.push((String::new(), false));
            }
            match r {
                Some((r, stale)) => {
                    let note = if stale { "  (not updated lately)" } else { "" };
                    lines.push((format!("{}: {}{note}", seg.provider, r.full), false));
                    lines.extend(r.detail.iter().map(|(d, warn)| (format!("  {d}"), *warn)));
                }
                None => lines.push((format!("{}: nothing to show", seg.provider), false)),
            }
        }
        if lines.is_empty() {
            lines.push(("no usage segments ([usage] in the config)".into(), false));
        }
        lines.push((String::new(), false));
        lines.push((format!("connected to core {}", self.core_version), false));
        let form = crate::form::Form::confirm("Usage", "close", lines);
        self.mode = Mode::Dialog(dialogs::Dialog {
            does: dialogs::Does::Info,
            form,
            busy: false,
        });
    }
}
