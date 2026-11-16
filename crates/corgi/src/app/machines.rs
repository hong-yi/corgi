//! Other machines in the app: polling core's `machine.list` (core sends no event when a line
//! drops), reattaching a machine's panes when it comes back, the Machines dialog (`C-b M`),
//! Add Machine, and connect / log in / update / rename / remove. Logging in and updating run
//! `hc machine …` in a popup, since ssh may ask for a password or code there.

use super::*;
use crate::commands::{DialogKind, MachineVerb};
use crate::form::{Field, Form};
use crate::machines::{Machine, Outcome, View};

/// How often the machines' lines are looked at.
const POLL: Duration = Duration::from_millis(2500);

/// A machine call finished: what it was, and how it went.
#[derive(Debug)]
pub enum Done {
    Added(Result<Machine, String>),
    Connected(String, Result<Machine, String>),
    Renamed(String, String, Result<(), String>),
    Removed(String, Result<(), String>),
}

impl App {
    /// While this connection lasts: its machines, every `POLL`.
    pub(super) fn poll_machines(&self) {
        let (c, out) = (self.client.clone(), self.out.clone());
        tokio::spawn(async move {
            while c.is_open() {
                if let Ok(v) = c.call("machine.list", json!({})).await
                    && out.send(Msg::Machines(crate::machines::parse(&v))).is_err()
                {
                    return;
                }
                tokio::select! {
                    _ = c.closed() => return,
                    _ = tokio::time::sleep(POLL) => {}
                }
            }
        });
    }

    /// Look now (after doing something to one), not at the next poll.
    fn refresh_machines(&self) {
        let (c, out) = (self.client.clone(), self.out.clone());
        tokio::spawn(async move {
            if let Ok(v) = c.call("machine.list", json!({})).await {
                let _ = out.send(Msg::Machines(crate::machines::parse(&v)));
            }
        });
    }

    pub(super) fn machines_arrived(&mut self, list: Vec<Machine>) {
        let before: HashMap<String, bool> = self.machines.iter().map(|m| (m.name.clone(), m.is_up())).collect();
        let mut back = false;
        for m in &list {
            let was = before.get(&m.name).copied();
            if m.is_up() && was != Some(true) {
                // Its line is (back) up: view its panes again.
                let prefix = format!("{}/", m.name);
                for p in self.panes.values_mut().filter(|p| p.addr.starts_with(&prefix)) {
                    if p.state != State::Live && p.state != State::Myself {
                        p.detach();
                        p.state = State::Offline;
                    }
                }
                back = true;
                if was == Some(false) {
                    self.machine_toast(crate::toast::Kind::Done, format!("{} is back", m.name), String::new());
                }
            } else if !m.is_up()
                && was == Some(true)
                && self.doc.workspaces.iter().any(|s| s.machine.as_ref() == Some(&m.name))
            {
                let why = m.why.clone().unwrap_or_default();
                let title = format!("{} {}", m.name, m.link().word());
                self.machine_toast(crate::toast::Kind::Blocked, title, why);
            }
        }
        self.machines = list;
        self.usage_target();
        if back {
            self.sync();
            self.tick();
        }
    }

    fn machine_toast(&mut self, kind: crate::toast::Kind, title: String, body: String) {
        // Toasts off: none of these either.
        if !self.config.notifications.toasts {
            return;
        }
        let t = crate::toast::Toast {
            pane: None,
            kind,
            title,
            place: String::new(),
            body,
            until: Instant::now(),
        };
        self.toasts.push(t, Instant::now(), self.config.notifications.duration);
    }

    /// The machine a terminal is on, when its line is down.
    pub(super) fn down_machine(&self, addr: &str) -> Option<&Machine> {
        let name = crate::machines::of(addr)?;
        self.machines.iter().find(|m| m.name == name).filter(|m| !m.is_up())
    }

    /// `C-b M`.
    pub(super) fn choose_machine(&mut self, at: Option<&str>) {
        let at = at
            .and_then(|n| self.machines.iter().position(|m| m.name == n))
            .unwrap_or(0);
        self.mode = Mode::Machines(Box::new(View { at }));
        self.refresh_machines();
    }

    /// How many workspaces each machine has, in the list's order.
    pub(super) fn machine_workspaces(&self) -> Vec<usize> {
        self.machines
            .iter()
            .map(|m| {
                self.doc
                    .workspaces
                    .iter()
                    .filter(|s| s.machine.as_ref() == Some(&m.name))
                    .count()
            })
            .collect()
    }

