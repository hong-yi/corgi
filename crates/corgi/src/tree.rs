//! Workspaces › windows › panes as a foldable tree of rows, shared by the
//! sidebar and the tree chooser (C-b w) so both group, fold and filter the
//! same way. Pure.

use crate::model::Doc;
use crate::status::{self, Agent};
use hc_proto::Terminal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// A machine's group of workspaces (`""`: this machine). Only there once other machines are.
    Machine(String),
    Workspace(String),
    Window {
        ws: String,
        win: String,
    },
    Pane {
        ws: String,
        win: String,
        pane: String,
    },
}

/// Which rows are open. Workspaces start open (show their windows); windows start closed.
/// Stored in this client's view file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Fold {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub closed_machines: BTreeSet<String>,
    #[serde(default)]
    pub closed_workspaces: BTreeSet<String>,
    #[serde(default)]
    pub open_windows: BTreeSet<String>,
}

impl Fold {
    pub fn is_open(&self, t: &Target) -> bool {
        match t {
            Target::Machine(m) => !self.closed_machines.contains(m),
            Target::Workspace(id) => !self.closed_workspaces.contains(id),
            Target::Window { win, .. } => self.open_windows.contains(win),
            Target::Pane { .. } => false,
        }
    }

    pub fn set(&mut self, t: &Target, open: bool) {
        match t {
            Target::Machine(m) if open => {
                self.closed_machines.remove(m);
            }
            Target::Machine(m) => {
                self.closed_machines.insert(m.clone());
            }
            Target::Workspace(id) if open => {
                self.closed_workspaces.remove(id);
            }
            Target::Workspace(id) => {
                self.closed_workspaces.insert(id.clone());
            }
            Target::Window { win, .. } if open => {
                self.open_windows.insert(win.clone());
            }
            Target::Window { win, .. } => {
                self.open_windows.remove(win);
            }
            Target::Pane { .. } => {}
        }
    }

    pub fn toggle(&mut self, t: &Target) {
        let open = self.is_open(t);
        self.set(t, !open);
    }
}

/// One line of the tree. The string fields feed the row templates' tokens.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub target: Option<Target>,
    pub depth: u8,
    /// Has children to show or hide.
    pub foldable: bool,
    pub open: bool,
    /// Where you are: the active workspace, its active window, that window's focused pane.
    pub current: bool,
    pub state: Option<Agent>,
    pub workspace: String,
    pub branch: Option<String>,
    pub machine: Option<String>,
    /// `index:name` on window and pane rows.
    pub window: String,
    pub pane: String,
    pub agent: String,
    pub title: Option<String>,
    pub labels: BTreeMap<String, String>,
    /// A workspace in a git worktree of another workspace's repository: listed under it.
    pub worktree: bool,
    /// On a machine row: its line (none for this machine).
    pub link: Option<crate::machines::Link>,
    /// On a machine whose line is down: what's shown is what was last seen.
    pub down: bool,
}

impl Row {
    /// What a filter matches against.
    fn haystack(&self) -> String {
        match &self.target {
            Some(Target::Machine(_)) => self.machine.clone().unwrap_or_default(),
            Some(Target::Workspace(_)) => format!("{} {}", self.workspace, self.branch.clone().unwrap_or_default()),
            Some(Target::Window { .. }) => self.window.clone(),
            _ => format!(
                "{} {} {}",
                self.pane,
                self.agent,
                self.title.clone().unwrap_or_default()
            ),
        }
        .to_lowercase()
    }
}

/// A window's panes in the order they were made (`t3` before `t12`, grouped by machine), not
/// where they are on screen: swapping or rearranging panes doesn't reorder the tree.
pub fn made_order(mut panes: Vec<&String>) -> Vec<&String> {
    panes.sort_by_cached_key(|a| {
        let (machine, local) = a.rsplit_once('/').map_or(("", a.as_str()), |(m, l)| (m, l));
        let n: Option<u64> = local.strip_prefix('t').and_then(|d| d.parse().ok());
        (machine.to_string(), n.is_none(), n, local.to_string())
    });
    panes
}

