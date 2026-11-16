//! The dialogs: New Workspace and New Window (their fields, the machine and template lists
//! they fetch from core, validation, what submitting starts), Rename (workspace, window,
//! pane) and Close (asked per `ui.confirm_close`, warning about agents that would end).
//! The form itself (fields, editing, drawing) is `crate::form`.

use super::*;
use crate::commands::DialogKind;
use crate::form::{Field, Form, Outcome};
use crate::layout::Preset;
use crate::model::L_NAME;

pub(super) struct Dialog {
    pub does: Does,
    pub form: Form,
    /// Waiting for something it started (a git worktree): Enter does nothing more.
    pub busy: bool,
}

/// A worktree made for a new workspace (its path), or why not; and the rest of what to start.
#[derive(Debug)]
pub struct Made {
    pub result: Result<String, String>,
    pub name: Option<String>,
    pub machine: Option<String>,
    pub run: Option<Run>,
}

/// What a dialog does when it's submitted.
pub(super) enum Does {
    Create(DialogKind),
    RenameWorkspace {
        ws: String,
    },
    RenameWindow {
        ws: String,
        win: String,
    },
    RenamePane {
        pane: String,
    },
    /// Close (kill-pane, kill-window, kill-workspace), no further questions.
    Close(Action),
    /// Just to read (usage details): closing is all it does.
    Info,
    /// Remove a git worktree (and close its workspace, if open).
    RemoveWorktree {
        folder: String,
        path: String,
        force: bool,
        machine: Option<String>,
        workspace: Option<String>,
    },
    RenameMachine {
        name: String,
    },
    /// Forget a machine (its workspaces close here).
    RemoveMachine {
        name: String,
    },
}

impl Does {
    /// Which mode badge it shows.
    pub(super) fn badge(&self) -> Badge {
        match self {
            Does::Create(_) => Badge::New,
            Does::RenameWorkspace { .. }
            | Does::RenameWindow { .. }
            | Does::RenamePane { .. }
            | Does::RenameMachine { .. } => Badge::Rename,
            Does::Close(_) | Does::RemoveWorktree { .. } | Does::RemoveMachine { .. } => Badge::Confirm,
            Does::Info => Badge::Info,
        }
    }
}

pub(super) enum Badge {
    New,
    Rename,
    Confirm,
    Info,
}

/// Machines (name, state) and templates (name, description), for the dialogs' selects.
#[derive(Debug, Default)]
pub struct Lists {
    pub machines: Vec<(String, String)>,
    pub templates: Vec<(String, String)>,
}

/// Agents offered as a first program when they're on this machine's PATH.
const AGENTS: &[&str] = &[
    "claude",
    "codex",
    "gemini",
    "opencode",
    "aider",
    "amp",
    "goose",
    "cursor-agent",
];

/// Initial layouts for a new window: (value, label, panes, preset).
const LAYOUTS: &[(&str, &str, u8, Preset)] = &[
    ("1", "single pane", 1, Preset::EvenHorizontal),
    ("2h", "2 side by side", 2, Preset::EvenHorizontal),
    ("2v", "2 stacked", 2, Preset::EvenVertical),
    ("3", "3: main + 2", 3, Preset::MainVertical),
    ("4", "4 tiled", 4, Preset::Tiled),
];

fn on_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
}

/// The first-program select: the shell, templates, then agents found here.
fn programs(templates: &[(String, String)]) -> Vec<(String, String)> {
    let mut out = vec![(String::new(), "shell".to_string())];
    for (name, desc) in templates {
        let label = if desc.is_empty() {
            format!("template: {name}")
        } else {
            format!("template: {name} ({desc})")
        };
        out.push((format!("t:{name}"), label));
    }
    out.extend(
        AGENTS
            .iter()
            .filter(|a| on_path(a))
            .map(|a| (format!("c:{a}"), a.to_string())),
    );
    out
}

fn machines(list: &[(String, String)]) -> Vec<(String, String)> {
    let mut out = vec![(String::new(), "this machine".to_string())];
    out.extend(list.iter().map(|(name, state)| {
        let label = if state == "connected" {
            name.clone()
        } else {
            format!("{name} ({state})")
        };
        (name.clone(), label)
    }));
    out
}