    pub(super) fn machines_key(&mut self, mut v: Box<View>, k: KeyEvent) {
        let name = |i: usize| self.machines.get(i).map(|m| m.name.clone());
        match v.key(&k, self.machines.len()) {
            Outcome::Stay => self.mode = Mode::Machines(v),
            Outcome::Close => {}
            Outcome::Add => self.open_dialog(DialogKind::NewMachine),
            Outcome::Connect(i) => {
                self.mode = Mode::Machines(v);
                self.machine(MachineVerb::Connect, name(i));
            }
            Outcome::Rename(i) => self.machine(MachineVerb::Rename, name(i)),
            Outcome::Remove(i) => self.machine(MachineVerb::Remove, name(i)),
            Outcome::Login(i) => self.machine(MachineVerb::Login, name(i)),
            Outcome::Update(i) => self.machine(MachineVerb::Update, name(i)),
            Outcome::NewWorkspace(i) => {
                if let Some(n) = name(i) {
                    self.new_workspace_on(&n);
                }
            }
        }
    }

    /// The New Workspace dialog, set to start on `machine`, in its home.
    pub(super) fn new_workspace_on(&mut self, machine: &str) {
        self.open_dialog(DialogKind::NewWorkspace);
        if let Mode::Dialog(d) = &mut self.mode {
            if let Some(f) = d.form.get_mut("machine")
                && let crate::form::Kind::Select { options, at } = &mut f.kind
                && let Some(i) = options.iter().position(|o| o.0 == machine)
            {
                *at = i;
            }
            if let Some(f) = d.form.get_mut("folder") {
                f.text = "~".into();
                f.cursor = 1;
                f.kind = crate::form::Kind::Path { local: false };
            }
        }
    }

    /// `machine <verb> [name]`: the machine named, else the current workspace's.
    pub(super) fn machine(&mut self, verb: MachineVerb, name: Option<String>) {
        let name = name.or_else(|| {
            self.active()
                .and_then(|(ws, _)| self.doc.workspace(&ws).and_then(|s| s.machine.clone()))
        });
        let Some(name) = name else {
            self.notify("which machine? this workspace is on this one".into());
            return;
        };
        let (c, out) = (self.client.clone(), self.out.clone());
        match verb {
            MachineVerb::NewWorkspace => self.new_workspace_on(&name),
            MachineVerb::Connect => {
                self.notify(format!("connecting to {name}…"));
                tokio::spawn(async move {
                    let r = c
                        .call_as::<Machine>("machine.connect", json!({"machine": name}))
                        .await
                        .map_err(|e| e.to_string());
                    let _ = out.send(Msg::Machine(Done::Connected(name, r)));
                });
            }
            MachineVerb::Login | MachineVerb::Update => {
                let (word, title) = match verb {
                    MachineVerb::Login => ("login", format!("log in to {name}")),
                    _ => ("update", format!("update hc on {name}")),
                };
                let line = format!("{} machine {word} {}", self.hc_program(), crate::worktree::quote(&name));
                self.open_popup(popup::Ask {
                    cmd: Some(line),
                    cwd: None,
                    machine: None,
                    width: crate::commands::Size::Percent(80),
                    height: crate::commands::Size::Percent(60),
                    title,
                    close: false,
                    tmp: None,
                });
            }
            MachineVerb::Rename => {
                let mut form = Form::new(
                    "Rename Machine",
                    "rename",
                    vec![Field::text("name", "Name", &name, "what to call it here")],
                );
                if let Some(f) = form.get_mut("name") {
                    f.cursor = name.chars().count();
                }
                form.message = vec![("Only this corgi's core calls it by this name.".into(), false)];
                self.mode = Mode::Dialog(dialogs::Dialog {
                    does: dialogs::Does::RenameMachine { name },
                    form,
                    busy: false,
                });
            }
            MachineVerb::Remove => {
                let n = self
                    .doc
                    .workspaces
                    .iter()
                    .filter(|s| s.machine.as_ref() == Some(&name))
                    .count();
                let mut message = vec![(format!("Forget the machine {name}?"), false)];
                match n {
                    0 => {}
                    1 => message.push(("its workspace closes here".into(), true)),
                    n => message.push((format!("its {n} workspaces close here"), true)),
                }
                message.push((
                    "Its programs keep running there; adding it again reaches them.".into(),
                    false,
                ));
                self.mode = Mode::Dialog(dialogs::Dialog {
                    does: dialogs::Does::RemoveMachine { name },
                    form: Form::confirm("Remove Machine", "remove", message),
                    busy: false,
                });
            }
        }
    }

    /// The `hc` corgi reaches core with, for running `hc machine …` in a popup.
    fn hc_program(&self) -> String {
        match &self.transport {
            hc_proto::Transport::Spawn { program, .. } => crate::worktree::quote(&program.display().to_string()),
            #[cfg(unix)]
            hc_proto::Transport::Socket(_) => "hc".into(),
        }
    }