pub struct Inputs<'a> {
    pub doc: &'a Doc,
    pub terminals: &'a BTreeMap<String, Terminal>,
    /// A pane's display name (title, foreground program, …).
    pub name: &'a dyn Fn(&str) -> String,
    pub branches: &'a std::collections::HashMap<String, String>,
    /// (workspace, window, pane) you're at.
    pub current: (&'a str, &'a str, &'a str),
    /// Workspace folder → its git repository's own checkout (where they differ: a worktree).
    pub repos: &'a std::collections::HashMap<String, String>,
    /// A workspace's name as shown, and the folder it's in now (its first pane's), which the
    /// branch and the worktree grouping go by.
    pub label: &'a dyn Fn(&crate::model::Workspace) -> String,
    pub folder: &'a dyn Fn(&crate::model::Workspace) -> String,
    /// Core's other machines. With any (or a workspace on one), the tree's top level is
    /// machines: this one (called `host`) first, then the others in core's order.
    pub machines: &'a [crate::machines::Machine],
    pub host: &'a str,
    /// The number the first window is shown with (`KeyPreset::base_index`).
    pub base: usize,
}

/// The workspaces in the order the tree lists them: as they are, except that one in a worktree
/// of another workspace's repository comes right after that workspace (with `true`); each with
/// its folder key (`Inputs::folder`), worked out once. One pass to index, one to place.
fn workspace_order<'d>(i: &Inputs<'d>) -> Vec<(&'d crate::model::Workspace, bool, String)> {
    let ws = &i.doc.workspaces;
    let folders: Vec<String> = ws.iter().map(|s| (i.folder)(s)).collect();
    // The first workspace at each (machine, folder).
    let mut at: std::collections::HashMap<(Option<&str>, &str), usize> = std::collections::HashMap::new();
    for (k, (s, f)) in ws.iter().zip(&folders).enumerate() {
        at.entry((s.machine.as_deref(), f.as_str())).or_insert(k);
    }
    // Each workspace's parent: the one at its repository's own checkout, on the same machine.
    let parent: Vec<Option<usize>> = ws
        .iter()
        .zip(&folders)
        .map(|(s, f)| {
            let repo = i.repos.get(f).filter(|r| *r != f)?;
            at.get(&(s.machine.as_deref(), repo.as_str())).copied()
        })
        .collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); ws.len()];
    for (k, p) in parent.iter().enumerate() {
        if let Some(p) = p {
            children[*p].push(k);
        }
    }
    let mut out = Vec::with_capacity(ws.len());
    for (k, s) in ws.iter().enumerate() {
        if parent[k].is_some() {
            continue;
        }
        out.push((s, false, folders[k].clone()));
        out.extend(children[k].iter().map(|&c| (&ws[c], true, folders[c].clone())));
    }
    out
}

/// A pane's row (its depth and whether it's where you are aside).
fn pane_row(
    i: &Inputs,
    s: &crate::model::Workspace,
    w: &crate::model::Window,
    label: &str,
    window: &str,
    p: &String,
) -> Row {
    let t = i.terminals.get(p);
    Row {
        target: Some(Target::Pane {
            ws: s.id.clone(),
            win: w.id.clone(),
            pane: p.clone(),
        }),
        state: t.and_then(status::agent),
        workspace: label.to_string(),
        machine: s.machine.clone(),
        window: window.to_string(),
        pane: p.clone(),
        agent: t
            .filter(|t| status::agent(t).is_some())
            .map(status::agent_name)
            .unwrap_or_else(|| (i.name)(p)),
        title: t.and_then(|t| t.title.clone()),
        labels: t.map(|t| t.labels.clone()).unwrap_or_default(),
        ..Default::default()
    }
}

/// Every pane with an agent, folded away or not, in the tree's order (workspaces, their
/// worktrees after them, windows, panes as made).
pub fn agents(i: &Inputs) -> Vec<Row> {
    let mut out = Vec::new();
    for (s, ..) in workspace_order(i) {
        let mut label = None;
        for (wi, w) in s.windows.iter().enumerate() {
            for p in made_order(w.root.panes()) {
                if i.terminals.get(p).and_then(status::agent).is_none() {
                    continue;
                }
                let label = label.get_or_insert_with(|| (i.label)(s)).clone();
                let window = format!(
                    "{}:{}",
                    wi + i.base,
                    w.name.clone().unwrap_or_else(|| (i.name)(&w.focus))
                );
                out.push(pane_row(i, s, w, &label, &window, p));
            }
        }
    }
    out
}

