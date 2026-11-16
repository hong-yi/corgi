//! What keys and clicks do: the prefix table's actions, and the chrome
//! (sidebar and tab clicks, seen-tracking, window title, config reload).

use super::*;

impl App {
    pub(super) fn run(&mut self, action: Action) {
        // These work without a window.
        let action = match action {
            Action::Detach => {
                self.quit = true;
                return;
            }
            Action::ReloadConfig(path) => {
                // Another file: that one, from now on (C-b R reads it again).
                if let Some(p) = path {
                    let p = std::path::PathBuf::from(crate::form::expand_home(&p, &super::dialogs::home()));
                    if !p.is_file() {
                        self.notify(format!("source-file: no file {}", p.display()));
                        return;
                    }
                    self.config_path = p;
                }
                return self.reload_config();
            }
            Action::ListKeys => {
                self.mode = Mode::Keys(Default::default());
                return;
            }
            Action::CommandPrompt {
                label,
                initial,
                template,
            } => {
                let label = label.unwrap_or_else(|| ":".into());
                self.prompt(&label, initial.unwrap_or_default(), Then::Command { template });
                return;
            }
            Action::ConfirmBefore { prompt, action } => {
                let q = self.expand(&prompt.unwrap_or_else(|| "are you sure? (y/n)".into()));
                self.mode = Mode::Prompt(Prompt {
                    label: q,
                    text: String::new(),
                    then: Then::Confirm(action),
                    hist: None,
                });
                return;
            }
            Action::SwitchTable(t) => {
                self.mode = Mode::Table(t);
                return;
            }
            Action::Dialog(kind) => {
                self.open_dialog(kind);
                return;
            }
            Action::Menu => {
                self.open_menu(None);
                return;
            }
            Action::Usage => {
                self.open_usage();
                return;
            }
            Action::NewWorkspace { cwd } => {
                let here = std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                // `~` is home here, as tmux has it (a new workspace starts on this machine).
                let cwd = cwd.map(|c| crate::form::expand_home(&c, &super::dialogs::home()));
                let folder = cwd.or_else(|| self.focused_cwd()).unwrap_or(here);
                self.new_workspace(folder);
                return;
            }
            other => other,
        };
        let Some((ws, win)) = self.active() else { return };
        let Tiling { area: body, sep, .. } = self.tiling();
        if self.sidebar_focus.is_some() && self.sidebar_action(&action) {
            return;
        }
        let machine = self.doc.workspace(&ws).and_then(|s| s.machine.clone());
        let changed = match action {
            Action::NewWindow { name, cwd, cmd } => {
                let w = self.doc.alloc('w');
                let cwd = cwd
                    .map(|c| self.home_for(&c, machine.as_deref()))
                    .or_else(|| self.focused_cwd());
                self.spawn(Purpose::Window { ws, win: w, name }, cwd, machine, cmd.map(Run::Line));
                false
            }
            Action::NextWindow | Action::PrevWindow | Action::LastWindow | Action::SelectWindow(_) => {
                let s = self.doc.workspace_mut(&ws).unwrap();
                let i = s.windows.iter().position(|w| w.id == win).unwrap_or(0);
                let n = s.windows.len();
                let target = match action {
                    Action::NextWindow => Some(s.windows[(i + 1) % n].id.clone()),
                    Action::PrevWindow => Some(s.windows[(i + n - 1) % n].id.clone()),
                    Action::LastWindow => s.last.clone(),
                    Action::SelectWindow(k) => s.windows.get(k as usize).map(|w| w.id.clone()),
                    _ => None,
                };
                match target {
                    Some(t) => s.select(&t),
                    None => self.notify("no such window".into()),
                }
                true
            }
            Action::RenameWindow(Some(name)) => {
                if let Some(w) = self.window_mut() {
                    w.name = Some(name).filter(|n| !n.is_empty());
                }
                true
            }
            Action::RenameWindow(None) => {
                self.open_rename(super::dialogs::Does::RenameWindow { ws, win });
                false
            }
            Action::RenamePane(name) => {
                let pane = self.window().map(|w| w.focus.clone()).unwrap_or_default();
                match name {
                    Some(n) => self.name_pane(&pane, Some(n).filter(|n| !n.is_empty())),
                    None => self.open_rename(super::dialogs::Does::RenamePane { pane }),
                }
                false
            }
            Action::RenameWorkspace(Some(name)) => {
                if let Some(s) = self.doc.workspace_mut(&ws) {
                    s.name = Some(name).filter(|n| !n.is_empty());
                }
                true
            }
            Action::RenameWorkspace(None) => {
                self.open_rename(super::dialogs::Does::RenameWorkspace { ws });
                false
            }
            Action::KillPane | Action::KillWindow | Action::KillWorkspace => {
                if !self.ask_close(&action) {
                    self.close_now(&action);
                }
                false
            }
            Action::SplitWindow { dir, pct, cwd, cmd } => {
                let target = self.window().map(|w| w.focus.clone()).unwrap_or_default();
                let cwd = cwd
                    .map(|c| self.home_for(&c, machine.as_deref()))
                    .or_else(|| self.focused_cwd());
                self.spawn(
                    Purpose::Split {
                        ws,
                        win,
                        target,
                        dir,
                        pct,
                    },
                    cwd,
                    machine,
                    cmd.map(Run::Line),
                );
                false
            }
            Action::SelectPane(side) => {
                if side == crate::layout::Side::Left
                    && self.areas().sidebar.is_some()
                    && self
                        .window()
                        .is_some_and(|w| w.root.neighbour_with(body, sep, &w.focus, side).is_none())
                {
                    // Off the left edge: into the sidebar.
                    let at = crate::tree::Target::Window { ws: ws.clone(), win };
                    self.sidebar_focus = Some(crate::chooser::Chooser::sidebar(Some(at)));
                    self.sync();
                    return;
                }
                let w = self.window_mut().unwrap();
                w.zoomed = false;
                let from = w.focus.clone();
                match w.root.neighbour_with(body, sep, &from, side) {
                    Some(n) => w.set_focus(&n),
                    None => return,
                }
                true
            }
            Action::NextPane | Action::PrevPane => {
                let w = self.window_mut().unwrap();
                let ps: Vec<String> = w.root.panes().into_iter().cloned().collect();
                let i = ps.iter().position(|p| *p == w.focus).unwrap_or(0);
                let n = ps.len();
                let next = ps[if action == Action::NextPane {
                    (i + 1) % n
                } else {
                    (i + n - 1) % n
                }]
                .clone();
                w.zoomed = false;
                w.set_focus(&next);
                true
            }
            Action::LastPane => {
                let w = self.window_mut().unwrap();
                match w.last.clone() {
                    Some(l) => w.set_focus(&l),
                    None => self.notify("no last pane".into()),
                }
                true
            }
            Action::Zoom => {
                let w = self.window_mut().unwrap();
                w.zoomed = !w.zoomed && w.root.panes().len() > 1;
                true
            }
            Action::SwapPrev | Action::SwapNext => {
                let w = self.window_mut().unwrap();
                let ps: Vec<String> = w.root.panes().into_iter().cloned().collect();
                let i = ps.iter().position(|p| *p == w.focus).unwrap_or(0);
                let n = ps.len();
                let j = if action == Action::SwapNext {
                    (i + 1) % n
                } else {
                    (i + n - 1) % n
                };
                if i != j {
                    w.root.swap(&ps[i], &ps[j]);
                }
                true
            }
            Action::SwapDir(side) => {
                let w = self.window_mut().unwrap();
                let f = w.focus.clone();
                match w.root.neighbour_with(body, sep, &f, side) {
                    Some(other) => w.root.swap(&f, &other),
                    None => return,
                }
                true
            }
            Action::BreakPane => {
                let w = self.window_mut().unwrap();
                if w.root.panes().len() < 2 {
                    self.notify("can't break with only one pane".into());
                    return;
                }
                let pane = w.focus.clone();
                self.doc.remove_pane(&pane);
                let new = self.doc.alloc('w');
                self.doc.add_window(&ws, Window::new(new.clone(), pane.clone()));
                self.label(&pane, &ws, &new);
                true
            }
            Action::NextLayout | Action::SelectLayout(_) => {
                let w = self.window_mut().unwrap();
                let p = match action {
                    Action::SelectLayout(p) => p,
                    _ => w.preset.map(Preset::next).unwrap_or_default(),
                };
                w.root = w.root.preset(p);
                w.preset = Some(p);
                w.zoomed = false;
                self.notify(p.name().into());
                true
            }
            Action::Resize(side, n) => {
                let w = self.window_mut().unwrap();
                let f = w.focus.clone();
                w.root.resize_with(body, sep, &f, side, n)
            }
            Action::PrevWorkspace | Action::NextWorkspace => {
                let n = self.doc.workspaces.len();
                let i = self.doc.workspaces.iter().position(|s| s.id == ws).unwrap_or(0);
                let j = if action == Action::NextWorkspace {
                    (i + 1) % n
                } else {
                    (i + n - 1) % n
                };
                self.view = self.doc.workspaces[j].id.clone();
                self.save_view();
                self.notify(format!("workspace {}", self.ws_label(&self.doc.workspaces[j])));
                false
            }
            Action::ChooseTree { workspaces } => {
                let at = if workspaces {
                    crate::tree::Target::Workspace(ws)
                } else {
                    crate::tree::Target::Window { ws, win }
                };
                self.mode = Mode::Chooser(crate::chooser::Chooser::new(Some(at)));
                false
            }
            Action::NextAgent { done, any, back } => {
                self.next_agent(done, any, back);
                return;
            }
            Action::SelectAgent(n) => {
                match self.tree_agents().into_iter().nth(n - 1).and_then(|r| r.target) {
                    Some(t) => self.go(t),
                    None => self.notify(format!("no agent {n}")),
                }
                return;
            }
            Action::SendPrefix => {
                if let Some(k) = input::to_vt(&self.keymap.prefix.event()) {
                    self.type_key(&k);
                }
                false
            }
            Action::SendKeys(words, true) => {
                // As text: each word as typed, a space between.
                let text = words.join(" ");
                for c in text.chars() {
                    self.type_key(&corgi_vt::Key::Char(c, corgi_vt::Mods::default()));
                }
                false
            }
            Action::SendKeys(words, false) => {
                for w in words {
                    match crate::keys::Key::parse(&w).and_then(|k| input::to_vt(&k.event())) {
                        Some(k) if w.chars().count() > 1 || !w.is_empty() => self.type_key(&k),
                        _ => {
                            for c in w.chars() {
                                self.type_key(&corgi_vt::Key::Char(c, corgi_vt::Mods::default()));
                            }
                        }
                    }
                }
                false
            }
            Action::CopyMode { up } => {
                self.start_copy(up);
                return;
            }
            Action::PasteBuffer => {
                self.paste_buffer();
                return;
            }
            Action::Popup {
                cmd,
                cwd,
                width,
                height,
                title,
                close,
            } => {
                let machine = self
                    .active()
                    .and_then(|(ws, _)| self.doc.workspace(&ws).and_then(|s| s.machine.clone()));
                let title = title.or_else(|| cmd.clone()).unwrap_or_else(|| "shell".into());
                self.open_popup(super::popup::Ask {
                    cwd: cwd.or_else(|| self.focused_cwd()),
                    cmd,
                    machine,
                    width,
                    height,
                    title,
                    close,
                    tmp: None,
                });
                return;
            }
            Action::RunShell(line) => {
                self.run_shell(line);
                return;
            }
            Action::EditScrollback => {
                self.edit_scrollback();
                return;
            }
            Action::PasteImage => {
                self.paste_clipboard_image();
                return;
            }
            Action::ChooseWorktree => {
                self.choose_worktree();
                return;
            }
            Action::ChooseMachine => {
                self.choose_machine(None);
                return;
            }
            Action::Machine(verb, name) => {
                self.machine(verb, name);
                return;
            }
            Action::DisplayPanes => {
                self.mode = Mode::Panes(Instant::now() + Duration::from_secs(1));
                false
            }
            Action::ToggleSidebar => {
                self.sidebar_focus = None;
                self.sidebar_mode = match self.sidebar_mode {
                    SidebarMode::Hidden => self.config.sidebar.mode.max_shown(),
                    _ => SidebarMode::Hidden,
                };
                false
            }
            // Handled above.
            Action::Detach
            | Action::ReloadConfig(_)
            | Action::ListKeys
            | Action::CommandPrompt { .. }
            | Action::ConfirmBefore { .. }
            | Action::SwitchTable(_)
            | Action::Dialog(_)
            | Action::Menu
            | Action::Usage
            | Action::NewWorkspace { .. } => false,
        };
        if changed {
            self.dirty = true;
        }
        self.sync();
    }

