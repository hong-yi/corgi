//! Keys: tmux notation, key tables, and the keymap built from a preset plus the
//! config's `[[keys.bind]]` and `unbind`. One keymap drives both what a key does
//! and what the key-hints dialog and `list-keys` show, so they can't disagree.
//!
//! Tables: `prefix` (after the prefix key), `root` (no prefix: corgi sees the key before
//! the pane does), `resize`, and any other a binding names (`switch-client -T name`).

use crate::commands::{self, Action};
use crate::config::{KeyPreset, Keys};
use crate::keyhints::{Group, Hint};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::BTreeMap;
use std::time::Duration;

/// A key in tmux notation: `c`, `%`, `C-b`, `M-Up`, `F2`, `Space`, `BTab`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
}

impl Key {
    pub fn parse(s: &str) -> Option<Key> {
        let (mut ctrl, mut alt, mut rest) = (false, false, s);
        loop {
            if let Some(r) = rest.strip_prefix("C-").filter(|r| !r.is_empty()) {
                ctrl = true;
                rest = r;
            } else if let Some(r) = rest.strip_prefix("M-").filter(|r| !r.is_empty()) {
                alt = true;
                rest = r;
            } else {
                break;
            }
        }
        let code = match rest {
            "Up" => KeyCode::Up,
            "Down" => KeyCode::Down,
            "Left" => KeyCode::Left,
            "Right" => KeyCode::Right,
            "Enter" => KeyCode::Enter,
            "Tab" => KeyCode::Tab,
            "BTab" | "S-Tab" => KeyCode::BackTab,
            "Space" => KeyCode::Char(' '),
            "Escape" | "Esc" => KeyCode::Esc,
            "BSpace" => KeyCode::Backspace,
            "Home" => KeyCode::Home,
            "End" => KeyCode::End,
            // tmux's names, and the plain ones.
            "PPage" | "PageUp" | "PgUp" => KeyCode::PageUp,
            "NPage" | "PageDown" | "PgDn" => KeyCode::PageDown,
            "DC" | "Delete" => KeyCode::Delete,
            "IC" | "Insert" => KeyCode::Insert,
            f if f.len() > 1 && f.starts_with('F') => KeyCode::F(f[1..].parse().ok()?),
            c if c.chars().count() == 1 => KeyCode::Char(c.chars().next()?),
            _ => return None,
        };
        Some(Key { code, ctrl, alt })
    }

    /// Characters carry their shift already (`%`, `N`), so Shift is ignored for them.
    pub fn matches(&self, k: &KeyEvent) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        let code = match (k.code, self.code) {
            // Some terminals report C-b as uppercase with Shift held; compare letters case-folded under Ctrl.
            (KeyCode::Char(a), KeyCode::Char(b)) if ctrl => a.eq_ignore_ascii_case(&b),
            (a, b) => a == b,
        };
        code && ctrl == self.ctrl && alt == self.alt
    }

    /// As a key event (for sending it to a pane).
    pub fn event(&self) -> KeyEvent {
        let mut m = KeyModifiers::NONE;
        if self.ctrl {
            m |= KeyModifiers::CONTROL;
        }
        if self.alt {
            m |= KeyModifiers::ALT;
        }
        KeyEvent::new(self.code, m)
    }
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub key: Key,
    /// As written (`C-Left`), for `list-keys`.
    pub key_text: String,
    pub action: Action,
    pub command: String,
    /// tmux `bind -r`: in the prefix table, more presses within the repeat time need no
    /// prefix; in other tables, the table stays in use after it runs.
    pub repeat: bool,
    /// (group, key as shown, label) in the key-hints dialog; `None` when another entry covers it.
    pub hint: Option<(String, String, String)>,
}

pub struct Keymap {
    pub tables: BTreeMap<String, Vec<Binding>>,
    pub prefix: Key,
    pub repeat_time: Duration,
}