/// What the program select's value starts: nothing (the shell), a template, or a command line.
fn run_of(value: &str) -> Option<Run> {
    if let Some(t) = value.strip_prefix("t:") {
        Some(Run::Template(t.to_string()))
    } else {
        value.strip_prefix("c:").map(|c| Run::Line(c.to_string()))
    }
}

/// A path with home shown as `~`.
fn short_home(p: &str) -> String {
    let h = home();
    match p.strip_prefix(&h) {
        Some(rest) if !h.is_empty() && (rest.is_empty() || rest.starts_with('/')) => format!("~{rest}"),
        _ => p.to_string(),
    }
}

pub(super) fn home() -> String {
    crate::paths::home().display().to_string()
}

impl App {
    pub(super) fn open_dialog(&mut self, kind: DialogKind) {
        if kind == DialogKind::NewWorktree {
            // New Workspace, switched to a worktree, from where you are (the current workspace's
            // folder, if it has one: the repository you're working in).
            let folder = self
                .active()
                .and_then(|(ws, _)| self.doc.workspace(&ws).map(|s| self.live_folder(s)));
            self.open_dialog(DialogKind::NewWorkspace);
            if let Mode::Dialog(d) = &mut self.mode {
                if let (Some(folder), Some(f)) = (folder.filter(|f| !f.is_empty()), d.form.get_mut("folder")) {
                    f.cursor = folder.chars().count();
                    f.text = folder;
                }
                if let Some(f) = d.form.get_mut("worktree") {
                    f.kind = crate::form::Kind::Toggle(true);
                }
                d.form.show("branch", true);
                d.form.show("path", true);
                if let Some(i) = d.form.fields.iter().position(|f| f.key == "branch") {
                    d.form.focus = i;
                }
            }
            return;
        }
        let here = self
            .focused_cwd()
            .or_else(|| std::env::current_dir().ok().map(|p| p.display().to_string()))
            .unwrap_or_else(home);
        // Show `~` for home: shorter, and what you'd type.
        let h = home();
        let shown = match here.strip_prefix(&h) {
            Some(rest) if !h.is_empty() && (rest.is_empty() || rest.starts_with('/')) => format!("~{rest}"),
            _ => here,
        };
        let form = match kind {
            DialogKind::NewWorkspace => Form::new(
                "New Workspace",
                "create",
                vec![
                    Field::path("folder", "Folder", &shown, true),
                    Field::text("name", "Name", "", "the folder's name"),
                    Field::select("machine", "Machine", machines(&self.machine_states()), 0),
                    Field::select("program", "Program", programs(&[]), 0),
                    // Last: optional; Branch and Path appear when it's on.
                    Field::toggle("worktree", "Worktree", false),
                    Field::text("branch", "Branch", "", "a new one: worktree/…"),
                    Field::path("path", "Path", "", true),
                ],
            ),
            DialogKind::NewWorktree => return,
            DialogKind::NewMachine => App::add_machine_form(),
            DialogKind::NewWindow => {
                let current = self.active().map(|(ws, _)| ws);
                let spaces: Vec<(String, String)> = self
                    .doc
                    .workspaces
                    .iter()
                    .map(|s| (s.id.clone(), self.ws_label(s)))
                    .collect();
                let at = spaces
                    .iter()
                    .position(|(id, _)| Some(id) == current.as_ref())
                    .unwrap_or(0);
                let mut fields = Vec::new();
                if spaces.len() > 1 {
                    fields.push(Field::select("workspace", "Workspace", spaces, at));
                }
                fields.extend([
                    Field::text("name", "Name", "", "what runs in it"),
                    Field::path("cwd", "Folder", &shown, true),
                    Field::select("program", "Program", programs(&[]), 0),
                    Field::select(
                        "layout",
                        "Layout",
                        LAYOUTS
                            .iter()
                            .map(|(v, l, ..)| (v.to_string(), l.to_string()))
                            .collect(),
                        0,
                    ),
                ]);
                Form::new("New Window", "create", fields)
            }
        };
        let mut form = form;
        // Worktree fields: only once it's switched on.
        form.show("branch", false);
        form.show("path", false);
        if let Some(f) = form.get_mut("path") {
            f.placeholder = format!("{}/<repository>/<branch>", self.config.worktrees.directory);
        }
        self.mode = Mode::Dialog(Dialog {
            does: Does::Create(kind),
            form,
            busy: false,
        });
        self.fetch_lists();
    }