    /// After a y/n prompt said yes.
    pub(super) fn confirmed(&mut self, action: Action) {
        self.run(action);
    }

    /// tmux-style placeholders in prompts: `#P` pane, `#I` window index, `#W` window, `#S` workspace.
    pub(super) fn expand(&self, s: &str) -> String {
        let pane = self.window().map(|w| w.focus.clone()).unwrap_or_default();
        let index = self.window_index().map(|i| i.to_string()).unwrap_or_default();
        let window = self
            .window()
            .map(|w| w.name.clone().unwrap_or_else(|| self.pane_name(&w.focus)))
            .unwrap_or_default();
        let ws = self
            .active()
            .and_then(|(ws, _)| self.doc.workspace(&ws).map(|s| self.ws_label(s)))
            .unwrap_or_default();
        s.replace("#P", &pane)
            .replace("#I", &index)
            .replace("#W", &window)
            .replace("#S", &ws)
    }

    /// `display-panes`: pick the window's pane by its number.
    pub(super) fn select_pane_index(&mut self, i: usize) {
        if let Some(w) = self.window_mut() {
            let ps: Vec<String> = w.root.panes().into_iter().cloned().collect();
            if let Some(p) = ps.get(i) {
                w.zoomed = false;
                w.set_focus(p);
                self.dirty = true;
            }
        }
        self.sync();
    }