/// The visible rows. With a filter, everything that matches is shown with its parents, all open.
pub fn rows(i: &Inputs, fold: &Fold, filter: &str) -> Vec<Row> {
    let filter = filter.trim().to_lowercase();
    let order = workspace_order(i);
    if i.machines.is_empty() && order.iter().all(|(s, ..)| s.machine.is_none()) {
        return order
            .into_iter()
            .flat_map(|(s, wt, f)| workspace_rows(i, s, wt, &f, fold, &filter, 0, false))
            .collect();
    }
    // This machine, the others core has, then any a workspace names that core doesn't (removed).
    let mut groups: Vec<Option<&str>> = vec![None];
    groups.extend(i.machines.iter().map(|m| Some(m.name.as_str())));
    for (s, ..) in &order {
        if let Some(m) = s.machine.as_deref()
            && !groups.contains(&Some(m))
        {
            groups.push(Some(m));
        }
    }
    let mut out = Vec::new();
    for g in groups {
        let link = g.map(|name| {
            i.machines
                .iter()
                .find(|m| m.name == name)
                .map_or(crate::machines::Link::Other("removed".into()), |m| m.link())
        });
        let down = link.as_ref().is_some_and(|l| !l.is_up());
        let kids: Vec<Row> = order
            .iter()
            .filter(|(s, ..)| s.machine.as_deref() == g)
            .flat_map(|(s, wt, f)| workspace_rows(i, s, *wt, f, fold, &filter, 1, down))
            .collect();
        let name = g.unwrap_or(i.host).to_string();
        if !filter.is_empty() && kids.is_empty() && !name.to_lowercase().contains(&filter) {
            continue;
        }
        let target = Target::Machine(g.unwrap_or_default().to_string());
        let state = kids
            .iter()
            .filter(|r| matches!(r.target, Some(Target::Workspace(_))))
            .filter_map(|r| r.state)
            .max();
        let open = !kids.is_empty() && (!filter.is_empty() || fold.is_open(&target));
        out.push(Row {
            foldable: !kids.is_empty(),
            open,
            target: Some(target),
            depth: 0,
            state,
            machine: Some(name),
            link,
            down,
            ..Default::default()
        });
        if open {
            out.extend(kids);
        }
    }
    out
}