    /// The New Workspace dialog, with `name` in its Name field (from the chooser's filter).
    pub(super) fn new_workspace_dialog(&mut self, name: Option<String>) {
        self.open_dialog(DialogKind::NewWorkspace);
        if let (Mode::Dialog(d), Some(name)) = (&mut self.mode, name)
            && let Some(f) = d.form.get_mut("name")
        {
            f.cursor = name.chars().count();
            f.text = name;
        }
    }

    /// The machines as the dialogs' selects list them: (name, state).
    fn machine_states(&self) -> Vec<(String, String)> {
        self.machines
            .iter()
            .map(|m| {
                (
                    m.name.clone(),
                    m.state.as_ref().map_or("", |l| l.word()).replace(' ', "_"),
                )
            })
            .collect()
    }

    /// Machines and templates for the selects; they fill in when core answers.
    fn fetch_lists(&self) {
        let (c, out) = (self.client.clone(), self.out.clone());
        tokio::spawn(async move {
            let mut lists = Lists::default();
            if let Ok(v) = c.call("machine.list", json!({})).await {
                for m in v["machines"].as_array().into_iter().flatten() {
                    let name = m["name"].as_str().unwrap_or_default().to_string();
                    lists
                        .machines
                        .push((name, m["state"].as_str().unwrap_or_default().to_string()));
                }
            }
            if let Ok(v) = c.call("templates.list", json!({})).await {
                for t in v["templates"].as_array().into_iter().flatten() {
                    if t["error"].is_string() {
                        continue;
                    }
                    let name = t["name"].as_str().unwrap_or_default().to_string();
                    lists
                        .templates
                        .push((name, t["description"].as_str().unwrap_or_default().to_string()));
                }
            }
            let _ = out.send(Msg::Lists(lists));
        });
    }

    /// Core's lists arrived: refill the selects, keeping what's chosen where it still exists.
    pub(super) fn lists_arrived(&mut self, lists: Lists) {
        let Mode::Dialog(d) = &mut self.mode else { return };
        let refill = |f: &mut Field, options: Vec<(String, String)>| {
            if let crate::form::Kind::Select { options: old, at } = &mut f.kind {
                let chosen = old.get(*at).map(|o| o.0.clone()).unwrap_or_default();
                *at = options.iter().position(|o| o.0 == chosen).unwrap_or(0);
                *old = options;
            }
        };
        if let Some(f) = d.form.get_mut("machine") {
            refill(f, machines(&lists.machines));
        }
        if let Some(f) = d.form.get_mut("program") {
            refill(f, programs(&lists.templates));
        }
    }

    pub(super) fn dialog_key(&mut self, mut d: Dialog, k: KeyEvent) {
        // Add Machine: C-t runs `hc machine add` in a terminal instead, where ssh can ask things.
        if matches!(d.does, Does::Create(DialogKind::NewMachine))
            && !d.busy
            && k.code == KeyCode::Char('t')
            && k.modifiers.contains(KeyModifiers::CONTROL)
        {
            match App::add_machine_args(&d.form) {
                Ok((_, args)) => self.add_machine_in_terminal(args),
                Err(e) => {
                    d.form.error = Some(e);
                    self.mode = Mode::Dialog(d);
                }
            }
            return;
        }
        let machine_before = d.form.value("machine");
        // Folder completion only makes sense on this machine.
        let local = d.form.value("machine").is_empty();
        for key in ["folder", "cwd"] {
            if let Some(f) = d.form.get_mut(key) {
                f.kind = crate::form::Kind::Path { local };
            }
        }
        // While a worktree is being made, only Esc does anything (and the workspace still opens).
        if d.busy {
            if k.code != KeyCode::Esc {
                self.mode = Mode::Dialog(d);
            }
            return;
        }
        match d.form.key(&k, &home()) {
            Outcome::Cancel => {}
            Outcome::Stay => self.mode = Mode::Dialog(d),
            Outcome::Submit => match self.submit(&mut d) {
                Ok(true) => {}
                Ok(false) => self.mode = Mode::Dialog(d),
                Err(e) => {
                    d.form.error = Some(e);
                    self.mode = Mode::Dialog(d);
                }
            },
        }
        let here = self.focused_cwd().map(|c| short_home(&c)).unwrap_or_else(|| "~".into());
        if let Mode::Dialog(d) = &mut self.mode {
            // Add Machine: a target for ssh, a command otherwise.
            if d.form.get("how").is_some() {
                let via = d.form.value("how") == "via";
                d.form.show("target", !via);
                d.form.show("via", via);
                if let Some(f) = d.form.get_mut("name") {
                    f.placeholder = if via {
                        "needed: what to call it"
                    } else {
                        "the host's name"
                    }
                    .into();
                }
            }
            // Another machine: start in its home (this one's folders mean nothing there).
            let machine = d.form.value("machine");
            if d.form.get("folder").is_some() && machine != machine_before {
                let text = if machine.is_empty() { here } else { "~".to_string() };
                if let Some(f) = d.form.get_mut("folder") {
                    f.cursor = text.chars().count();
                    f.text = text;
                }
            }
        }
        // Branch and Path only matter for a worktree.
        if let Mode::Dialog(d) = &mut self.mode
            && d.form.get("worktree").is_some()
        {
            let on = d.form.value("worktree") == "true";
            d.form.show("branch", on);
            d.form.show("path", on);
            if let Some(f) = d.form.get_mut("path")
                && f.placeholder.is_empty()
            {
                f.placeholder = format!("{}/<repository>/<branch>", self.config.worktrees.directory);
            }
        }
    }