    /// The next agent (after the focused pane, in tree order, wrapping) that's blocked, or finished unseen.
    /// The next agent waiting for you (`done`: finished unseen; `any`: any agent, in the
    /// sidebar's order), round from the end; `back`: the one before.
    fn next_agent(&mut self, done: bool, any: bool, back: bool) {
        let want = if done {
            crate::status::Agent::Done
        } else {
            crate::status::Agent::Blocked
        };
        let order: Vec<crate::tree::Target> = self
            .tree_agents()
            .into_iter()
            .filter(|r| any || r.state == Some(want))
            .filter_map(|r| r.target)
            .collect();
        let here = self.window().map(|w| w.focus.clone());
        let at = order
            .iter()
            .position(|t| matches!(t, crate::tree::Target::Pane { pane, .. } if Some(pane) == here.as_ref()));
        let next = match (at, back) {
            (Some(i), false) => order.get((i + 1) % order.len()).cloned(),
            (Some(i), true) => order.get((i + order.len() - 1) % order.len()).cloned(),
            (None, false) => order.first().cloned(),
            (None, true) => order.last().cloned(),
        };
        match next {
            Some(t) => self.go(t),
            None => self.notify(
                if any {
                    "no agents running"
                } else if done {
                    "no agent has finished unseen"
                } else {
                    "no agent is waiting for you"
                }
                .into(),
            ),
        }
    }

