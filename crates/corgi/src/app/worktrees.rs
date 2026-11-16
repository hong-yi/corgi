//! The Worktrees dialog in the app: listing the current workspace's repository's worktrees
//! (through core, on the workspace's machine), opening one as a workspace (or going to it),
//! and removing one (asked first; its workspace closes too).

use super::*;
use crate::worktrees::{Outcome, View, Worktree};

/// What the dialog shows: the worktrees, of which repository (a folder in it), on which machine.
pub(super) struct Shown {
    pub view: View,
    pub list: Vec<Worktree>,
    pub folder: String,
    pub machine: Option<String>,
}

impl App {
    /// `C-b W`: the current workspace's repository's worktrees.
    pub(super) fn choose_worktree(&mut self) {
        let Some((ws, _)) = self.active() else { return };
        let Some(s) = self.doc.workspace(&ws) else { return };
        let (folder, machine) = (self.live_folder(s), s.machine.clone());
        if folder.is_empty() {
            self.notify("this workspace has no folder".into());
            return;
        }
        let line = crate::worktrees::list_script(&folder);
        let (c, out) = (self.client.clone(), self.out.clone());
        tokio::spawn(async move {
            let r = exec(&c, &line, machine.as_deref())
                .await
                .map(|o| crate::worktrees::parse(&o));
            let _ = out.send(Msg::Worktrees(r, folder, machine));
        });
    }

    pub(super) fn worktrees_listed(
        &mut self,
        r: Result<Vec<Worktree>, String>,
        folder: String,
        machine: Option<String>,
    ) {
        match r {
            Ok(list) => {
                let at = list
                    .iter()
                    .position(|w| {
                        self.workspace_at(&w.path, &machine)
                            .is_some_and(|ws| Some(ws) == self.active().map(|a| a.0))
                    })
                    .unwrap_or(0);
                self.mode = Mode::Worktrees(Box::new(Shown {
                    view: View { at },
                    list,
                    folder,
                    machine,
                }));
            }
            Err(e) => self.notify(format!("no worktrees: {e}")),
        }
    }

    /// The workspace open on `path` (on `machine`), if any.
    fn workspace_at(&self, path: &str, machine: &Option<String>) -> Option<String> {
        let path = path.trim_end_matches('/');
        self.doc
            .workspaces
            .iter()
            .find(|s| s.folder.trim_end_matches('/') == path && &s.machine == machine)
            .map(|s| s.id.clone())
    }

    /// Which of the dialog's worktrees are open as workspaces.
    pub(super) fn worktrees_open(&self, sh: &Shown) -> Vec<bool> {
        sh.list
            .iter()
            .map(|w| self.workspace_at(&w.path, &sh.machine).is_some())
            .collect()
    }

    pub(super) fn worktrees_key(&mut self, mut sh: Box<Shown>, k: KeyEvent) {
        match sh.view.key(&k, sh.list.len()) {
            Outcome::Stay => self.mode = Mode::Worktrees(sh),
            Outcome::Close => {}
            Outcome::New => self.open_dialog(crate::commands::DialogKind::NewWorktree),
            Outcome::Open(i) => {
                let w = &sh.list[i];
                match self.workspace_at(&w.path, &sh.machine) {
                    Some(ws) => self.go(crate::tree::Target::Workspace(ws)),
                    None => {
                        let (ws, win) = (self.doc.alloc('s'), self.doc.alloc('w'));
                        self.spawn(
                            Purpose::Workspace {
                                ws,
                                win,
                                folder: w.path.clone(),
                                name: None,
                            },
                            Some(w.path.clone()),
                            sh.machine.clone(),
                            None,
                        );
                    }
                }
            }
            Outcome::Remove(i) => {
                let w = sh.list[i].clone();
                if w.main {
                    sh.view.at = i;
                    self.mode = Mode::Worktrees(sh);
                    self.notify("that's the repository's own checkout: it can't be removed".into());
                    return;
                }
                let name = w
                    .branch
                    .clone()
                    .unwrap_or_else(|| w.path.rsplit('/').next().unwrap_or("").to_string());
                let mut message = vec![(format!("Remove the worktree {name} at {}?", w.path), false)];
                if w.dirty {
                    message.push(("⚠ it has uncommitted changes: they'll be lost".into(), true));
                }
                if w.locked {
                    message.push(("⚠ it's locked".into(), true));
                }
                let open = self.workspace_at(&w.path, &sh.machine);
                if open.is_some() {
                    message.push(("its workspace closes too".into(), false));
                }
                self.mode = Mode::Dialog(dialogs::Dialog {
                    does: dialogs::Does::RemoveWorktree {
                        folder: sh.folder.clone(),
                        path: w.path,
                        force: w.dirty || w.locked,
                        machine: sh.machine.clone(),
                        workspace: open,
                    },
                    form: crate::form::Form::confirm("Remove Worktree", "remove", message),
                    busy: false,
                });
            }
        }
    }

    /// Yes, remove it: close its workspace, then `git worktree remove`.
    pub(super) fn remove_worktree(
        &mut self,
        folder: &str,
        path: &str,
        force: bool,
        machine: Option<String>,
        workspace: Option<String>,
    ) {
        if let Some(ws) = workspace
            && let Some(s) = self.doc.workspace(&ws)
        {
            let panes: Vec<String> = s
                .windows
                .iter()
                .flat_map(|w| w.root.panes().into_iter().cloned())
                .collect();
            self.kill(panes);
        }
        let force = if force { " --force --force" } else { "" };
        let line = format!(
            "git -C {} worktree remove{force} {}",
            crate::worktree::quote(folder),
            crate::worktree::quote(path)
        );
        let (c, out, path) = (self.client.clone(), self.out.clone(), path.to_string());
        tokio::spawn(async move {
            let note = match exec(&c, &line, machine.as_deref()).await {
                Ok(_) => format!("removed the worktree at {path}"),
                Err(e) => format!("not removed: {e}"),
            };
            let _ = out.send(Msg::Note(note));
        });
    }
}

/// Run a shell line through core (`machine.exec`): what it printed, or why it failed (git's
/// last word on it).
pub(super) async fn exec(c: &Client, line: &str, machine: Option<&str>) -> Result<String, String> {
    exec_for(c, line, machine, 60).await
}

/// `exec`, given up on after `secs` (core ends what it started then).
pub(super) async fn exec_for(c: &Client, line: &str, machine: Option<&str>, secs: u64) -> Result<String, String> {
    let v = c
        .call("machine.exec", json!({"line": line, "on": machine, "timeout": secs}))
        .await
        .map_err(|e| e.to_string())?;
    let text = |k: &str| {
        let raw = crate::worktree::unbase64(v[k].as_str().unwrap_or_default()).unwrap_or_default();
        String::from_utf8_lossy(&raw).into_owned()
    };
    if v["code"].as_i64() == Some(0) {
        Ok(text("stdout"))
    } else {
        let err = text("stderr");
        let why = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("it failed");
        Err(why
            .trim_start_matches("fatal: ")
            .trim_start_matches("error: ")
            .to_string())
    }
}