    /// A click while a dialog is up: on a field focuses it; anywhere else is ignored.
    pub(super) fn dialog_click(&mut self, x: u16, y: u16) {
        if let Mode::Dialog(d) = &mut self.mode {
            d.form.click(x, y);
        }
    }

    /// Ok(true): done, the dialog goes; Ok(false): it stays, waiting (`busy`).
    fn submit(&mut self, d: &mut Dialog) -> Result<bool, String> {
        let f = &d.form;
        let name = Some(f.value("name").trim().to_string()).filter(|n| !n.is_empty());
        let run = run_of(&f.value("program"));
        match &d.does {
            Does::RenameWorkspace { ws } => {
                if let Some(s) = self.doc.workspace_mut(ws) {
                    s.name = name;
                }
                self.dirty = true;
            }
            Does::RenameWindow { ws, win } => {
                if let Some(w) = self.doc.workspace_mut(ws).and_then(|s| s.window_mut(win)) {
                    w.name = name;
                }
                self.dirty = true;
            }
            Does::RenamePane { pane } => self.name_pane(pane, name),
            Does::Close(action) => self.close_now(action),
            Does::Info => {}
            Does::RemoveWorktree {
                folder,
                path,
                force,
                machine,
                workspace,
            } => self.remove_worktree(folder, path, *force, machine.clone(), workspace.clone()),
            Does::Create(DialogKind::NewWorkspace) => {
                let machine = Some(f.value("machine")).filter(|m| !m.is_empty());
                let folder = folder(&f.value("folder"), machine.is_none())?;
                if f.value("worktree") == "true" {
                    let branch = Some(f.value("branch").trim().to_string())
                        .filter(|b| !b.is_empty())
                        .unwrap_or_else(|| {
                            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
                            crate::worktree::new_branch(t.map_or(0, |d| d.as_nanos() as u64))
                        });
                    let path = Some(f.value("path").trim().to_string()).filter(|p| !p.is_empty());
                    let line =
                        crate::worktree::script(&folder, &branch, path.as_deref(), &self.config.worktrees.directory);
                    self.make_worktree(line, name, machine, run);
                    d.busy = true;
                    d.form.error = None;
                    d.form.message = vec![(format!("making a worktree on {branch}…"), false)];
                    return Ok(false);
                }
                let (ws, win) = (self.doc.alloc('s'), self.doc.alloc('w'));
                self.spawn(
                    Purpose::Workspace {
                        ws,
                        win,
                        folder: folder.clone(),
                        name,
                    },
                    Some(folder),
                    machine,
                    run,
                );
            }
            Does::Create(DialogKind::NewWorktree) => {}
            Does::Create(DialogKind::NewMachine) => {
                let (params, _) = App::add_machine_args(f)?;
                let what = f.value("name");
                let what = if what.trim().is_empty() {
                    f.value("target")
                } else {
                    what
                };
                self.add_machine(params);
                d.busy = true;
                d.form.error = None;
                d.form.message = vec![(format!("reaching {}…", what.trim()), false)];
                return Ok(false);
            }
            Does::RenameMachine { name: old } => {
                let Some(new) = name else {
                    return Err("a name is needed".into());
                };
                if new == *old {
                    return Ok(true);
                }
                if new.contains(['/', ' ']) {
                    return Err("no spaces or slashes in a machine's name".into());
                }
                self.rename_machine(old.clone(), new);
                d.busy = true;
                d.form.error = None;
                return Ok(false);
            }
            Does::RemoveMachine { name } => {
                self.remove_machine(name.clone());
                d.busy = true;
                d.form.error = None;
                return Ok(false);
            }
            Does::Create(DialogKind::NewWindow) => {
                let ws = Some(f.value("workspace"))
                    .filter(|w| !w.is_empty())
                    .or_else(|| self.active().map(|(ws, _)| ws))
                    .ok_or("no workspace to put it in")?;
                let machine = self.doc.workspace(&ws).and_then(|s| s.machine.clone());
                let cwd = folder(&f.value("cwd"), machine.is_none())?;
                let layout = f.value("layout");
                let &(_, _, panes, preset) = LAYOUTS.iter().find(|l| l.0 == layout).unwrap_or(&LAYOUTS[0]);
                let win = self.doc.alloc('w');
                if panes > 1 {
                    self.layouts.insert(win.clone(), (panes - 1, preset, cwd.clone()));
                }
                self.spawn(Purpose::Window { ws, win, name }, Some(cwd), machine, run);
            }
        }
        Ok(true)
    }