    // ----- chrome -----

    /// A click on the status line or sidebar. False if it wasn't on either.
    pub(super) fn click(&mut self, at: ratatui::layout::Position) -> bool {
        if self.click_toast(at) {
            return true;
        }
        if let Some((_, hit)) = self.hits.tabs.iter().find(|(r, _)| r.contains(at)).cloned() {
            match hit {
                TabHit::Window(id) => {
                    if let Some((ws, _)) = self.active()
                        && let Some(s) = self.doc.workspace_mut(&ws)
                    {
                        s.select(&id);
                        self.dirty = true;
                    }
                    self.sync();
                }
                TabHit::NewWindow => self.run(Action::NewWindow {
                    name: None,
                    cwd: None,
                    cmd: None,
                }),
                TabHit::Usage => self.open_usage(),
                TabHit::Switch => self.run(Action::ChooseTree { workspaces: false }),
            }
            return true;
        }
        if let Some((_, hit)) = self.hits.sidebar.iter().find(|(r, _)| r.contains(at)).cloned() {
            match hit {
                crate::sidebar::Hit::Fold(t) => {
                    self.fold.toggle(&t);
                    self.save_view();
                }
                crate::sidebar::Hit::Go(t) => self.go(t),
            }
            return true;
        }
        false
    }

    /// A prefix action while the sidebar has the keyboard. True if it was the sidebar's to handle;
    /// anything else hands the keyboard back to the panes and runs as usual.
    pub(super) fn sidebar_action(&mut self, action: &Action) -> bool {
        use crate::layout::Side;
        match action {
            Action::SelectPane(Side::Right) => self.sidebar_focus = None,
            Action::SelectPane(_) => {}
            Action::Resize(side @ (Side::Left | Side::Right), n) => {
                let now = self.areas().sidebar.map_or(self.config.sidebar.width, |r| r.width);
                let w = if *side == Side::Right {
                    now.saturating_add(*n)
                } else {
                    now.saturating_sub(*n)
                };
                self.set_sidebar_width(w);
                self.save_view();
            }
            Action::Resize(..) => {}
            _ => {
                self.sidebar_focus = None;
                return false;
            }
        }
        self.sync();
        true
    }

