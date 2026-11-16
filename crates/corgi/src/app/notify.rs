//! Notifications: what programs ask for (OSC 9 / OSC 777) and what agents' status says
//! (needs you, finished), shown as toasts inside corgi and passed on to the host terminal.
//!
//! Nothing is shown for the pane you're looking at. The host terminal is told only while
//! its window isn't focused (or, if it never says, for panes you aren't looking at): inside
//! corgi, the toast is enough.

use super::App;
use crate::host_notify::{self, Proto};
use crate::status::{self, Agent};
use crate::toast::{Kind, Toast};
use hc_proto::Terminal;
use std::time::Instant;

/// How notifications leave for the host terminal, decided once from the config and environment.
pub(super) struct HostOut {
    pub proto: Proto,
    pub tmux: bool,
    /// Numbers kitty notifications, so one doesn't replace another.
    pub next_id: u64,
    /// When each pane last went out: an agent that reports its status and sends an OSC 9
    /// for the same moment makes one desktop notification, as it makes one toast.
    last: std::collections::HashMap<String, Instant>,
}

/// Notices about one pane within this long of each other reach the host once.
const HOST_DEDUPE: std::time::Duration = std::time::Duration::from_secs(2);

impl HostOut {
    pub fn new(setting: crate::config::HostNotify) -> HostOut {
        HostOut {
            proto: host_notify::resolve(setting, |k| std::env::var(k).ok()),
            tmux: std::env::var_os("TMUX").is_some(),
            next_id: 1,
            last: Default::default(),
        }
    }
}

impl App {
    /// Whether you're looking at `pane` right now.
    fn looking_at(&self, pane: &str) -> bool {
        self.host_focus && self.sidebar_focus.is_none() && self.window().is_some_and(|w| w.focus == pane)
    }

    /// `workspace:index` of the window `pane` is in.
    fn whereabouts(&self, pane: &str) -> String {
        let Some((si, wi)) = self.doc.locate(pane) else {
            return String::new();
        };
        format!("{}:{wi}", self.ws_label(&self.doc.workspaces[si]))
    }

    /// A pane's program asked for desktop notifications.
    pub(super) fn pane_notifications(&mut self, pane: &str, notes: Vec<corgi_vt::Notification>) {
        for n in notes {
            let title = if n.title.is_empty() {
                self.terminals
                    .get(pane)
                    .map(status::agent_name)
                    .unwrap_or_else(|| self.pane_name(pane))
            } else {
                n.title
            };
            self.alert(pane, Kind::Info, title, n.body);
        }
    }

    /// A terminal's record changed from `before`: toast if its agent now needs you or finished.
    pub(super) fn agent_changed(&mut self, before: Option<&Terminal>, after: &Terminal) {
        if !self.config.notifications.agents || self.doc.locate(&after.address).is_none() {
            return;
        }
        let was = before.and_then(status::agent);
        let now = status::agent(after);
        // Done again after a new turn counts too: `turn` moved on.
        let new_turn = before.is_none_or(|b| b.turn < after.turn);
        let kind = match now {
            Some(Agent::Blocked) if was != Some(Agent::Blocked) => Kind::Blocked,
            Some(Agent::Done) if was != Some(Agent::Done) || new_turn => Kind::Done,
            _ => return,
        };
        let name = status::agent_name(after);
        let s = after.status.as_ref();
        let title = match kind {
            Kind::Blocked => format!("{name} needs you"),
            _ => format!("{name} finished"),
        };
        let body = s
            .and_then(|s| s.msg.clone().or(s.title.clone()))
            .or_else(|| s.and_then(|s| s.kind.clone()))
            .unwrap_or_default();
        self.alert(&after.address, kind, title, body);
    }

    /// Tell you about `pane`, unless you're looking at it.
    fn alert(&mut self, pane: &str, kind: Kind, title: String, body: String) {
        if self.looking_at(pane) {
            return;
        }
        tracing::info!(pane, ?kind, title, body, "notification");
        let cfg = &self.config.notifications;
        if cfg.toasts {
            let toast = Toast {
                pane: Some(pane.to_string()),
                kind,
                title: title.clone(),
                place: self.whereabouts(pane),
                body: body.clone(),
                until: Instant::now(),
            };
            let life = cfg.duration;
            self.toasts.push(toast, Instant::now(), life);
        }
        // Focus reports tell us when you're in another window; without them, assume you might be.
        if !self.host_focus || !self.focus_known {
            let h = &mut self.host_out;
            let now = Instant::now();
            h.last.retain(|_, t| now.duration_since(*t) < HOST_DEDUPE);
            if h.last.insert(pane.to_string(), now).is_some() {
                return;
            }
            let bytes = host_notify::encode(h.proto, &title, &body, h.next_id, h.tmux);
            h.next_id += 1;
            crate::term::host_write(&bytes);
        }
    }

    /// Take a toast's click: go to its pane. True if there was a toast there.
    pub(super) fn click_toast(&mut self, at: ratatui::layout::Position) -> bool {
        let Some((_, pane)) = self.hits.toasts.iter().find(|(r, _)| r.contains(at)).cloned() else {
            return false;
        };
        match pane {
            Some(p) if self.doc.locate(&p).is_some() => {
                self.toasts.dismiss_pane(&p);
                self.show_terminal(&p);
            }
            Some(p) => {
                self.toasts.dismiss_pane(&p);
            }
            None => {}
        }
        true
    }
}