    /// The Add Machine dialog.
    pub(super) fn add_machine_form() -> Form {
        let mut form = Form::new(
            "Add Machine",
            "add",
            vec![
                Field::select(
                    "how",
                    "Reach it",
                    vec![
                        ("ssh".into(), "over ssh".into()),
                        ("via".into(), "with a command".into()),
                    ],
                    0,
                ),
                Field::text(
                    "target",
                    "Target",
                    "",
                    "user@host, host:2222, a Host from ~/.ssh/config",
                ),
                Field::text("via", "Command", "", "fly ssh console -a app --pty=false -C"),
                Field::text("name", "Name", "", "the host's name"),
                Field::text("session", "Session", "", "its default"),
                Field::toggle("temporary", "Temporary", false),
            ],
        );
        form.show("via", false);
        form.message = vec![(
            "C-t runs it in a terminal instead (passwords, host keys, installing hc there)".into(),
            false,
        )];
        form
    }

    /// The `hc machine add …` line for what the Add Machine dialog says, and its params.
    pub(super) fn add_machine_args(f: &Form) -> Result<(Value, Vec<String>), String> {
        let get = |k: &str| Some(f.value(k).trim().to_string()).filter(|v| !v.is_empty());
        let via = f.value("how") == "via";
        let (target, cmd, name, session) = (get("target"), get("via"), get("name"), get("session"));
        let temporary = f.value("temporary") == "true";
        let mut args = Vec::new();
        if via {
            let cmd = cmd.clone().ok_or("the command that reaches it is needed")?;
            if name.is_none() {
                return Err("a machine reached with a command needs a name".into());
            }
            args.extend(["--via".to_string(), cmd]);
        } else if target.is_none() {
            return Err("where is it? an ssh target is needed".into());
        }
        if let Some(n) = &name {
            args.extend(["--name".to_string(), n.clone()]);
        }
        if let Some(s) = &session {
            args.extend(["--session".to_string(), s.clone()]);
        }
        if temporary {
            args.push("--temp".into());
        }
        let target = if via { None } else { target };
        if let Some(t) = &target {
            args.push(t.clone());
        }
        let params = json!({
            "target": target,
            "via": if via { cmd } else { None },
            "name": name,
            "session": session,
            "temporary": temporary,
        });
        Ok((params, args))
    }

    /// Submitting Add Machine: core dials it (this can take a while).
    pub(super) fn add_machine(&mut self, params: Value) {
        let (c, out) = (self.client.clone(), self.out.clone());
        tokio::spawn(async move {
            let r = c
                .call_as::<Machine>("machine.add", params)
                .await
                .map_err(|e| e.to_string());
            let _ = out.send(Msg::Machine(Done::Added(r)));
        });
    }

    /// C-t in Add Machine: `hc machine add …` in a popup, where ssh can ask what it needs to.
    pub(super) fn add_machine_in_terminal(&mut self, args: Vec<String>) {
        let line = std::iter::once(format!("{} machine add", self.hc_program()))
            .chain(args.iter().map(|a| crate::worktree::quote(a)))
            .collect::<Vec<_>>()
            .join(" ");
        self.mode = Mode::Normal;
        self.open_popup(popup::Ask {
            cmd: Some(line),
            cwd: None,
            machine: None,
            width: crate::commands::Size::Percent(80),
            height: crate::commands::Size::Percent(60),
            title: "add a machine".into(),
            close: false,
            tmp: None,
        });
    }

    pub(super) fn rename_machine(&mut self, old: String, new: String) {
        let (c, out) = (self.client.clone(), self.out.clone());
        tokio::spawn(async move {
            let r = c
                .call("machine.rename", json!({"machine": old, "new_name": new}))
                .await
                .map(|_| ())
                .map_err(|e| e.to_string());
            let _ = out.send(Msg::Machine(Done::Renamed(old, new, r)));
        });
    }

    pub(super) fn remove_machine(&mut self, name: String) {
        let (c, out) = (self.client.clone(), self.out.clone());
        tokio::spawn(async move {
            let r = c
                .call("machine.rm", json!({"machine": name}))
                .await
                .map(|_| ())
                .map_err(|e| e.to_string());
            let _ = out.send(Msg::Machine(Done::Removed(name, r)));
        });
    }

    /// A busy dialog's call failed: say why in it (if it's still up), else in a toast.
    fn dialog_failed(&mut self, e: String) {
        if let Mode::Dialog(d) = &mut self.mode
            && d.busy
        {
            d.busy = false;
            d.form.error = Some(e);
            return;
        }
        self.notify(e);
    }

    fn close_busy_dialog(&mut self) {
        if matches!(&self.mode, Mode::Dialog(d) if d.busy) {
            self.mode = Mode::Normal;
        }
    }