type Row = (
    &'static str,
    &'static str,
    &'static str,
    bool,
    Option<(&'static str, &'static str, &'static str)>,
);

/// The tmux preset: tmux's defaults where corgi has the command.
fn tmux_rows() -> Vec<Row> {
    let (w, p, s, a, o) = ("Windows", "Panes", "Workspaces", "Agents", "Other");
    let mut r: Vec<Row> = vec![
        ("prefix", "c", "new-window", false, Some((w, "c", "new window"))),
        ("prefix", "C", "dialog new-window", false, Some((w, "C", "new window…"))),
        ("prefix", "n", "next-window", false, Some((w, "n", "next window"))),
        ("prefix", "p", "previous-window", false, Some((w, "p", "prev window"))),
        // tmux has last-window on l; here l is a pane direction (h j k l), so it's on Tab.
        ("prefix", "Tab", "last-window", false, Some((w, "Tab", "last window"))),
        (
            "prefix",
            "'",
            "command-prompt -p index \"select-window -t ':%%'\"",
            false,
            Some((w, "'", "window by index")),
        ),
        ("prefix", ",", "rename-window", false, Some((w, ",", "rename window"))),
        ("prefix", "&", "kill-window", false, Some((w, "&", "kill window"))),
        ("prefix", "%", "split-window -h", false, Some((p, "%", "split right"))),
        ("prefix", "\"", "split-window -v", false, Some((p, "\"", "split down"))),
        (
            "prefix",
            "Up",
            "select-pane -U",
            true,
            Some((p, "h j k l", "select pane (or ←↑↓→)")),
        ),
        ("prefix", "Down", "select-pane -D", true, None),
        ("prefix", "Left", "select-pane -L", true, None),
        ("prefix", "Right", "select-pane -R", true, None),
        // vi directions too. Not repeatable: text typed straight after shouldn't move focus.
        ("prefix", "h", "select-pane -L", false, None),
        ("prefix", "j", "select-pane -D", false, None),
        ("prefix", "k", "select-pane -U", false, None),
        ("prefix", "l", "select-pane -R", false, None),
        ("prefix", "o", "next-pane", false, Some((p, "o", "next pane"))),
        ("prefix", ";", "last-pane", false, Some((p, ";", "last pane"))),
        ("prefix", "z", "resize-pane -Z", false, Some((p, "z", "zoom"))),
        ("prefix", "[", "copy-mode", false, Some((p, "[ ]", "copy mode / paste"))),
        ("prefix", "]", "paste-buffer", false, None),
        (
            "prefix",
            "C-v",
            "paste-image",
            false,
            Some((p, "C-v", "paste clipboard image")),
        ),
        ("prefix", "x", "kill-pane", false, Some((p, "x", "kill pane"))),
        (
            "prefix",
            "{",
            "swap-pane -U",
            false,
            Some((p, "{ }", "swap prev / next")),
        ),
        ("prefix", "}", "swap-pane -D", false, None),
        ("prefix", "!", "break-pane", false, Some((p, "!", "break to window"))),
        (
            "prefix",
            "Space",
            "next-layout",
            false,
            Some((p, "Space", "next layout")),
        ),
        (
            "prefix",
            "M-1",
            "select-layout even-horizontal",
            false,
            Some((p, "M-1..5", "select layout")),
        ),
        ("prefix", "M-2", "select-layout even-vertical", false, None),
        ("prefix", "M-3", "select-layout main-horizontal", false, None),
        ("prefix", "M-4", "select-layout main-vertical", false, None),
        ("prefix", "M-5", "select-layout tiled", false, None),
        ("prefix", "q", "display-panes", false, Some((p, "q", "pane numbers"))),
        ("prefix", "PageUp", "copy-mode -u", false, None),
        (
            "prefix",
            "N",
            "dialog new-workspace",
            false,
            Some((s, "N", "new workspace…")),
        ),
        ("prefix", "W", "choose-worktree", false, Some((s, "W", "worktrees"))),
        ("prefix", "M", "choose-machine", false, Some((s, "M", "machines"))),
        (
            "prefix",
            "$",
            "rename-workspace",
            false,
            Some((s, "$", "rename workspace")),
        ),
        (
            "prefix",
            "(",
            "switch-client -p",
            false,
            Some((s, "( )", "prev / next workspace")),
        ),
        ("prefix", ")", "switch-client -n", false, None),
        (
            "prefix",
            "w",
            "choose-tree",
            false,
            Some((s, "w", "choose from the tree")),
        ),
        (
            "prefix",
            "s",
            "choose-tree -s",
            false,
            Some((s, "s", "choose workspace")),
        ),
        (
            "prefix",
            "a",
            "next-agent -b",
            false,
            Some((a, "a", "next blocked agent")),
        ),
        (
            "prefix",
            "A",
            "next-agent -d",
            false,
            Some((a, "A", "next finished agent")),
        ),
        ("prefix", "d", "detach-client", false, Some((o, "d", "detach"))),
        ("prefix", ":", "command-prompt", false, Some((o, ":", "command"))),
        ("prefix", "?", "list-keys", false, Some((o, "?", "all keys"))),
        (
            "prefix",
            "e",
            "edit-scrollback",
            false,
            Some((o, "e", "scrollback in $EDITOR")),
        ),
        ("prefix", "m", "display-menu", false, Some((o, "m", "menu…"))),
        ("prefix", "u", "display-usage", false, Some((o, "u", "usage"))),
        ("prefix", "b", "toggle-sidebar", false, Some((o, "b", "toggle sidebar"))),
        ("prefix", "R", "source-file", false, Some((o, "R", "reload config"))),
    ];
    for i in 0..=9u8 {
        let hint = (i == 0).then_some((w, "0-9", "select window"));
        r.push((
            "prefix",
            leak(i.to_string()),
            leak(format!("select-window -t {i}")),
            false,
            hint,
        ));
    }
    for (k, d) in [("Up", "U"), ("Down", "D"), ("Left", "L"), ("Right", "R")] {
        let first = k == "Up";
        r.push((
            "prefix",
            leak(format!("C-{k}")),
            leak(format!("resize-pane -{d} 1")),
            true,
            first.then_some((p, "C-←↑↓→", "resize")),
        ));
        r.push((
            "prefix",
            leak(format!("M-{k}")),
            leak(format!("resize-pane -{d} 5")),
            true,
            first.then_some((p, "M-←↑↓→", "resize by 5")),
        ));
    }
    r.extend(resize_rows());
    r
}

/// The herdr preset: the old herdr TUI's defaults.
fn herdr_rows() -> Vec<Row> {
    let (w, p, s, o) = ("Windows", "Panes", "Workspaces", "Other");
    let mut r: Vec<Row> = vec![
        ("prefix", "c", "new-window", false, Some((w, "c", "new window"))),
        ("prefix", "C", "dialog new-window", false, Some((w, "C", "new window…"))),
        ("prefix", "n", "next-window", false, Some((w, "n", "next window"))),
        ("prefix", "p", "previous-window", false, Some((w, "p", "prev window"))),
        ("prefix", "T", "rename-window", false, Some((w, "T", "rename window"))),
        ("prefix", "X", "kill-window", false, Some((w, "X", "close window"))),
        ("prefix", "v", "split-window -h", false, Some((p, "v", "split right"))),
        ("prefix", "-", "split-window -v", false, Some((p, "-", "split down"))),
        (
            "prefix",
            "h",
            "select-pane -L",
            false,
            Some((p, "h j k l", "focus pane")),
        ),
        ("prefix", "j", "select-pane -D", false, None),
        ("prefix", "k", "select-pane -U", false, None),
        ("prefix", "l", "select-pane -R", false, None),
        (
            "prefix",
            "H",
            "swap-pane left",
            false,
            Some((p, "H J K L", "swap pane")),
        ),
        ("prefix", "J", "swap-pane down", false, None),
        ("prefix", "K", "swap-pane up", false, None),
        ("prefix", "L", "swap-pane right", false, None),
        ("prefix", "Tab", "next-pane", false, Some((p, "Tab", "next pane"))),
        ("prefix", "BTab", "previous-pane", false, None),
        ("prefix", "x", "kill-pane", false, Some((p, "x", "close pane"))),
        ("prefix", "z", "resize-pane -Z", false, Some((p, "z", "zoom"))),
        ("prefix", "[", "copy-mode", false, Some((p, "[ ]", "copy mode / paste"))),
        ("prefix", "]", "paste-buffer", false, None),
        (
            "prefix",
            "C-v",
            "paste-image",
            false,
            Some((p, "C-v", "paste clipboard image")),
        ),
        (
            "prefix",
            "r",
            "switch-client -T resize",
            false,
            Some((p, "r", "resize mode")),
        ),
        (
            "prefix",
            "N",
            "dialog new-workspace",
            false,
            Some((s, "N", "new workspace…")),
        ),
        ("prefix", "M", "choose-machine", false, Some((s, "M", "machines"))),
        (
            "prefix",
            "W",
            "rename-workspace",
            false,
            Some((s, "W", "rename workspace")),
        ),
        (
            "prefix",
            "D",
            "kill-workspace",
            false,
            Some((s, "D", "close workspace")),
        ),
        ("prefix", "w", "choose-tree -s", false, Some((s, "w", "workspaces"))),
        ("prefix", "g", "choose-tree", false, Some((s, "g", "go to"))),
        ("prefix", "q", "detach-client", false, Some((o, "q", "detach"))),
        ("prefix", ":", "command-prompt", false, Some((o, ":", "command"))),
        ("prefix", "?", "list-keys", false, Some((o, "?", "all keys"))),
        (
            "prefix",
            "e",
            "edit-scrollback",
            false,
            Some((o, "e", "scrollback in $EDITOR")),
        ),
        ("prefix", "m", "display-menu", false, Some((o, "m", "menu…"))),
        ("prefix", "u", "display-usage", false, Some((o, "u", "usage"))),
        ("prefix", "b", "toggle-sidebar", false, Some((o, "b", "toggle sidebar"))),
        ("prefix", "R", "source-file", false, Some((o, "R", "reload config"))),
    ];
    for i in 1..=9u8 {
        let hint = (i == 1).then_some((w, "1-9", "select window"));
        r.push((
            "prefix",
            leak(i.to_string()),
            leak(format!("select-window -t {}", i - 1)),
            false,
            hint,
        ));
    }
    r.extend(resize_rows());
    r
}

/// Nothing but a way back out: build your own with `[[keys.bind]]`.
fn none_rows() -> Vec<Row> {
    let o = "Other";
    vec![
        ("prefix", "d", "detach-client", false, Some((o, "d", "detach"))),
        ("prefix", ":", "command-prompt", false, Some((o, ":", "command"))),
        ("prefix", "?", "list-keys", false, Some((o, "?", "all keys"))),
        ("prefix", "m", "display-menu", false, Some((o, "m", "menu…"))),
        ("prefix", "u", "display-usage", false, Some((o, "u", "usage"))),
    ]
}

/// The resize table (`switch-client -T resize`): stays in use until Esc or Enter.
fn resize_rows() -> Vec<Row> {
    let r = "Resize";
    let mut rows = Vec::new();
    for (small, arrow, big, d) in [
        ("h", "Left", "H", "L"),
        ("j", "Down", "J", "D"),
        ("k", "Up", "K", "U"),
        ("l", "Right", "L", "R"),
    ] {
        let first = d == "L";
        rows.push((
            "resize",
            small,
            leak(format!("resize-pane -{d} 1")),
            true,
            first.then_some((r, "h j k l", "resize")),
        ));
        rows.push(("resize", arrow, leak(format!("resize-pane -{d} 1")), true, None));
        rows.push((
            "resize",
            big,
            leak(format!("resize-pane -{d} 5")),
            true,
            first.then_some((r, "H J K L", "resize by 5")),
        ));
    }
    rows
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

impl Keymap {
    /// The preset, then `unbind`, then `bind`. Errors name the binding that's wrong.
    pub fn build(cfg: &Keys) -> Result<Keymap, String> {
        let prefix = Key::parse(&cfg.prefix).ok_or(format!("keys.prefix {:?} isn't a key", cfg.prefix))?;
        let rows = match cfg.preset {
            KeyPreset::Tmux => tmux_rows(),
            KeyPreset::Herdr => herdr_rows(),
            KeyPreset::None => none_rows(),
        };
        let mut map = Keymap {
            tables: BTreeMap::new(),
            prefix,
            repeat_time: cfg.repeat_time,
        };
        for (table, key, command, repeat, hint) in rows {
            let hint = hint.map(|(g, k, l)| (g.to_string(), k.to_string(), l.to_string()));
            map.add(table, key, command, repeat, hint)
                .map_err(|e| format!("preset: {e}"))?;
        }
        // The prefix pressed twice sends it to the pane.
        let p = cfg.prefix.clone();
        map.add(
            "prefix",
            &p,
            "send-prefix",
            false,
            Some(("Other".into(), p.clone(), format!("send {p}"))),
        )?;
        for u in &cfg.unbind {
            let (table, key) = match u.split_once(' ') {
                Some((t, k)) => (t, k),
                None => ("prefix", u.as_str()),
            };
            let k = Key::parse(key).ok_or(format!("keys.unbind {u:?}: {key:?} isn't a key"))?;
            if let Some(bs) = map.tables.get_mut(table) {
                bs.retain(|b| b.key != k);
            }
        }
        for b in &cfg.bind {
            let label = b.note.clone().unwrap_or_else(|| b.command.clone());
            let group = b.group.clone().unwrap_or_else(|| "Custom".into());
            map.add(
                &b.table,
                &b.key,
                &b.command,
                b.repeat,
                Some((group, b.key.clone(), label)),
            )
            .map_err(|e| format!("keys.bind {:?}: {e}", b.key))?;
        }
        Ok(map)
    }

    /// Add (or replace) a binding.
    fn add(
        &mut self,
        table: &str,
        key: &str,
        command: &str,
        repeat: bool,
        hint: Option<(String, String, String)>,
    ) -> Result<(), String> {
        let k = Key::parse(key).ok_or(format!("{key:?} isn't a key"))?;
        let action = commands::parse(command)?;
        let bs = self.tables.entry(table.to_string()).or_default();
        bs.retain(|b| b.key != k);
        bs.push(Binding {
            key: k,
            key_text: key.to_string(),
            action,
            command: command.to_string(),
            repeat,
            hint,
        });
        Ok(())
    }

    pub fn lookup(&self, table: &str, k: &KeyEvent) -> Option<&Binding> {
        self.tables.get(table)?.iter().find(|b| b.key.matches(k))
    }

    /// Every binding, by table, for `list-keys`.
    pub fn all(&self) -> Vec<(&str, &Binding)> {
        self.tables
            .iter()
            .flat_map(|(t, bs)| bs.iter().map(move |b| (t.as_str(), b)))
            .collect()
    }

    /// The key-hints dialog's contents for a table.
    pub fn hints(&self, table: &str) -> Vec<Group> {
        let mut groups: Vec<Group> = Vec::new();
        for (group, key, label) in self
            .tables
            .get(table)
            .into_iter()
            .flatten()
            .filter_map(|b| b.hint.clone())
        {
            let hint = Hint { key, label };
            match groups.iter_mut().find(|g| g.title == group) {
                Some(g) => g.hints.push(hint),
                None => groups.push(Group {
                    title: group,
                    hints: vec![hint],
                }),
            }
        }
        // Custom bindings last; Esc closes.
        if let Some(i) = groups.iter().position(|g| g.title == "Custom") {
            let c = groups.remove(i);
            groups.push(c);
        }
        let esc = Hint {
            key: "Esc".into(),
            label: if table == "prefix" { "cancel" } else { "leave" }.into(),
        };
        match groups.iter_mut().find(|g| g.title == "Other") {
            Some(o) => o.hints.push(esc),
            None => groups.push(Group {
                title: "Other".into(),
                hints: vec![esc],
            }),
        }
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BindConfig;
    use crate::layout::Side;

    fn ev(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, m)
    }

    fn act(map: &Keymap, table: &str, code: KeyCode, m: KeyModifiers) -> Option<Action> {
        map.lookup(table, &ev(code, m)).map(|b| b.action.clone())
    }

    #[test]
    fn notation() {
        assert_eq!(
            Key::parse("C-b"),
            Some(Key {
                code: KeyCode::Char('b'),
                ctrl: true,
                alt: false
            })
        );
        assert_eq!(
            Key::parse("M-Up"),
            Some(Key {
                code: KeyCode::Up,
                ctrl: false,
                alt: true
            })
        );
        assert_eq!(
            Key::parse("C-M-x"),
            Some(Key {
                code: KeyCode::Char('x'),
                ctrl: true,
                alt: true
            })
        );
        assert_eq!(Key::parse("F12").unwrap().code, KeyCode::F(12));
        assert_eq!(Key::parse("BTab").unwrap().code, KeyCode::BackTab);
        assert_eq!(Key::parse("-").unwrap().code, KeyCode::Char('-'));
        assert_eq!(Key::parse("Nope"), None);
    }

    #[test]
    fn tmux_preset() {
        let map = Keymap::build(&Keys::default()).unwrap();
        for (c, side) in [
            ('h', Side::Left),
            ('j', Side::Down),
            ('k', Side::Up),
            ('l', Side::Right),
        ] {
            let b = map.lookup("prefix", &ev(KeyCode::Char(c), KeyModifiers::NONE)).unwrap();
            assert_eq!(b.action, Action::SelectPane(side), "{c}");
            assert!(!b.repeat, "{c} doesn't repeat");
        }
        assert_eq!(
            act(&map, "prefix", KeyCode::Tab, KeyModifiers::NONE),
            Some(Action::LastWindow)
        );
        assert!(matches!(
            act(&map, "prefix", KeyCode::Char('%'), KeyModifiers::SHIFT),
            Some(Action::SplitWindow { .. })
        ));
        assert_eq!(
            act(&map, "prefix", KeyCode::Char('3'), KeyModifiers::NONE),
            Some(Action::SelectWindow(3))
        );
        assert_eq!(
            act(&map, "prefix", KeyCode::Up, KeyModifiers::CONTROL),
            Some(Action::Resize(Side::Up, 1))
        );
        assert_eq!(
            act(&map, "prefix", KeyCode::Left, KeyModifiers::ALT),
            Some(Action::Resize(Side::Left, 5))
        );
        assert_eq!(
            act(
                &map,
                "prefix",
                KeyCode::Char('B'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            ),
            Some(Action::SendPrefix)
        );
        // Plain: whether it asks first is `ui.confirm_close`'s to say.
        assert_eq!(
            act(&map, "prefix", KeyCode::Char('x'), KeyModifiers::NONE),
            Some(Action::KillPane)
        );
        assert!(
            map.lookup("prefix", &ev(KeyCode::Up, KeyModifiers::NONE))
                .unwrap()
                .repeat
        );
        assert_eq!(
            act(&map, "resize", KeyCode::Char('h'), KeyModifiers::NONE),
            Some(Action::Resize(Side::Left, 1))
        );
        assert_eq!(
            act(&map, "resize", KeyCode::Char('L'), KeyModifiers::SHIFT),
            Some(Action::Resize(Side::Right, 5))
        );
        assert_eq!(act(&map, "prefix", KeyCode::Char('Q'), KeyModifiers::NONE), None);
        let g = map.hints("prefix");
        let titles: Vec<&str> = g.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, ["Windows", "Panes", "Workspaces", "Agents", "Other"]);
        assert_eq!(g[4].hints.last().unwrap().key, "Esc");
    }

    #[test]
    fn herdr_preset() {
        let map = Keymap::build(&Keys {
            preset: KeyPreset::Herdr,
            ..Keys::default()
        })
        .unwrap();
        let split = |c| act(&map, "prefix", KeyCode::Char(c), KeyModifiers::NONE);
        assert!(matches!(
            split('v'),
            Some(Action::SplitWindow {
                dir: crate::layout::Dir::Horizontal,
                ..
            })
        ));
        assert!(matches!(
            split('-'),
            Some(Action::SplitWindow {
                dir: crate::layout::Dir::Vertical,
                ..
            })
        ));
        assert_eq!(
            act(&map, "prefix", KeyCode::Char('h'), KeyModifiers::NONE),
            Some(Action::SelectPane(Side::Left))
        );
        assert_eq!(
            act(&map, "prefix", KeyCode::Char('L'), KeyModifiers::SHIFT),
            Some(Action::SwapDir(Side::Right))
        );
        assert_eq!(
            act(&map, "prefix", KeyCode::Char('1'), KeyModifiers::NONE),
            Some(Action::SelectWindow(0))
        );
        assert_eq!(
            act(&map, "prefix", KeyCode::Char('q'), KeyModifiers::NONE),
            Some(Action::Detach)
        );
        assert_eq!(
            act(&map, "prefix", KeyCode::Char('r'), KeyModifiers::NONE),
            Some(Action::SwitchTable("resize".into()))
        );
        assert_eq!(
            act(&map, "prefix", KeyCode::BackTab, KeyModifiers::SHIFT),
            Some(Action::PrevPane)
        );
    }

    #[test]
    fn config_binds_unbinds_and_errors() {
        let cfg = Keys {
            prefix: "C-a".into(),
            unbind: vec!["%".into(), "resize h".into()],
            bind: vec![
                BindConfig {
                    key: "|".into(),
                    command: "split-window -h".into(),
                    ..Default::default()
                },
                BindConfig {
                    key: "M-h".into(),
                    command: "select-pane -L".into(),
                    table: "root".into(),
                    ..Default::default()
                },
                BindConfig {
                    key: "c".into(),
                    command: "new-window -n scratch".into(),
                    note: Some("scratch window".into()),
                    ..Default::default()
                },
            ],
            ..Keys::default()
        };
        let map = Keymap::build(&cfg).unwrap();
        assert!(
            act(&map, "prefix", KeyCode::Char('%'), KeyModifiers::SHIFT).is_none(),
            "unbound"
        );
        assert!(act(&map, "resize", KeyCode::Char('h'), KeyModifiers::NONE).is_none());
        assert!(matches!(
            act(&map, "prefix", KeyCode::Char('|'), KeyModifiers::SHIFT),
            Some(Action::SplitWindow { .. })
        ));
        assert_eq!(
            act(&map, "root", KeyCode::Char('h'), KeyModifiers::ALT),
            Some(Action::SelectPane(Side::Left))
        );
        assert!(
            matches!(
                act(&map, "prefix", KeyCode::Char('c'), KeyModifiers::NONE),
                Some(Action::NewWindow { name: Some(_), .. })
            ),
            "replaced"
        );
        assert_eq!(
            act(&map, "prefix", KeyCode::Char('a'), KeyModifiers::CONTROL),
            Some(Action::SendPrefix),
            "C-a C-a"
        );
        let custom = map.hints("prefix").into_iter().find(|g| g.title == "Custom").unwrap();
        assert!(custom.hints.iter().any(|h| h.key == "c" && h.label == "scratch window"));

        let bad = Keys {
            bind: vec![BindConfig {
                key: "x".into(),
                command: "frobnicate".into(),
                ..Default::default()
            }],
            ..Keys::default()
        };
        let e = Keymap::build(&bad).err().unwrap();
        assert!(e.contains("keys.bind \"x\"") && e.contains("frobnicate"), "{e}");
        let bad = Keys {
            bind: vec![BindConfig {
                key: "Hyper-q".into(),
                command: "detach".into(),
                ..Default::default()
            }],
            ..Keys::default()
        };
        assert!(Keymap::build(&bad).err().unwrap().contains("isn't a key"));
    }
}