    /// Keys (no prefix) while the sidebar has the keyboard: move, fold, Enter to go, Esc to leave.
    pub(super) fn sidebar_key(&mut self, k: crossterm::event::KeyEvent) {
        let Some(mut ch) = self.sidebar_focus.take() else {
            return;
        };
        use crossterm::event::{KeyCode, KeyModifiers};
        // m: the action menu for the row (or agent) under the cursor.
        if k.code == KeyCode::Char('m') && k.modifiers.is_empty() {
            let at = ch.agent.clone().or_else(|| ch.at.clone());
            self.sidebar_focus = Some(ch);
            self.open_menu(at);
            return;
        }
        let agents: Vec<_> = self.sidebar_agents().iter().map(|a| a.target()).collect();
        let rows = self.tree_rows("");
        let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if ch.agent.is_some() {
            match agent_key(&mut ch, &k, &agents, &rows) {
                crate::chooser::Outcome::Close => self.sync(),
                crate::chooser::Outcome::Go(t) => self.go(t),
                _ => {
                    if ch.agent.is_none() {
                        ch.settle(&rows);
                    }
                    self.sidebar_focus = Some(ch);
                }
            }
            return;
        }
        // Into the agents list: `a`, or down from the tree's last row.
        let last = rows.iter().position(|r| r.target == ch.at).unwrap_or(0) + 1 >= rows.len();
        let down = plain && matches!(k.code, KeyCode::Down | KeyCode::Char('j'));
        if !agents.is_empty() && ((plain && k.code == KeyCode::Char('a')) || (down && last)) {
            ch.agent = agents.first().cloned();
            self.sidebar_focus = Some(ch);
            return;
        }
        let before = self.fold.clone();
        match ch.key(&k, &rows, &mut self.fold) {
            crate::chooser::Outcome::Close => self.sync(),
            crate::chooser::Outcome::Go(t) => self.go(t),
            crate::chooser::Outcome::NewWorkspace(name) => self.new_workspace_dialog(name),
            crate::chooser::Outcome::Stay => {
                ch.settle(&self.tree_rows(""));
                self.sidebar_focus = Some(ch);
            }
        }
        if self.fold != before {
            self.save_view();
        }
    }

    pub(super) fn set_sidebar_width(&mut self, w: u16) {
        let max = self.screen.width.saturating_sub(20).max(12);
        self.sidebar_width = Some(w.clamp(12, max));
    }

