//! Keys, by mode: the prefix and repeat keys, other key tables, the root table, dialogs,
//! prompts (rename, y/n, and the `:` command prompt with history and completion).

use super::*;
use crate::commands;

impl App {
    pub(super) fn key(&mut self, k: KeyEvent) {
        tracing::debug!(code = ?k.code, mods = ?k.modifiers, "key");
        match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Prompt(p) => self.prompt_key(p, k),
            Mode::Dialog(d) => self.dialog_key(d, k),
            Mode::Copy(c, pane) => self.copy_key(c, pane, k),
            Mode::Popup => self.popup_key(k),
            Mode::Worktrees(sh) => self.worktrees_key(sh, k),
            Mode::Machines(v) => self.machines_key(v, k),
            Mode::Menu(mut m, target) => match m.key(&k) {
                crate::menu::Outcome::Close => {}
                crate::menu::Outcome::Stay => self.mode = Mode::Menu(m, target),
                crate::menu::Outcome::Run(cmd) => {
                    if let Some(t) = target {
                        self.go(t);
                    }
                    self.run_command(&cmd);
                }
            },
            Mode::Chooser(mut ch) => {
                let rows = self.tree_rows(&ch.filter);
                let before = self.fold.clone();
                match ch.key(&k, &rows, &mut self.fold) {
                    crate::chooser::Outcome::Close => {}
                    crate::chooser::Outcome::Go(t) => self.go(t),
                    crate::chooser::Outcome::NewWorkspace(name) => self.new_workspace_dialog(name),
                    crate::chooser::Outcome::Stay => {
                        ch.settle(&self.tree_rows(&ch.filter));
                        self.mode = Mode::Chooser(ch);
                    }
                }
                if self.fold != before {
                    self.save_view();
                }
            }
            Mode::Keys(mut kl) => {
                let rows = self.key_rows();
                match kl.key(&k, &rows) {
                    crate::keylist::Outcome::Close => {}
                    crate::keylist::Outcome::Stay => self.mode = Mode::Keys(kl),
                    crate::keylist::Outcome::Run(cmd) => self.run_command(&cmd),
                }
            }
            Mode::Panes(until) => {
                // A digit picks that pane; any other key just puts the numbers away.
                if let KeyCode::Char(c) = k.code
                    && let Some(d) = c.to_digit(10)
                    && Instant::now() < until
                {
                    self.select_pane_index(d as usize);
                }
            }
            Mode::Prefix(_) => {
                if k.code == KeyCode::Esc {
                    return;
                }
                if let Some(b) = self.keymap.lookup("prefix", &k).cloned() {
                    if b.repeat {
                        self.mode = Mode::Repeat(Instant::now() + self.keymap.repeat_time);
                    }
                    self.run(b.action);
                }
            }
            Mode::Repeat(until) if Instant::now() < until => {
                if let Some(b) = self.keymap.lookup("prefix", &k).filter(|b| b.repeat).cloned() {
                    self.mode = Mode::Repeat(Instant::now() + self.keymap.repeat_time);
                    self.run(b.action);
                } else {
                    self.key(k);
                }
            }
            Mode::Table(name) => match k.code {
                KeyCode::Esc | KeyCode::Enter => {}
                _ if self.keymap.prefix.matches(&k) => self.mode = Mode::Prefix(Instant::now()),
                _ => {
                    // Repeatable bindings keep the table in use; others go back to normal keys.
                    if let Some(b) = self.keymap.lookup(&name, &k).cloned() {
                        if b.repeat {
                            self.mode = Mode::Table(name);
                        }
                        self.run(b.action);
                    }
                }
            },
            Mode::Normal | Mode::Repeat(_) => {
                if self.keymap.prefix.matches(&k) {
                    self.mode = Mode::Prefix(Instant::now());
                } else if let Some(b) = self.keymap.lookup("root", &k).cloned() {
                    self.run(b.action);
                } else if self.sidebar_focus.is_some() {
                    self.sidebar_key(k);
                } else if let Some(key) = input::to_vt(&k) {
                    self.type_key(&key);
                }
            }
        }
    }

    /// Every binding, for `list-keys`.
    pub(super) fn key_rows(&self) -> Vec<crate::keylist::Row> {
        self.keymap
            .all()
            .into_iter()
            .map(|(table, b)| crate::keylist::Row {
                table: table.to_string(),
                key: b.key_text.clone(),
                command: b.command.clone(),
                note: b.hint.as_ref().map(|(_, _, l)| l.clone()),
            })
            .collect()
    }

    /// Run a command line (the `:` prompt, list-keys' Enter). Mistakes show in the status line.
    pub(super) fn run_command(&mut self, line: &str) {
        match commands::parse(line) {
            Ok(a) => self.run(a),
            Err(e) => self.notify(e),
        }
    }

    fn prompt_key(&mut self, mut p: Prompt, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if let Then::Confirm(action) = p.then {
            if k.code == KeyCode::Char('y') {
                self.confirmed(*action);
            }
            return;
        }
        let command = matches!(p.then, Then::Command { .. });
        match k.code {
            KeyCode::Esc => return,
            KeyCode::Char('c' | 'g') if ctrl => return,
            KeyCode::Char('u') if ctrl => p.text.clear(),
            // History, newest first.
            KeyCode::Up if command && !self.history.is_empty() => {
                let i = p.hist.map_or(self.history.len() - 1, |i| i.saturating_sub(1));
                p.hist = Some(i);
                p.text = self.history[i].clone();
            }
            KeyCode::Down if command => match p.hist {
                Some(i) if i + 1 < self.history.len() => {
                    p.hist = Some(i + 1);
                    p.text = self.history[i + 1].clone();
                }
                _ => {
                    p.hist = None;
                    p.text.clear();
                }
            },
            // Complete the command's name.
            KeyCode::Tab if command && !p.text.contains(' ') => {
                let names = commands::complete(&p.text);
                match names.as_slice() {
                    [one] => p.text = format!("{one} "),
                    [] => {}
                    many => {
                        let common = many.iter().skip(1).fold(many[0].to_string(), |acc, n| {
                            acc.chars()
                                .zip(n.chars())
                                .take_while(|(a, b)| a == b)
                                .map(|(a, _)| a)
                                .collect()
                        });
                        if common.len() > p.text.len() {
                            p.text = common;
                        } else {
                            self.notify(many.join("  "));
                        }
                    }
                }
            }
            KeyCode::Enter => {
                let text = p.text.trim().to_string();
                match p.then {
                    Then::Command { template } => {
                        if text.is_empty() {
                            return;
                        }
                        let line = match template {
                            Some(t) => t.replace("%%", &text),
                            None => text.clone(),
                        };
                        if self.history.last() != Some(&text) {
                            self.history.push(text);
                        }
                        self.run_command(&line);
                    }
                    Then::Confirm(_) => {}
                }
                return;
            }
            KeyCode::Backspace => {
                p.text.pop();
            }
            KeyCode::Char(c) if !ctrl => p.text.push(c),
            _ => {}
        }
        self.mode = Mode::Prompt(p);
    }
}