    /// Make a git worktree (through core, on the workspace's machine); `Msg::Worktree` says how it went.
    fn make_worktree(&self, line: String, name: Option<String>, machine: Option<String>, run: Option<Run>) {
        let (c, out) = (self.client.clone(), self.out.clone());
        tokio::spawn(async move {
            let params = json!({"line": line, "on": machine, "timeout": 120});
            let result = match c.call("machine.exec", params).await {
                Err(e) => Err(e.to_string()),
                Ok(v) => {
                    let text = |k: &str| {
                        let raw = crate::worktree::unbase64(v[k].as_str().unwrap_or_default()).unwrap_or_default();
                        String::from_utf8_lossy(&raw).trim().to_string()
                    };
                    if v["code"].as_i64() == Some(0) {
                        text("stdout")
                            .lines()
                            .last()
                            .map(str::to_string)
                            .ok_or_else(|| "git said nothing".into())
                    } else {
                        // git's last word on it is the reason.
                        let err = text("stderr");
                        let why = err
                            .lines()
                            .rev()
                            .find(|l| !l.trim().is_empty())
                            .unwrap_or("git worktree add failed");
                        Err(why.trim_start_matches("fatal: ").to_string())
                    }
                }
            };
            let _ = out.send(Msg::Worktree(Made {
                result,
                name,
                machine,
                run,
            }));
        });
    }

    /// The worktree is there (open the workspace in it), or isn't (say why, in the dialog if it's
    /// still up).
    pub(super) fn worktree_made(&mut self, m: Made) {
        let waiting = matches!(&self.mode, Mode::Dialog(d) if d.busy);
        match m.result {
            Ok(path) => {
                if waiting {
                    self.mode = Mode::Normal;
                }
                let (ws, win) = (self.doc.alloc('s'), self.doc.alloc('w'));
                self.spawn(
                    Purpose::Workspace {
                        ws,
                        win,
                        folder: path.clone(),
                        name: m.name,
                    },
                    Some(path),
                    m.machine,
                    m.run,
                );
            }
            Err(e) if waiting => {
                if let Mode::Dialog(d) = &mut self.mode {
                    d.busy = false;
                    d.form.message.clear();
                    d.form.error = Some(e);
                }
            }
            Err(e) => self.notify(format!("no worktree: {e}")),
        }
    }