    pub(super) fn machine_done(&mut self, done: Done) {
        match done {
            Done::Added(Ok(m)) => {
                let waiting = matches!(&self.mode, Mode::Dialog(d) if d.busy);
                self.notify(format!("added {}", m.name));
                if waiting {
                    // Show it among the others, with its line's state.
                    let name = m.name.clone();
                    self.machines.retain(|x| x.name != name);
                    self.machines.push(m);
                    self.choose_machine(Some(&name));
                } else {
                    self.refresh_machines();
                }
            }
            Done::Added(Err(e)) => {
                self.dialog_failed(format!("{e} (C-t tries it in a terminal)"));
            }
            Done::Connected(name, Ok(m)) => {
                let say = if m.is_up() {
                    format!("{name} is connected")
                } else {
                    m.trouble(crate::machines::now_ms())
                };
                self.notify(say);
                self.refresh_machines();
            }
            Done::Connected(name, Err(e)) => {
                self.notify(format!("can't connect to {name}: {e}"));
                self.refresh_machines();
            }
            Done::Renamed(old, new, Ok(())) => {
                self.close_busy_dialog();
                self.doc.rename_machine(&old, &new);
                self.dirty = true;
                let prefix = format!("{old}/");
                let moved: Vec<String> = self
                    .terminals
                    .keys()
                    .filter(|a| a.starts_with(&prefix))
                    .cloned()
                    .collect();
                for a in moved {
                    if let Some(mut t) = self.terminals.remove(&a) {
                        let addr = format!("{new}/{}", &a[prefix.len()..]);
                        t.address = addr.clone();
                        self.terminals.insert(addr, t);
                    }
                }
                // Its panes reattach under their new addresses.
                self.panes.retain(|a, _| !a.starts_with(&prefix));
                if self.fold.closed_machines.remove(&old) {
                    self.fold.closed_machines.insert(new.clone());
                    self.save_view();
                }
                if let Some(m) = self.machines.iter_mut().find(|m| m.name == old) {
                    m.name = new.clone();
                }
                self.notify(format!("{old} is now {new}"));
                self.sync();
                self.refresh_machines();
            }
            Done::Renamed(_, _, Err(e)) => self.dialog_failed(e),
            Done::Removed(name, Ok(())) => {
                self.close_busy_dialog();
                let panes: Vec<String> = self
                    .doc
                    .workspaces
                    .iter()
                    .filter(|s| s.machine.as_ref() == Some(&name))
                    .flat_map(|s| s.windows.iter().flat_map(|w| w.root.panes().into_iter().cloned()))
                    .collect();
                for p in &panes {
                    self.doc.remove_pane(p);
                }
                self.doc.workspaces.retain(|s| s.machine.as_ref() != Some(&name));
                self.machines.retain(|m| m.name != name);
                self.dirty = true;
                self.notify(format!("forgot {name}"));
                self.sync();
                self.refresh_machines();
            }
            Done::Removed(_, Err(e)) => self.dialog_failed(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(how: &str, fields: &[(&str, &str)]) -> Form {
        let mut f = App::add_machine_form();
        if let Some(h) = f.get_mut("how")
            && let crate::form::Kind::Select { options, at } = &mut h.kind
        {
            *at = options.iter().position(|o| o.0 == how).unwrap();
        }
        for (k, v) in fields {
            match f.get_mut(k).map(|x| &mut x.kind) {
                Some(crate::form::Kind::Toggle(on)) => *on = *v == "true",
                _ => f.get_mut(k).unwrap().text = v.to_string(),
            }
        }
        f
    }

    #[test]
    fn add_machine_params_and_command_line() {
        let (p, args) = App::add_machine_args(&form("ssh", &[("target", "me@box:2222"), ("name", "box")])).unwrap();
        assert_eq!(p["target"], "me@box:2222");
        assert!(p["via"].is_null());
        assert_eq!(args, ["--name", "box", "me@box:2222"]);
        let (p, args) = App::add_machine_args(&form(
            "via",
            &[
                ("via", "fly ssh console -C"),
                ("name", "fly"),
                ("session", "s"),
                ("temporary", "true"),
                ("target", "ignored"),
            ],
        ))
        .unwrap();
        assert_eq!(p["via"], "fly ssh console -C");
        assert!(p["target"].is_null(), "{p}");
        assert_eq!(p["temporary"], true);
        assert_eq!(
            args,
            [
                "--via",
                "fly ssh console -C",
                "--name",
                "fly",
                "--session",
                "s",
                "--temp"
            ]
        );
        assert!(App::add_machine_args(&form("ssh", &[])).is_err());
        assert!(
            App::add_machine_args(&form("via", &[("via", "x")]))
                .unwrap_err()
                .contains("needs a name")
        );
    }
}
