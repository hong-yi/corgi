//! Workspaces › windows › panes, as stored in core's `corgi` document.
//! Pure: changes are plain methods, and reconciling with the terminals core
//! reports is a function of the two, so it all tests without a core.
//!
//! Each terminal corgi starts carries `corgi.workspace` and `corgi.window`
//! labels. They make the layout recoverable: a terminal that is live but
//! missing from the document (a lost write, another client's edit) is put
//! back where its labels say.

use crate::layout::{Dir, Node, PaneId, Preset};
use hc_proto::Terminal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DOC: &str = "corgi";
pub const SCHEMA: u32 = 1;
pub const L_WORKSPACE: &str = "corgi.workspace";
pub const L_WINDOW: &str = "corgi.window";
/// A pane's name (`rename-pane`), on its terminal so every viewer sees it.
pub const L_NAME: &str = "corgi.name";
/// M2's single-pane label; terminals carrying it are adopted into the first workspace.
pub const L_M2: &str = "corgi.pane";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Doc {
    pub schema: u32,
    /// Next id number, shared by workspaces (`s7`) and windows (`w7`).
    pub next: u64,
    pub workspaces: Vec<Workspace>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    /// Set by the person; otherwise the folder's name is shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub folder: String,
    /// Where new terminals start: `None` is this machine, else a core machine name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    pub windows: Vec<Window>,
    /// Active window id.
    pub active: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub root: Node,
    pub focus: PaneId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<PaneId>,
    #[serde(default)]
    pub zoomed: bool,
    /// The preset `next-layout` applied last.
    #[serde(default)]
    pub preset: Option<Preset>,
}

impl Window {
    pub fn new(id: String, pane: PaneId) -> Window {
        Window {
            id,
            name: None,
            root: Node::Pane(pane.clone()),
            focus: pane,
            last: None,
            zoomed: false,
            preset: None,
        }
    }

    pub fn set_focus(&mut self, pane: &str) {
        if self.focus != pane && self.root.contains(pane) {
            self.last = Some(std::mem::replace(&mut self.focus, pane.to_string()));
        }
    }
}

impl Workspace {
    pub fn window(&self, id: &str) -> Option<&Window> {
        self.windows.iter().find(|w| w.id == id)
    }

    pub fn window_mut(&mut self, id: &str) -> Option<&mut Window> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    pub fn active_window(&self) -> Option<&Window> {
        self.window(&self.active).or(self.windows.first())
    }

    pub fn select(&mut self, window: &str) {
        if self.active != window && self.window(window).is_some() {
            self.last = Some(std::mem::replace(&mut self.active, window.to_string()));
        }
    }

    /// Shown name: set, or the folder's last component.
    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| {
            std::path::Path::new(&self.folder)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.folder.clone())
        })
    }
}

/// What reconciling changed, for the caller to act on.
#[derive(Debug, Default, PartialEq)]
pub struct Reconciled {
    /// Panes taken out: their terminal is gone or finished.
    pub removed: Vec<PaneId>,
    /// Terminals put back from their labels.
    pub adopted: Vec<PaneId>,
    /// Terminals adopted without full labels: set these (`addr`, workspace, window).
    pub relabel: Vec<(PaneId, String, String)>,
}

impl Doc {
    pub fn new() -> Doc {
        Doc {
            schema: SCHEMA,
            next: 1,
            workspaces: Vec::new(),
        }
    }

    pub fn alloc(&mut self, prefix: char) -> String {
        let id = format!("{prefix}{}", self.next);
        self.next += 1;
        id
    }