    /// After a terminal is placed: a new window with a layout to fill gets its other panes
    /// (shells in the same folder), and once they're all there, the layout's arrangement.
    pub(super) fn after_place(&mut self, purpose: &Purpose, addr: &str) {
        match purpose {
            Purpose::Window { ws, win, .. } => {
                let Some((n, _, cwd)) = self.layouts.get(win).cloned() else {
                    return;
                };
                let machine = self.doc.workspace(ws).and_then(|s| s.machine.clone());
                for _ in 0..n {
                    let p = Purpose::Split {
                        ws: ws.clone(),
                        win: win.clone(),
                        target: addr.to_string(),
                        dir: Dir::Horizontal,
                        pct: 50,
                    };
                    self.spawn(p, Some(cwd.clone()), machine.clone(), None);
                }
            }
            Purpose::Split { ws, win, .. } => {
                let Some(pending) = self.layouts.get_mut(win) else {
                    return;
                };
                pending.0 -= 1;
                if pending.0 > 0 {
                    return;
                }
                let preset = pending.1;
                self.layouts.remove(win);
                if let Some(w) = self.doc.workspace_mut(ws).and_then(|s| s.window_mut(win)) {
                    w.root = w.root.preset(preset);
                    w.preset = Some(preset);
                    // The program's pane (the first) keeps the focus.
                    if let Some(first) = w.root.panes().first().map(|p| p.to_string()) {
                        w.set_focus(&first);
                    }
                }
                self.dirty = true;
                self.sync();
            }
            Purpose::Workspace { .. } => {}
        }
    }
}

impl App {
    /// Rename what `what` names, in a dialog with the current name to edit.
    pub(super) fn open_rename(&mut self, what: Does) {
        let (title, now, auto) = match &what {
            Does::RenameWorkspace { ws } => {
                let s = self.doc.workspace(ws);
                let auto = s.map(|s| {
                    std::path::Path::new(&s.folder)
                        .file_name()
                        .map_or(s.folder.clone(), |f| f.to_string_lossy().into_owned())
                });
                ("Rename Workspace", s.and_then(|s| s.name.clone()), auto)
            }
            Does::RenameWindow { ws, win } => {
                let w = self.doc.workspace(ws).and_then(|s| s.window(win));
                let auto = w.map(|w| self.pane_name(&w.focus));
                ("Rename Window", w.and_then(|w| w.name.clone()), auto)
            }
            Does::RenamePane { pane } => {
                let now = self.terminals.get(pane).and_then(|t| t.labels.get(L_NAME).cloned());
                ("Rename Pane", now, Some(self.auto_pane_name(pane)))
            }
            _ => return,
        };
        let hint = format!("empty: {}", auto.unwrap_or_else(|| "automatic".into()));
        let form = Form::new(
            title,
            "rename",
            vec![Field::text("name", "Name", &now.unwrap_or_default(), &hint)],
        );
        self.mode = Mode::Dialog(Dialog {
            does: what,
            form,
            busy: false,
        });
    }

    /// Name a pane (or, with `None`, go back to what it runs).
    pub(super) fn name_pane(&mut self, pane: &str, name: Option<String>) {
        let value = name.clone().unwrap_or_default();
        if let Some(t) = self.terminals.get_mut(pane) {
            match &name {
                Some(n) => t.labels.insert(L_NAME.into(), n.clone()),
                None => t.labels.remove(L_NAME),
            };
        }
        let (c, pane) = (self.client.clone(), pane.to_string());
        tokio::spawn(async move {
            if let Err(e) = c
                .call("terminal.label", json!({"terminal": pane, "labels": {L_NAME: value}}))
                .await
            {
                tracing::warn!("naming {pane}: {e}");
            }
        });
    }

    /// The panes `action` (kill-pane, kill-window, kill-workspace) would close, and what to call
    /// that in a question.
    pub(super) fn closing(&self, action: &Action) -> Option<(String, Vec<String>)> {
        let (ws, _) = self.active()?;
        let w = self.window()?;
        Some(match action {
            Action::KillPane => (format!("pane {}", self.pane_label(&w.focus)), vec![w.focus.clone()]),
            Action::KillWindow => {
                let i = self.window_index().unwrap_or(0);
                let name = w.name.clone().unwrap_or_else(|| self.pane_name(&w.focus));
                let panes: Vec<String> = w.root.panes().into_iter().cloned().collect();
                let what = match panes.len() {
                    1 => format!("window {i}:{name}"),
                    n => format!("window {i}:{name} and its {n} panes"),
                };
                (what, panes)
            }
            Action::KillWorkspace => {
                let s = self.doc.workspace(&ws)?;
                let panes: Vec<String> = s
                    .windows
                    .iter()
                    .flat_map(|w| w.root.panes().into_iter().cloned())
                    .collect();
                let windows = s.windows.len();
                let what = format!(
                    "workspace {}: {windows} window{}, {} pane{}",
                    self.ws_label(s),
                    if windows == 1 { "" } else { "s" },
                    panes.len(),
                    if panes.len() == 1 { "" } else { "s" }
                );
                (what, panes)
            }
            _ => return None,
        })
    }