/// One workspace's rows (itself, its windows, their panes, as folded or filtered), `shift`
/// levels deep; none if the filter matches nothing in it.
#[allow(clippy::too_many_arguments)]
fn workspace_rows(
    i: &Inputs,
    s: &crate::model::Workspace,
    worktree: bool,
    folder: &str,
    fold: &Fold,
    filter: &str,
    shift: u8,
    down: bool,
) -> Vec<Row> {
    let mut out = Vec::new();
    let label = (i.label)(s);
    let ws_current = s.id == i.current.0;
    let mut ws_terms = Vec::new();
    let mut windows = Vec::new();
    for (wi, w) in s.windows.iter().enumerate() {
        let window = format!(
            "{}:{}",
            wi + i.base,
            w.name.clone().unwrap_or_else(|| (i.name)(&w.focus))
        );
        let win_current = ws_current && w.id == i.current.1;
        let mut panes = Vec::new();
        let mut win_terms = Vec::new();
        for p in made_order(w.root.panes()) {
            let t = i.terminals.get(p);
            if let Some(t) = t {
                win_terms.push(t);
                ws_terms.push(t);
            }
            panes.push(Row {
                depth: 2 + shift,
                current: win_current && p.as_str() == i.current.2,
                ..pane_row(i, s, w, &label, &window, p)
            });
        }
        let target = Target::Window {
            ws: s.id.clone(),
            win: w.id.clone(),
        };
        windows.push((
            Row {
                foldable: panes.len() > 1,
                open: panes.len() > 1 && fold.is_open(&target),
                target: Some(target),
                depth: 1 + shift,
                current: win_current,
                state: status::rollup(win_terms),
                workspace: label.clone(),
                machine: s.machine.clone(),
                window,
                ..Default::default()
            },
            panes,
        ));
    }
    let target = Target::Workspace(s.id.clone());
    let ws_row = Row {
        foldable: !windows.is_empty(),
        open: fold.is_open(&target),
        target: Some(target),
        depth: shift,
        current: ws_current,
        state: status::rollup(ws_terms),
        workspace: label,
        branch: i.branches.get(folder).cloned(),
        machine: s.machine.clone(),
        worktree,
        ..Default::default()
    };

    if filter.is_empty() {
        let open = ws_row.open;
        out.push(ws_row);
        if open {
            for (w, panes) in windows {
                let show = w.open;
                out.push(w);
                if show {
                    out.extend(panes);
                }
            }
        }
    } else {
        // Filtering: keep matches and the path to them, unfolded.
        let ws_hit = ws_row.haystack().contains(filter);
        let mut kept = Vec::new();
        for (mut w, panes) in windows {
            let hits: Vec<Row> = panes.into_iter().filter(|p| p.haystack().contains(filter)).collect();
            if ws_hit || w.haystack().contains(filter) || !hits.is_empty() {
                w.open = !hits.is_empty();
                kept.push(w);
                kept.extend(hits);
            }
        }
        if ws_hit || !kept.is_empty() {
            let mut ws_row = ws_row;
            ws_row.open = true;
            out.push(ws_row);
            out.extend(kept);
        }
    }
    for r in &mut out {
        r.down = down;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Dir;
    use crate::model::Window;
    use hc_proto::Status;
    use std::collections::HashMap;

    fn doc() -> Doc {
        let mut d = Doc::new();
        d.add_workspace(
            "s1".into(),
            "/src/corgi".into(),
            None,
            Window::new("w1".into(), "t1".into()),
        );
        d.split("s1", "w1", "t1", "t2".into(), Dir::Horizontal, 50);
        d.add_window("s1", Window::new("w2".into(), "t3".into()));
        d.add_workspace(
            "s2".into(),
            "/src/api".into(),
            None,
            Window::new("w3".into(), "t4".into()),
        );
        d
    }

    fn terms() -> BTreeMap<String, Terminal> {
        let mut m = BTreeMap::new();
        for a in ["t1", "t2", "t3", "t4"] {
            m.insert(
                a.to_string(),
                Terminal {
                    address: a.into(),
                    state: "live".into(),
                    ..Default::default()
                },
            );
        }
        let t2 = m.get_mut("t2").unwrap();
        t2.status = Some(Status {
            state: "blocked".into(),
            app: Some("claude-code".into()),
            ..Default::default()
        });
        t2.reported = true;
        m
    }

    fn texts(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| {
                let fold = if !r.foldable {
                    " "
                } else if r.open {
                    "▾"
                } else {
                    "▸"
                };
                let what = match &r.target {
                    Some(Target::Machine(_)) => r.machine.clone().unwrap_or_default(),
                    Some(Target::Workspace(_)) => r.workspace.clone(),
                    Some(Target::Window { .. }) => r.window.clone(),
                    _ => r.agent.clone(),
                };
                format!("{}{fold}{what}", "  ".repeat(r.depth as usize))
            })
            .collect()
    }

    fn run(fold: &Fold, filter: &str) -> Vec<Row> {
        let (d, t, b) = (doc(), terms(), HashMap::new());
        let name = |p: &str| format!("sh-{p}");
        let i = Inputs {
            doc: &d,
            terminals: &t,
            name: &name,
            branches: &b,
            current: ("s1", "w1", "t2"),
            repos: &HashMap::new(),
            label: &|s| s.label(),
            folder: &|s| s.folder.clone(),
            machines: &[],
            host: "mac",
            base: 0,
        };
        rows(&i, fold, filter)
    }

    #[test]
    fn workspaces_open_windows_closed_by_default() {
        let r = run(&Fold::default(), "");
        assert_eq!(texts(&r), ["▾corgi", "  ▸0:sh-t2", "   1:sh-t3", "▾api", "   0:sh-t4"]);
        assert_eq!(r[0].state, Some(Agent::Blocked), "rolled up from t2");
        assert_eq!(r[1].state, Some(Agent::Blocked));
        assert!(r[0].current && r[1].current && !r[2].current);
        assert!(!r[2].foldable, "one pane: nothing to unfold");
    }

    #[test]
    fn worktree_workspaces_list_under_their_repository() {
        let mut d = doc();
        // s3: a worktree of s1's repository, added last; s2 stays where it is.
        d.add_workspace(
            "s3".into(),
            "/trees/corgi/feat".into(),
            None,
            Window::new("w9".into(), "t9".into()),
        );
        let (t, b) = (terms(), HashMap::new());
        let repos: HashMap<String, String> = [
            ("/trees/corgi/feat".to_string(), "/src/corgi".to_string()),
            ("/src/corgi".to_string(), "/src/corgi".to_string()),
        ]
        .into();
        let name = |p: &str| format!("sh-{p}");
        let i = Inputs {
            doc: &d,
            terminals: &t,
            name: &name,
            branches: &b,
            current: ("s1", "w1", "t2"),
            repos: &repos,
            label: &|s| s.label(),
            folder: &|s| s.folder.clone(),
            machines: &[],
            host: "mac",
            base: 0,
        };
        let mut f = Fold::default();
        f.toggle(&Target::Workspace("s1".into()));
        let r = rows(&i, &f, "");
        let ws: Vec<(&str, bool)> = r
            .iter()
            .filter(|r| matches!(r.target, Some(Target::Workspace(_))))
            .map(|r| (r.workspace.as_str(), r.worktree))
            .collect();
        assert_eq!(ws, [("corgi", false), ("feat", true), ("api", false)]);
    }

    #[test]
    fn swapped_panes_keep_their_place_in_the_tree() {
        let mut f = Fold::default();
        f.toggle(&Target::Window {
            ws: "s1".into(),
            win: "w1".into(),
        });
        let before = texts(&run(&f, ""));
        let (mut d, t, b) = (doc(), terms(), HashMap::new());
        let w = d.workspace_mut("s1").unwrap().window_mut("w1").unwrap();
        w.root.swap("t1", "t2");
        assert_eq!(
            w.root.panes(),
            [&"t2".to_string(), &"t1".to_string()],
            "swapped on screen"
        );
        let name = |p: &str| format!("sh-{p}");
        let i = Inputs {
            doc: &d,
            terminals: &t,
            name: &name,
            branches: &b,
            current: ("s1", "w1", "t2"),
            repos: &HashMap::new(),
            label: &|s| s.label(),
            folder: &|s| s.folder.clone(),
            machines: &[],
            host: "mac",
            base: 0,
        };
        assert_eq!(texts(&rows(&i, &f, "")), before, "but not in the tree");
        let names = ["t12".to_string(), "t3".to_string(), "m/t1".to_string(), "x".to_string()];
        let order = made_order(names.iter().collect());
        assert_eq!(
            order,
            [&names[1], &names[0], &names[3], &names[2]],
            "by number, then other names, by machine"
        );
    }

    #[test]
    fn folding_opens_windows_and_closes_workspaces() {
        let mut f = Fold::default();
        f.toggle(&Target::Window {
            ws: "s1".into(),
            win: "w1".into(),
        });
        f.toggle(&Target::Workspace("s2".into()));
        let r = run(&f, "");
        assert_eq!(
            texts(&r),
            [
                "▾corgi",
                "  ▾0:sh-t2",
                "     sh-t1",
                "     claude-code",
                "   1:sh-t3",
                "▸api"
            ]
        );
        assert!(r[3].current, "the focused pane");
        assert_eq!(
            r[3].target,
            Some(Target::Pane {
                ws: "s1".into(),
                win: "w1".into(),
                pane: "t2".into()
            })
        );
    }

    #[test]
    fn filter_shows_matches_with_their_parents() {
        let mut f = Fold::default();
        f.toggle(&Target::Workspace("s1".into()));
        assert_eq!(
            texts(&run(&f, "claude")),
            ["▾corgi", "  ▾0:sh-t2", "     claude-code"],
            "found inside closed rows"
        );
        assert_eq!(
            texts(&run(&f, "API")),
            ["▾api", "   0:sh-t4"],
            "case-insensitive; a workspace match keeps its windows"
        );
        assert!(run(&f, "nothing-like-this").is_empty());
    }

    #[test]
    fn machines_are_the_top_level_once_there_are_any() {
        let mut d = doc();
        d.add_workspace(
            "s5".into(),
            "/srv/app".into(),
            Some("vps".into()),
            Window::new("w5".into(), "vps/t1".into()),
        );
        let mut t = terms();
        t.insert(
            "vps/t1".into(),
            Terminal {
                address: "vps/t1".into(),
                state: "live".into(),
                ..Default::default()
            },
        );
        let machines = crate::machines::parse(&serde_json::json!({"machines": [
            {"name": "vps", "target": "me@vps", "state": "unreachable", "why": "timed out"},
            {"name": "spare", "target": "spare", "state": "connected"},
        ]}));
        let b = HashMap::new();
        let name = |p: &str| format!("sh-{p}");
        let i = Inputs {
            doc: &d,
            terminals: &t,
            name: &name,
            branches: &b,
            current: ("s1", "w1", "t2"),
            repos: &HashMap::new(),
            label: &|s| s.label(),
            folder: &|s| s.folder.clone(),
            machines: &machines,
            host: "mac",
            base: 0,
        };
        let mut f = Fold::default();
        f.toggle(&Target::Workspace("s1".into()));
        let r = rows(&i, &f, "");
        assert_eq!(
            texts(&r),
            [
                "▾mac",
                "  ▸corgi",
                "  ▾api",
                "     0:sh-t4",
                "▾vps",
                "  ▾app",
                "     0:sh-vps/t1",
                " spare"
            ]
        );
        assert_eq!(r[0].target, Some(Target::Machine(String::new())));
        assert_eq!(r[0].state, Some(Agent::Blocked), "its workspaces' states roll up");
        assert!(r[0].link.is_none() && !r[0].down);
        assert_eq!(r[4].link, Some(crate::machines::Link::Unreachable));
        assert!(r[4].down && r[5].down && r[6].down, "a down machine's rows say so");
        assert!(!r[7].foldable, "no workspaces on it");
        f.toggle(&Target::Machine("vps".into()));
        assert_eq!(texts(&rows(&i, &f, ""))[4..], ["▸vps", " spare"]);
        assert_eq!(
            texts(&rows(&i, &f, "app")),
            ["▾vps", "  ▾app", "     0:sh-vps/t1"],
            "filtering opens it"
        );
    }

    #[test]
    fn grouping_looks_each_folder_up_once() {
        // 400 workspaces: half roots, half worktrees of a root far down the list.
        let mut d = Doc::new();
        let mut repos = HashMap::new();
        for k in 0..400 {
            let folder = if k % 2 == 0 {
                format!("/src/r{k}")
            } else {
                format!("/trees/t{k}")
            };
            if k % 2 == 1 {
                repos.insert(folder.clone(), format!("/src/r{}", 398 - (k - 1) % 398));
            }
            d.add_workspace(
                format!("s{k}"),
                folder,
                None,
                Window::new(format!("w{k}"), format!("t{k}")),
            );
        }
        let (t, b) = (BTreeMap::new(), HashMap::new());
        let calls = std::cell::Cell::new(0usize);
        let folder = |s: &crate::model::Workspace| {
            calls.set(calls.get() + 1);
            s.folder.clone()
        };
        let name = |p: &str| p.to_string();
        let i = Inputs {
            doc: &d,
            terminals: &t,
            name: &name,
            branches: &b,
            current: ("s0", "w0", "t0"),
            repos: &repos,
            label: &|s| s.label(),
            folder: &folder,
            machines: &[],
            host: "mac",
            base: 0,
        };
        let started = std::time::Instant::now();
        let r = rows(&i, &Fold::default(), "");
        assert_eq!(calls.get(), 400, "once each");
        assert!(
            started.elapsed() < std::time::Duration::from_millis(200),
            "{:?}",
            started.elapsed()
        );
        let ws: Vec<&Row> = r
            .iter()
            .filter(|r| matches!(r.target, Some(Target::Workspace(_))))
            .collect();
        assert_eq!(ws.len(), 400, "every workspace once");
        assert_eq!(ws.iter().filter(|r| r.worktree).count(), 200);
        // A worktree comes right after its repository's workspace.
        let at = |name: &str| ws.iter().position(|r| r.workspace == name).unwrap();
        assert_eq!(at("t1"), at("r398") + 1);
    }
}