    pub fn workspace(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    pub fn workspace_mut(&mut self, id: &str) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|w| w.id == id)
    }

    /// (workspace index, window index) holding a pane.
    pub fn locate(&self, pane: &str) -> Option<(usize, usize)> {
        self.workspaces
            .iter()
            .enumerate()
            .find_map(|(si, s)| s.windows.iter().position(|w| w.root.contains(pane)).map(|wi| (si, wi)))
    }

    pub fn panes(&self) -> Vec<PaneId> {
        self.workspaces
            .iter()
            .flat_map(|s| &s.windows)
            .flat_map(|w| w.root.panes().into_iter().cloned())
            .collect()
    }

    pub fn add_workspace(&mut self, id: String, folder: String, machine: Option<String>, window: Window) {
        let active = window.id.clone();
        self.workspaces.push(Workspace {
            id,
            name: None,
            folder,
            machine,
            windows: vec![window],
            active,
            last: None,
        });
    }

    /// Append a window (tmux `new-window` takes the next index) and make it active.
    pub fn add_window(&mut self, ws: &str, window: Window) -> bool {
        let Some(s) = self.workspace_mut(ws) else { return false };
        let id = window.id.clone();
        s.windows.push(window);
        s.select(&id);
        true
    }

    /// Split `target` with `new` and focus it. Falls back to the window's focused pane if `target` has gone.
    pub fn split(&mut self, ws: &str, win: &str, target: &str, new: PaneId, dir: Dir, pct: u8) -> bool {
        let Some(w) = self.workspace_mut(ws).and_then(|s| s.window_mut(win)) else {
            return false;
        };
        let target = if w.root.contains(target) {
            target.to_string()
        } else {
            w.focus.clone()
        };
        if !w.root.split(&target, new.clone(), dir, pct) {
            return false;
        }
        w.zoomed = false;
        w.set_focus(&new);
        true
    }

    /// A machine now goes by another name: its workspaces say so, and its panes' addresses
    /// (`old/t3`) become `new/t3`.
    pub fn rename_machine(&mut self, old: &str, new: &str) {
        let f = |p: &str| {
            p.strip_prefix(old)
                .and_then(|rest| rest.strip_prefix('/'))
                .map(|rest| format!("{new}/{rest}"))
        };
        for s in &mut self.workspaces {
            if s.machine.as_deref() == Some(old) {
                s.machine = Some(new.to_string());
            }
            for w in &mut s.windows {
                w.root.rename(&f);
                if let Some(n) = f(&w.focus) {
                    w.focus = n;
                }
                if let Some(n) = w.last.as_deref().and_then(f) {
                    w.last = Some(n);
                }
            }
        }
    }

    /// Take a pane out; empty windows and workspaces go with it. True if it was there.
    pub fn remove_pane(&mut self, pane: &str) -> bool {
        let Some((si, wi)) = self.locate(pane) else {
            return false;
        };
        let s = &mut self.workspaces[si];
        let w = &mut s.windows[wi];
        match w.root.clone().remove(pane) {
            Some(root) => {
                w.root = root;
                if w.focus == pane {
                    let next = w
                        .last
                        .clone()
                        .filter(|l| w.root.contains(l))
                        .unwrap_or_else(|| w.root.panes()[0].clone());
                    w.focus = next;
                    w.zoomed = false;
                }
                if w.last.as_deref() == Some(pane) {
                    w.last = None;
                }
            }
            None => {
                let id = s.windows.remove(wi).id;
                if s.windows.is_empty() {
                    self.workspaces.remove(si);
                } else if s.active == id {
                    let next = s
                        .last
                        .clone()
                        .filter(|l| s.window(l).is_some())
                        .unwrap_or_else(|| s.windows[wi.min(s.windows.len() - 1)].id.clone());
                    s.active = next;
                    s.last = None;
                }
            }
        }
        true
    }

    /// Make the document agree with the terminals core reports (`live`: address → record).
    pub fn reconcile(&mut self, live: &BTreeMap<String, Terminal>) -> Reconciled {
        let mut r = Reconciled::default();
        for pane in self.panes() {
            let gone = match live.get(&pane) {
                Some(t) => !t.is_live(),
                // A remote machine's terminals vanish while its line is down; keep them.
                None => !pane.contains('/'),
            };
            if gone && self.remove_pane(&pane) {
                r.removed.push(pane);
            }
        }
        let placed: std::collections::HashSet<PaneId> = self.panes().into_iter().collect();
        for (addr, t) in live {
            if !t.is_live() || placed.contains(addr) {
                continue;
            }
            let ws = t.labels.get(L_WORKSPACE).cloned();
            let win = t.labels.get(L_WINDOW).cloned();
            match (ws, win) {
                (Some(ws), Some(win)) => self.adopt(addr, &ws, &win, &t.cwd),
                (Some(ws), None) => {
                    let win = self.alloc('w');
                    self.adopt(addr, &ws, &win, &t.cwd);
                    r.relabel.push((addr.clone(), ws, win));
                }
                (None, _) if t.labels.contains_key(L_M2) => {
                    let ws = match self.workspaces.first() {
                        Some(s) => s.id.clone(),
                        None => self.alloc('s'),
                    };
                    let win = self.alloc('w');
                    self.adopt(addr, &ws, &win, &t.cwd);
                    r.relabel.push((addr.clone(), ws, win));
                }
                // Not corgi's: listed as unplaced in the chooser (M6), never pulled in uninvited.
                (None, _) => continue,
            }
            r.adopted.push(addr.clone());
        }
        r
    }

    fn adopt(&mut self, addr: &str, ws: &str, win: &str, cwd: &str) {
        if self.workspace(ws).is_none() {
            self.add_workspace(
                ws.into(),
                cwd.into(),
                addr.split_once('/').map(|(m, _)| m.into()),
                Window::new(win.into(), addr.into()),
            );
            return;
        }
        let s = self.workspace_mut(ws).unwrap();
        match s.window_mut(win) {
            Some(w) => {
                let target = w.root.panes().last().map(|p| p.to_string()).unwrap_or_default();
                w.root.split(&target, addr.into(), Dir::Vertical, 50);
            }
            None => s.windows.push(Window::new(win.into(), addr.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term(addr: &str, labels: &[(&str, &str)]) -> Terminal {
        Terminal {
            address: addr.into(),
            state: "live".into(),
            cwd: "/src/proj".into(),
            labels: labels.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            ..Default::default()
        }
    }

    fn live(ts: &[Terminal]) -> BTreeMap<String, Terminal> {
        ts.iter().map(|t| (t.address.clone(), t.clone())).collect()
    }

    fn doc() -> Doc {
        let mut d = Doc::new();
        let (s, w) = (d.alloc('s'), d.alloc('w'));
        d.add_workspace(s.clone(), "/src/proj".into(), None, Window::new(w.clone(), "t1".into()));
        d.split(&s, &w, "t1", "t2".into(), Dir::Horizontal, 50);
        let w2 = d.alloc('w');
        d.add_window(&s, Window::new(w2, "t3".into()));
        d
    }

    #[test]
    fn build_and_locate() {
        let d = doc();
        assert_eq!(d.panes(), ["t1", "t2", "t3"]);
        assert_eq!(d.locate("t2"), Some((0, 0)));
        assert_eq!(d.locate("t3"), Some((0, 1)));
        let s = &d.workspaces[0];
        assert_eq!((s.active.as_str(), s.last.as_deref()), ("w3", Some("w2")));
        assert_eq!(s.windows[0].focus, "t2", "split focuses the new pane");
        assert_eq!(s.label(), "proj");
    }

    #[test]
    fn removing_panes_closes_empty_windows_and_workspaces() {
        let mut d = doc();
        assert!(d.remove_pane("t2"));
        assert_eq!(d.workspaces[0].windows[0].focus, "t1", "focus falls back");
        assert!(d.remove_pane("t3"));
        assert_eq!(d.workspaces[0].windows.len(), 1);
        assert_eq!(d.workspaces[0].active, "w2", "active falls back to the last window");
        assert!(d.remove_pane("t1"));
        assert!(d.workspaces.is_empty());
        assert!(!d.remove_pane("t1"));
    }

    #[test]
    fn reconcile_drops_dead_panes_and_adopts_labelled_terminals() {
        let mut d = doc();
        let mut finished = term("t2", &[]);
        finished.state = "finished".into();
        let r = d.reconcile(&live(&[
            term("t1", &[]),
            finished,
            // t3 is missing entirely; vps/t5 is remote and its machine is down: kept.
            term("t4", &[(L_WORKSPACE, "s1"), (L_WINDOW, "w2")]),
            term("t6", &[(L_WORKSPACE, "s9"), (L_WINDOW, "w9")]),
            term("t7", &[]),
        ]));
        assert_eq!(r.removed, ["t2", "t3"]);
        assert_eq!(r.adopted, ["t4", "t6"]);
        assert!(r.relabel.is_empty());
        assert_eq!(
            d.workspaces[0].windows[0].root.panes(),
            ["t1", "t4"],
            "back into its window"
        );
        assert_eq!(d.workspaces[1].id, "s9", "its workspace is recreated");
        assert!(d.locate("t7").is_none(), "unlabelled terminals stay out");

        let mut remote = doc();
        remote.split("s1", "w2", "t1", "vps/t5".into(), Dir::Vertical, 50);
        let r = remote.reconcile(&live(&[term("t1", &[]), term("t2", &[]), term("t3", &[])]));
        assert!(r.removed.is_empty(), "{r:?}");
    }

    #[test]
    fn m2_terminal_is_adopted_and_relabelled() {
        let mut d = Doc::new();
        let r = d.reconcile(&live(&[term("t1", &[(L_M2, "main")])]));
        assert_eq!(r.adopted, ["t1"]);
        let (addr, ws, win) = &r.relabel[0];
        assert_eq!(
            (
                addr.as_str(),
                d.workspaces[0].id.as_str(),
                d.workspaces[0].windows[0].id.as_str()
            ),
            ("t1", ws.as_str(), win.as_str())
        );
        assert_eq!(d.workspaces[0].folder, "/src/proj");
    }

    #[test]
    fn document_round_trips() {
        let d = doc();
        let j = serde_json::to_value(&d).unwrap();
        assert_eq!(serde_json::from_value::<Doc>(j).unwrap(), d);
    }

    #[test]
    fn renaming_a_machine_renames_its_panes() {
        let mut d = Doc::new();
        d.add_workspace(
            "s1".into(),
            "/srv".into(),
            Some("vps".into()),
            Window::new("w1".into(), "vps/t1".into()),
        );
        d.split("s1", "w1", "vps/t1", "vps/t2".into(), Dir::Horizontal, 50);
        d.add_workspace("s2".into(), "/src".into(), None, Window::new("w2".into(), "t1".into()));
        d.rename_machine("vps", "box");
        let s = d.workspace("s1").unwrap();
        assert_eq!(s.machine.as_deref(), Some("box"));
        let w = s.window("w1").unwrap();
        assert_eq!(w.root.panes(), [&"box/t1".to_string(), &"box/t2".to_string()]);
        assert_eq!((w.focus.as_str(), w.last.as_deref()), ("box/t2", Some("box/t1")));
        assert_eq!(
            d.workspace("s2").unwrap().windows[0].focus,
            "t1",
            "this machine's untouched"
        );
    }
}