    /// Before closing: ask, if `ui.confirm_close` says to. True if it asked (and so the closing
    /// waits for the answer).
    pub(super) fn ask_close(&mut self, action: &Action) -> bool {
        use crate::config::ConfirmClose;
        let Some((what, panes)) = self.closing(action) else {
            return false;
        };
        let agents: Vec<String> = panes
            .iter()
            .filter_map(|p| {
                let t = self.terminals.get(p)?;
                let state = crate::status::agent(t)?;
                let doing = match state {
                    crate::status::Agent::Working => "is working",
                    crate::status::Agent::Blocked => "is waiting for you",
                    crate::status::Agent::Done => "has finished (unseen)",
                    crate::status::Agent::Idle => "is still running",
                    crate::status::Agent::Unknown => "is running",
                };
                Some(format!("⚠ {} {doing} in {p}", crate::status::agent_name(t)))
            })
            .collect();
        let ask = match self.config.ui.confirm_close {
            ConfirmClose::Never => false,
            ConfirmClose::Always => true,
            ConfirmClose::Auto => *action == Action::KillWorkspace || !agents.is_empty(),
        };
        if !ask {
            return false;
        }
        let title = match action {
            Action::KillPane => "Close Pane",
            Action::KillWindow => "Close Window",
            _ => "Close Workspace",
        };
        let mut message = vec![(format!("Close {what}?"), false)];
        if !agents.is_empty() {
            message.push((String::new(), false));
            message.extend(agents.into_iter().map(|a| (a, true)));
        }
        self.mode = Mode::Dialog(Dialog {
            does: Does::Close(action.clone()),
            form: Form::confirm(title, "close", message),
            busy: false,
        });
        true
    }

    /// Close without asking.
    pub(super) fn close_now(&mut self, action: &Action) {
        if let Some((_, panes)) = self.closing(action) {
            self.kill(panes);
        }
    }

    /// A pane as a question names it: its name and address (`claude (t3)`).
    fn pane_label(&self, addr: &str) -> String {
        let name = self.pane_name(addr);
        if name == addr { name } else { format!("{name} ({addr})") }
    }
}

/// The folder as given (`~` expanded), checked when it's on this machine.
fn folder(text: &str, local: bool) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("a folder is needed".into());
    }
    let path = crate::form::expand_home(text.trim_end_matches('/'), &home());
    let path = if path.is_empty() { "/".to_string() } else { path };
    if local && !std::path::Path::new(&path).is_dir() {
        return Err(format!("{text} isn't a folder here"));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_and_machines() {
        let p = programs(&[("review".into(), "review a PR".into())]);
        assert_eq!(p[0], ("".into(), "shell".into()));
        assert_eq!(p[1], ("t:review".into(), "template: review (review a PR)".into()));
        assert_eq!(run_of(&p[1].0), Some(Run::Template("review".into())));
        assert_eq!(run_of("c:claude"), Some(Run::Line("claude".into())));
        assert_eq!(run_of(""), None);
        let m = machines(&[
            ("rohan".into(), "connected".into()),
            ("box".into(), "unreachable".into()),
        ]);
        assert_eq!(m[0].1, "this machine");
        assert_eq!(m[2], ("box".into(), "box (unreachable)".into()));
    }

    #[test]
    fn folders_are_checked_here_but_not_elsewhere() {
        let tmp = std::env::temp_dir().display().to_string();
        assert_eq!(folder(&tmp, true).unwrap(), tmp.trim_end_matches('/'));
        assert!(folder("/no/such/dir", true).unwrap_err().contains("isn't a folder"));
        assert_eq!(
            folder("/no/such/dir", false).unwrap(),
            "/no/such/dir",
            "remote: as given"
        );
        assert!(folder("  ", true).is_err());
        assert_eq!(folder("/", true).unwrap(), "/");
    }
}
