//! Which action menu opens where: `C-b m` (the focused pane), `m` on a sidebar row, a
//! right-click. A pane's menu has its window's and workspace's too; a window's, its
//! workspace's. The entries are commands, keyed like the prefix keys that do the same.

use super::*;
use crate::menu::{Entry, Menu, Section};
use crate::tree::Target;

fn section(title: &str, entries: &[(char, &str, &str)]) -> Section {
    Section {
        title: title.into(),
        entries: entries
            .iter()
            .map(|&(key, label, command)| Entry {
                key,
                label: label.into(),
                command: command.into(),
            })
            .collect(),
    }
}

fn pane_section() -> Section {
    section(
        "Pane",
        &[
            ('%', "split right", "split-window -h"),
            ('"', "split down", "split-window -v"),
            ('z', "zoom", "resize-pane -Z"),
            ('r', "rename pane…", "rename-pane"),
            ('!', "break into its own window", "break-pane"),
            ('x', "close pane", "kill-pane"),
        ],
    )
}

fn window_section() -> Section {
    section(
        "Window",
        &[
            ('c', "new window", "new-window"),
            ('C', "new window…", "dialog new-window"),
            (',', "rename window…", "rename-window"),
            ('l', "next layout", "next-layout"),
            ('&', "close window", "kill-window"),
        ],
    )
}

fn workspace_section() -> Section {
    section(
        "Workspace",
        &[
            ('N', "new workspace…", "dialog new-workspace"),
            ('W', "new worktree workspace…", "dialog new-worktree"),
            ('$', "rename workspace…", "rename-workspace"),
            ('X', "close workspace", "kill-workspace"),
        ],
    )
}

/// A machine row's: what's done to it (the commands name it, whatever's current).
fn machine_section(m: &str) -> Section {
    let q = crate::worktree::quote(m);
    let entries = [
        ('N', "new workspace on it…", format!("machine new-workspace {q}")),
        ('c', "connect now", format!("machine connect {q}")),
        ('l', "log in…", format!("machine login {q}")),
        ('u', "update hc there…", format!("machine update {q}")),
        ('r', "rename…", format!("machine rename {q}")),
        ('X', "forget it…", format!("machine remove {q}")),
    ];
    Section {
        title: "Machine".into(),
        entries: entries
            .into_iter()
            .map(|(key, label, command)| Entry {
                key,
                label: label.into(),
                command,
            })
            .collect(),
    }
}

fn machines_section() -> Section {
    section(
        "Machines",
        &[
            ('M', "machines…", "choose-machine"),
            ('a', "add a machine…", "dialog new-machine"),
        ],
    )
}

impl App {
    /// The menu for `target` (a sidebar row, a tab, a pane), or for the focused pane.
    pub(super) fn open_menu(&mut self, target: Option<Target>) {
        let here = || {
            let (ws, win) = self.active()?;
            let pane = self.window()?.focus.clone();
            Some(Target::Pane { ws, win, pane })
        };
        let Some(t) = target.clone().or_else(here) else {
            // Nothing open: only making things makes sense.
            let menu = Menu::new("corgi", vec![workspace_section()]);
            self.mode = Mode::Menu(menu, None);
            return;
        };
        let ws_label = |ws: &str| self.doc.workspace(ws).map(|s| self.ws_label(s)).unwrap_or_default();
        let win_label = |ws: &str, win: &str| {
            let s = self.doc.workspace(ws);
            let i = s.and_then(|s| s.windows.iter().position(|w| w.id == win)).unwrap_or(0);
            let name = s
                .and_then(|s| s.window(win))
                .map(|w| w.name.clone().unwrap_or_else(|| self.pane_name(&w.focus)))
                .unwrap_or_default();
            format!("{}:{name}", i + self.config.keys.preset.base_index())
        };
        let (title, sections) = match &t {
            Target::Pane { ws, win, pane } => (
                format!("{} · {} · {}", self.pane_name(pane), win_label(ws, win), ws_label(ws)),
                vec![pane_section(), window_section(), workspace_section()],
            ),
            Target::Window { ws, win } => (
                format!("{} · {}", win_label(ws, win), ws_label(ws)),
                vec![window_section(), workspace_section()],
            ),
            Target::Workspace(ws) => (ws_label(ws), vec![workspace_section()]),
            Target::Machine(m) if m.is_empty() => {
                (self.hostname.clone(), vec![workspace_section(), machines_section()])
            }
            Target::Machine(m) => (m.clone(), vec![machine_section(m), machines_section()]),
        };
        self.mode = Mode::Menu(Menu::new(&title, sections), target);
    }

    /// A right-click: the menu for the tab, sidebar row or pane under it.
    pub(super) fn right_click(&mut self, at: ratatui::layout::Position) {
        let target = if let Some((_, TabHit::Window(win))) = self.hits.tabs.iter().find(|(r, _)| r.contains(at)) {
            self.active().map(|(ws, _)| Target::Window { ws, win: win.clone() })
        } else if let Some((_, crate::sidebar::Hit::Go(t))) = self
            .hits
            .sidebar
            .iter()
            .find(|(r, h)| r.contains(at) && matches!(h, crate::sidebar::Hit::Go(_)))
        {
            Some(t.clone())
        } else {
            let pane = self
                .rects()
                .into_iter()
                .find(|(_, r)| r.contains(at) || (at.y < r.y && at.y + 2 >= r.y && at.x >= r.x && at.x < r.right()))
                .map(|(id, _)| id);
            match (pane, self.active()) {
                (Some(pane), Some((ws, win))) => Some(Target::Pane { ws, win, pane }),
                _ => return,
            }
        };
        if target.is_some() {
            self.open_menu(target);
        }
    }
}