    /// Mouse on the sidebar's right edge: press to grab, drag to resize, release to keep.
    pub(super) fn drag_sidebar(&mut self, kind: crossterm::event::MouseEventKind, col: u16, row: u16) -> bool {
        use crossterm::event::{MouseButton, MouseEventKind};
        let Some(side) = self.areas().sidebar else {
            self.dragging = false;
            return false;
        };
        match kind {
            MouseEventKind::Down(MouseButton::Left)
                if col + 1 == side.right() && row >= side.y && row < side.bottom() =>
            {
                self.dragging = true;
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging => {
                self.set_sidebar_width(col.saturating_sub(side.x) + 1);
                self.sync();
            }
            MouseEventKind::Up(MouseButton::Left) if self.dragging => {
                self.dragging = false;
                self.save_view();
            }
            _ => return false,
        }
        true
    }

    /// Show a workspace, window or pane (sidebar click, chooser Enter, agent row).
    pub(super) fn go(&mut self, t: crate::tree::Target) {
        use crate::tree::Target;
        let (ws, win, pane) = match t {
            Target::Machine(m) => {
                // Its workspace you were in last (else its first); none: what's there to do.
                let on = |s: &&crate::model::Workspace| s.machine.as_deref().unwrap_or("") == m;
                let ws = self
                    .doc
                    .workspaces
                    .iter()
                    .filter(on)
                    .find(|s| s.id == self.view)
                    .or_else(|| self.doc.workspaces.iter().find(on))
                    .map(|s| s.id.clone());
                match ws {
                    Some(ws) => (ws, None, None),
                    None if m.is_empty() => return,
                    None => return self.choose_machine(Some(&m)),
                }
            }
            Target::Workspace(ws) => (ws, None, None),
            Target::Window { ws, win } => (ws, Some(win), None),
            Target::Pane { ws, win, pane } => (ws, Some(win), Some(pane)),
        };
        if let Some(s) = self.doc.workspace_mut(&ws) {
            if let Some(win) = &win {
                s.select(win);
                if let (Some(w), Some(p)) = (s.window_mut(win), &pane) {
                    w.set_focus(p);
                    w.zoomed = false;
                }
            }
            self.dirty = true;
        }
        self.view = ws;
        self.save_view();
        self.sync();
    }

    pub(super) fn save_view(&self) {
        persist::save_view(
            &self.session,
            &persist::View {
                workspace: self.view.clone(),
                fold: self.fold.clone(),
                sidebar_width: self.sidebar_width,
            },
        );
    }

    /// The workspace tree as the sidebar and chooser show it.
    pub(super) fn tree_rows(&self, filter: &str) -> Vec<crate::tree::Row> {
        self.with_tree(|i| crate::tree::rows(i, &self.fold, filter))
    }

    /// Every agent's row, in the tree's order (the sidebar's bottom part).
    pub(super) fn tree_agents(&self) -> Vec<crate::tree::Row> {
        self.with_tree(crate::tree::agents)
    }

    /// The sidebar's agents list: every pane that reports a status, in the order the sidebar
    /// shows them (`sidebar.agent_sort`), folded or not.
    pub(super) fn sidebar_agents(&self) -> Vec<crate::sidebar::AgentRow> {
        let mut agents: Vec<_> = self
            .tree_agents()
            .into_iter()
            .enumerate()
            .filter_map(|(order, row)| match &row.target {
                Some(crate::tree::Target::Pane { ws, win, pane }) => Some(crate::sidebar::AgentRow {
                    ws: ws.clone(),
                    win: win.clone(),
                    pane: pane.clone(),
                    order,
                    row,
                }),
                _ => None,
            })
            .collect();
        crate::sidebar::sort_agents(&mut agents, self.config.sidebar.agent_sort);
        agents
    }

    fn with_tree<T>(&self, f: impl FnOnce(&crate::tree::Inputs) -> T) -> T {
        let (ws, win) = self.active().unwrap_or_default();
        let pane = self.window().map(|w| w.focus.clone()).unwrap_or_default();
        let name = |p: &str| self.pane_name(p);
        let inputs = crate::tree::Inputs {
            doc: &self.doc,
            terminals: &self.terminals,
            name: &name,
            branches: &self.branches,
            current: (&ws, &win, &pane),
            repos: &self.repos,
            label: &|s| self.ws_label(s),
            folder: &|s| git_key(s.machine.as_deref(), &self.live_folder(s)),
            machines: &self.machines,
            host: &self.hostname,
        };
        f(&inputs)
    }

    /// You're looking at this pane: its finished turn is no longer "done".
    pub(super) fn mark_seen(&mut self, addr: &str) {
        let Some(t) = self.terminals.get_mut(addr) else { return };
        if t.turn <= status::seen(t) || self.conn != Conn::Up {
            return;
        }
        t.labels.insert(status::L_SEEN.into(), t.turn.to_string());
        let (c, a, turn) = (self.client.clone(), addr.to_string(), t.turn.to_string());
        tokio::spawn(async move {
            let _ = c
                .call(
                    "terminal.label",
                    json!({"terminal": a, "labels": {status::L_SEEN: turn}}),
                )
                .await;
        });
    }

    /// A `-c` folder with `~` as home: this machine's here; on another machine its own home,
    /// which only it knows, so `~` is left for it (core starts a terminal there in its home).
    fn home_for(&self, c: &str, machine: Option<&str>) -> String {
        match machine {
            None => crate::form::expand_home(c, &super::dialogs::home()),
            Some(_) => c.to_string(),
        }
    }

    /// Whether a terminal's folder is a workspace's (its first window's first-made pane).
    pub(super) fn gives_folder(&self, addr: &str) -> bool {
        self.doc.workspaces.iter().any(|s| {
            s.windows
                .first()
                .and_then(|w| {
                    crate::tree::made_order(w.root.panes())
                        .first()
                        .map(|p| p.as_str() == addr)
                })
                .unwrap_or(false)
        })
    }

    /// A git scan finished: run the one asked for meanwhile, if any.
    pub(super) fn scan_done(&mut self) {
        self.scanning = None;
        if std::mem::take(&mut self.rescan) {
            self.tick();
        }
    }

    /// Slow-changing things: git branches, checkouts and repositories of the workspaces' folders;
    /// on this machine with git, on others (whose line is up) with one `machine.exec` each, side
    /// by side. One scan at a time: asked again meanwhile, it runs once more when it's done.
    pub(super) fn tick(&mut self) {
        // (One that's taken over a minute is taken as lost.)
        if self.scanning.is_some_and(|t| t.elapsed() < Duration::from_secs(60)) {
            self.rescan = true;
            return;
        }
        let mut local: Vec<String> = Vec::new();
        let mut remote: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for s in &self.doc.workspaces {
            let f = self.live_folder(s);
            if f.is_empty() {
                continue;
            }
            let list = match &s.machine {
                None => &mut local,
                Some(m) if self.machines.iter().any(|x| &x.name == m && x.is_up()) => {
                    remote.entry(m.clone()).or_default()
                }
                Some(_) => continue,
            };
            if !list.contains(&f) {
                list.push(f);
            }
        }
        if local.is_empty() && remote.is_empty() {
            return;
        }
        // What's known about machines not asked now (down, say) stays.
        let keep = |m: &HashMap<String, String>| -> HashMap<String, String> {
            m.iter()
                .filter(|(k, _)| k.contains(KEY_SEP) && !remote.keys().any(|r| k.starts_with(&format!("{r}{KEY_SEP}"))))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        let (mut branches, mut repos, mut tops) = (keep(&self.branches), keep(&self.repos), keep(&self.toplevels));
        self.scanning = Some(Instant::now());
        let (c, out) = (self.client.clone(), self.out.clone());
        tokio::spawn(async move {
            let git = |f: &str, args: &[&str]| {
                let mut c = tokio::process::Command::new("git");
                c.arg("-C")
                    .arg(f)
                    .args(args)
                    .stdin(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    // Past its time it's killed, not left running.
                    .kill_on_drop(true);
                async move {
                    match tokio::time::timeout(Duration::from_secs(2), c.output()).await {
                        Ok(Ok(o)) if o.status.success() => Some(String::from_utf8_lossy(&o.stdout).trim().to_string()),
                        _ => None,
                    }
                }
            };
            // The other machines, all at once (a slow one holds up only its own).
            let asks: Vec<_> = remote
                .into_iter()
                .map(|(m, folders)| {
                    let c = c.clone();
                    tokio::spawn(async move {
                        let line = crate::worktree::facts_script(&folders);
                        let r = super::worktrees::exec_for(&c, &line, Some(&m), 10).await;
                        (m, r)
                    })
                })
                .collect();
            let there = async {
                let mut all = Vec::new();
                for a in asks {
                    if let Ok(r) = a.await {
                        all.push(r);
                    }
                }
                all
            };
            let here = async {
                let mut facts = Vec::new();
                for f in local {
                    let b = git(&f, &["symbolic-ref", "--short", "-q", "HEAD"])
                        .await
                        .filter(|b| !b.is_empty());
                    // The checkout it's in (named after, when the workspace has no name of its own).
                    let t = git(&f, &["rev-parse", "--show-toplevel"])
                        .await
                        .filter(|t| !t.is_empty());
                    // The repository's own checkout: where its shared `.git` is.
                    let r = git(&f, &["rev-parse", "--path-format=absolute", "--git-common-dir"])
                        .await
                        .map(|c| crate::worktree::repo_of(&c));
                    facts.push((f, b, t, r));
                }
                facts
            };
            let (here, there) = tokio::join!(here, there);
            for (f, b, t, r) in here {
                if let Some(b) = b {
                    branches.insert(f.clone(), b);
                }
                if let Some(t) = t {
                    tops.insert(f.clone(), t);
                }
                if let Some(r) = r {
                    repos.insert(f, r);
                }
            }
            for (m, r) in there {
                let Ok(text) = r else { continue };
                for (f, b, t, r) in crate::worktree::parse_facts(&text) {
                    let key = git_key(Some(&m), &f);
                    if let Some(b) = b {
                        branches.insert(key.clone(), b);
                    }
                    if let Some(t) = t {
                        tops.insert(key.clone(), t);
                    }
                    if let Some(r) = r {
                        repos.insert(key, git_key(Some(&m), &r));
                    }
                }
            }
            let _ = out.send(Msg::Branches(branches, repos, tops));
        });
    }

    /// The host terminal's window title: corgi · workspace:window.
    pub(super) fn update_title(&mut self) {
        if !self.config.ui.window_title {
            return;
        }
        let Some((ws, _)) = self.active() else { return };
        let s = self.doc.workspace(&ws).map(|s| self.ws_label(s)).unwrap_or_default();
        let w = self
            .window()
            .map(|w| w.name.clone().unwrap_or_else(|| self.pane_name(&w.focus)))
            .unwrap_or_default();
        let title = format!("corgi · {s}:{w}");
        if title != self.title {
            let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::SetTitle(&title));
            self.title = title;
        }
    }

    /// C-b R: read the config again. A broken file keeps the current config.
    pub(super) fn reload_config(&mut self) {
        match config::load(&self.config_path) {
            Ok(l) => {
                let bg = self.display.as_ref().and_then(|d| d.bg.clone());
                if let Ok(t) = Theme::resolve(&l.config.theme, bg.as_deref()) {
                    self.theme = Theme {
                        rounded: l.config.ui.corners == crate::config::Corners::Rounded,
                        ..t
                    };
                }
                match crate::keys::Keymap::build(&l.config.keys) {
                    Ok(k) => self.keymap = k,
                    Err(e) => tracing::warn!("{e}"),
                }
                self.sidebar_mode = l.config.sidebar.mode;
                // The mouse taken or given back now, not only at the next start.
                if l.config.ui.mouse != self.wide.ui.mouse {
                    crate::term::mouse(l.config.ui.mouse);
                }
                self.host_out = super::notify::HostOut::new(l.config.notifications.host);
                self.wide = l.config;
                self.narrow = false;
                self.config = self.wide.clone();
                self.fit_width();
                self.start_usage();
                let msg = if l.warnings.is_empty() {
                    "config reloaded".to_string()
                } else {
                    format!("config reloaded; {}", l.warnings.join("; "))
                };
                self.notify(msg);
                self.title.clear();
                self.sync();
            }
            Err(e) => self.notify(format!("config not reloaded: {e}")),
        }
    }
}

/// A key while the sidebar's cursor is in the agents list (`agents`: their panes, in the
/// sidebar's order). `Stay` with `ch.agent` cleared means the cursor went back up to the tree,
/// onto its last row (`rows`).
fn agent_key(
    ch: &mut crate::chooser::Chooser,
    k: &crossterm::event::KeyEvent,
    agents: &[crate::tree::Target],
    rows: &[crate::tree::Row],
) -> crate::chooser::Outcome {
    use crate::chooser::Outcome;
    use crossterm::event::{KeyCode, KeyModifiers};
    let Some(at) = ch.agent.clone() else {
        return Outcome::Stay;
    };
    // Its agent went away: the first one, or the tree when there are none.
    let Some(i) = agents.iter().position(|a| *a == at) else {
        ch.agent = agents.first().cloned();
        return Outcome::Stay;
    };
    let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let up = matches!(k.code, KeyCode::Up)
        || (plain && k.code == KeyCode::Char('k'))
        || (ctrl && matches!(k.code, KeyCode::Char('p')));
    let down = matches!(k.code, KeyCode::Down)
        || (plain && k.code == KeyCode::Char('j'))
        || (ctrl && matches!(k.code, KeyCode::Char('n')));
    let to_tree = |ch: &mut crate::chooser::Chooser, row: Option<&crate::tree::Row>| {
        ch.agent = None;
        if let Some(r) = row {
            ch.at = r.target.clone();
        }
    };
    match k.code {
        _ if up && i == 0 => to_tree(ch, rows.last()),
        _ if up => ch.agent = Some(agents[i - 1].clone()),
        _ if down => ch.agent = Some(agents[(i + 1).min(agents.len() - 1)].clone()),
        KeyCode::Home => ch.agent = agents.first().cloned(),
        KeyCode::End => ch.agent = agents.last().cloned(),
        // Back to the tree, where the cursor was.
        KeyCode::Char('a') if plain => ch.agent = None,
        KeyCode::Enter => return Outcome::Go(at),
        KeyCode::Esc => return Outcome::Close,
        KeyCode::Char('c' | 'g') if ctrl => return Outcome::Close,
        _ => {}
    }
    Outcome::Stay
}

#[cfg(test)]
mod tests {
    use super::agent_key;
    use crate::chooser::{Chooser, Outcome};
    use crate::tree::{Row, Target};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn pane(p: &str) -> Target {
        Target::Pane {
            ws: "s1".into(),
            win: "w1".into(),
            pane: p.into(),
        }
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    #[test]
    fn the_agents_list_has_its_own_cursor() {
        let agents = [pane("t1"), pane("t2"), pane("t3")];
        let rows = [
            Row {
                target: Some(Target::Workspace("s1".into())),
                ..Default::default()
            },
            Row {
                target: Some(Target::Workspace("s2".into())),
                ..Default::default()
            },
        ];
        let mut ch = Chooser::sidebar(Some(Target::Workspace("s1".into())));
        ch.agent = Some(pane("t1"));
        agent_key(&mut ch, &key(KeyCode::Char('j')), &agents, &rows);
        agent_key(&mut ch, &key(KeyCode::Down), &agents, &rows);
        agent_key(&mut ch, &key(KeyCode::Down), &agents, &rows);
        assert_eq!(ch.agent, Some(pane("t3")), "stops at the last");
        agent_key(&mut ch, &key(KeyCode::Home), &agents, &rows);
        assert_eq!(ch.agent, Some(pane("t1")));
        assert_eq!(agent_key(&mut ch, &key(KeyCode::End), &agents, &rows), Outcome::Stay);
        assert_eq!(
            agent_key(&mut ch, &key(KeyCode::Enter), &agents, &rows),
            Outcome::Go(pane("t3"))
        );
        // Up from the first agent: the tree's last row.
        ch.agent = Some(pane("t1"));
        agent_key(&mut ch, &key(KeyCode::Char('k')), &agents, &rows);
        assert_eq!(ch.agent, None);
        assert_eq!(ch.at, Some(Target::Workspace("s2".into())));
        // `a` goes back to where the tree's cursor was.
        ch.at = Some(Target::Workspace("s1".into()));
        ch.agent = Some(pane("t2"));
        agent_key(&mut ch, &key(KeyCode::Char('a')), &agents, &rows);
        assert_eq!(
            (ch.agent.clone(), ch.at.clone()),
            (None, Some(Target::Workspace("s1".into())))
        );
        // Its agent went away: onto the first that's left.
        ch.agent = Some(pane("t9"));
        agent_key(&mut ch, &key(KeyCode::Down), &agents, &rows);
        assert_eq!(ch.agent, Some(pane("t1")));
        ch.agent = Some(pane("t1"));
        assert_eq!(agent_key(&mut ch, &key(KeyCode::Esc), &agents, &rows), Outcome::Close);
    }
}
