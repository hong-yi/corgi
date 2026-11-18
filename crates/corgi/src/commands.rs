//! corgi's commands, tmux style: what key bindings, `[[keys.bind]]` in the config, and
//! the `C-b :` prompt all run. Parsing is pure: text in, [`Action`] or an error out.

use crate::layout::{Dir, Preset, Side};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    NewWindow {
        name: Option<String>,
        cwd: Option<String>,
        cmd: Option<String>,
    },
    NextWindow,
    PrevWindow,
    LastWindow,
    SelectWindow(u8),
    /// `None`: ask for the name.
    RenameWindow(Option<String>),
    /// Name the focused pane (`None`: ask).
    RenamePane(Option<String>),
    KillWindow,
    SplitWindow {
        dir: Dir,
        pct: u8,
        cwd: Option<String>,
        cmd: Option<String>,
    },
    SelectPane(Side),
    NextPane,
    PrevPane,
    LastPane,
    Zoom,
    SwapPrev,
    SwapNext,
    /// Swap with the neighbour on that side.
    SwapDir(Side),
    BreakPane,
    KillPane,
    NextLayout,
    SelectLayout(Preset),
    Resize(Side, u16),
    NewWorkspace {
        cwd: Option<String>,
        /// `-w`: in a new git worktree of that folder, on a branch named with random words.
        worktree: bool,
    },
    RenameWorkspace(Option<String>),
    KillWorkspace,
    PrevWorkspace,
    NextWorkspace,
    /// The tree chooser, at the current window (`choose-tree`) or workspace (`-s`).
    ChooseTree {
        workspaces: bool,
    },
    /// Jump to the next agent that's blocked, or (`done`) that finished unseen.
    NextAgent {
        done: bool,
        /// Any agent (in the sidebar's order), not only those waiting or finished.
        any: bool,
        /// The one before instead.
        back: bool,
    },
    /// The agent at this place in the sidebar's list (1 is the first).
    SelectAgent(usize),
    Detach,
    SendPrefix,
    /// Key names (`Enter`, `C-c`) and literal text, as tmux's `send-keys`.
    /// Keys to type; `literal` (`-l`): the words as text, not key names.
    SendKeys(Vec<String>, bool),
    ToggleSidebar,
    /// Read the config again (from this file, which is then the one used).
    ReloadConfig(Option<String>),
    /// The `:` prompt; with a template, what's typed replaces `%%` in it.
    CommandPrompt {
        label: Option<String>,
        initial: Option<String>,
        template: Option<String>,
    },
    /// Ask y/n first.
    ConfirmBefore {
        prompt: Option<String>,
        action: Box<Action>,
    },
    ListKeys,
    DisplayPanes,
    /// Copy mode in the focused pane; `up`: a screen up at once (`copy-mode -u`).
    CopyMode {
        up: bool,
    },
    /// Paste what copy mode copied last.
    PasteBuffer,
    /// A terminal floating over everything, running `cmd` (none: the shell) in `cwd` (none: the
    /// focused pane's); `close`: it goes when the program exits (`-E`), else it waits for a key.
    Popup {
        cmd: Option<String>,
        cwd: Option<String>,
        width: Size,
        height: Size,
        title: Option<String>,
        close: bool,
    },
    /// Run a shell line in the background; its first line of output (or its error) is shown.
    RunShell(String),
    /// The focused pane's scrollback in `$EDITOR`, in a popup.
    EditScrollback,
    /// This machine's clipboard image, put beside the focused pane's terminal (on its
    /// machine), its path there pasted.
    PasteImage,
    /// The current workspace's repository's git worktrees, to open, make and remove.
    ChooseWorktree,
    /// Other machines: their lines, to add, connect, rename and remove.
    ChooseMachine,
    /// Do something to a machine (by name; else the current workspace's).
    Machine(MachineVerb, Option<String>),
    /// Use another key table until Esc (`switch-client -T resize`).
    SwitchTable(String),
    /// Open a dialog (`dialog new-window`).
    Dialog(DialogKind),
    /// What you can do to the focused pane, its window and workspace (`display-menu`).
    Menu,
    /// Everything the tab bar's usage segments know (`display-usage`).
    Usage,
}

/// A popup's width or height: a share of the screen, or cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    Percent(u8),
    Cells(u16),
}

impl Size {
    /// Cells, of `of` (the screen's width or height).
    pub fn of(self, of: u16) -> u16 {
        match self {
            Size::Percent(p) => (of as u32 * p as u32 / 100) as u16,
            Size::Cells(n) => n,
        }
    }
}

fn size(cmd: &str, v: &str) -> Result<Size, String> {
    match v.strip_suffix('%') {
        Some(p) => match p.parse::<u8>() {
            Ok(n @ 1..=100) => Ok(Size::Percent(n)),
            _ => Err(format!("{cmd}: {v:?} isn't a percentage (1-100%)")),
        },
        None => match v.parse::<u16>() {
            Ok(n @ 3..) => Ok(Size::Cells(n)),
            _ => Err(format!("{cmd}: {v:?} isn't a size (cells, or a percentage)")),
        },
    }
}

/// The dialogs a command can open: for now, all for making something new.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogKind {
    NewWorkspace,
    /// New Workspace with its worktree switch on, from the current workspace's folder.
    NewWorktree,
    NewWindow,
    /// Add Machine.
    NewMachine,
}

/// What `machine` does to one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineVerb {
    /// Dial it now instead of at its next retry.
    Connect,
    /// `hc machine login`, in a popup (ssh passwords and codes).
    Login,
    /// `hc machine update`, in a popup.
    Update,
    Rename,
    Remove,
    /// The New Workspace dialog, on it.
    NewWorkspace,
}

/// Every command, for completion and `list-keys`.
pub const NAMES: &[&str] = &[
    "break-pane",
    "choose-tree",
    "choose-machine",
    "choose-worktree",
    "command-prompt",
    "confirm-before",
    "copy-mode",
    "detach-client",
    "display-popup",
    "edit-scrollback",
    "dialog",
    "display-menu",
    "display-usage",
    "display-panes",
    "kill-pane",
    "kill-window",
    "kill-workspace",
    "last-pane",
    "last-window",
    "list-keys",
    "machine",
    "new-window",
    "new-workspace",
    "next-agent",
    "next-layout",
    "next-pane",
    "next-window",
    "next-workspace",
    "paste-buffer",
    "paste-image",
    "previous-agent",
    "previous-pane",
    "previous-window",
    "previous-workspace",
    "rename-pane",
    "run-shell",
    "rename-window",
    "rename-workspace",
    "resize-pane",
    "select-layout",
    "select-pane",
    "select-agent",
    "select-window",
    "send-keys",
    "send-prefix",
    "source-file",
    "split-window",
    "swap-pane",
    "switch-client",
    "toggle-sidebar",
];

/// Split a command line into words: spaces separate, quotes group, backslash escapes.
pub fn words(line: &str) -> Result<Vec<String>, String> {
    let (mut out, mut cur, mut any) = (Vec::new(), String::new(), false);
    let mut chars = line.chars();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"') | None, '\\') => cur.push(chars.next().ok_or("trailing backslash")?),
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                any = true;
            }
            (None, c) if c.is_whitespace() => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                any = false;
            }
            (None, c) => cur.push(c),
        }
    }
    if quote.is_some() {
        return Err("unterminated quote".into());
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    Ok(out)
}

/// Options and positional words, tmux style: `-h`, `-p 30`, `-c dir`, `--` ends options.
struct Args {
    flags: Vec<char>,
    values: Vec<(char, String)>,
    rest: Vec<String>,
}

impl Args {
    /// `takes`: the options that take a value.
    fn parse(cmd: &str, words: &[String], takes: &str, allowed: &str) -> Result<Args, String> {
        let mut a = Args {
            flags: Vec::new(),
            values: Vec::new(),
            rest: Vec::new(),
        };
        let mut it = words.iter().peekable();
        while let Some(w) = it.next() {
            if w == "--" {
                a.rest.extend(it.cloned());
                break;
            }
            match w.strip_prefix('-').filter(|f| !f.is_empty() && a.rest.is_empty()) {
                Some(f) => {
                    let mut cs = f.chars();
                    while let Some(c) = cs.next() {
                        if !allowed.contains(c) && !takes.contains(c) {
                            return Err(format!("{cmd}: unknown option -{c}"));
                        }
                        if takes.contains(c) {
                            let rest: String = cs.collect();
                            let v = if rest.is_empty() {
                                it.next().cloned().ok_or(format!("{cmd}: -{c} needs a value"))?
                            } else {
                                rest
                            };
                            a.values.push((c, v));
                            break;
                        }
                        a.flags.push(c);
                    }
                }
                None => a.rest.push(w.clone()),
            }
        }
        Ok(a)
    }

    fn has(&self, c: char) -> bool {
        self.flags.contains(&c)
    }

    fn value(&self, c: char) -> Option<String> {
        self.values.iter().rev().find(|(k, _)| *k == c).map(|(_, v)| v.clone())
    }

    fn side(&self) -> Option<Side> {
        [
            ('L', Side::Left),
            ('R', Side::Right),
            ('U', Side::Up),
            ('D', Side::Down),
        ]
        .into_iter()
        .find(|(c, _)| self.has(*c))
        .map(|(_, s)| s)
    }

    /// The words after the options, as typed (names, templates, commands).
    fn rest_line(&self) -> Option<String> {
        (!self.rest.is_empty()).then(|| self.rest.join(" "))
    }

    /// The words after the options as one shell line. A word that was quoted to keep it together
    /// (it has spaces or quotes in it) is quoted again; the rest go as they were typed, so the
    /// shell still expands them.
    fn rest_shell(&self) -> Option<String> {
        let quote = |w: &String| {
            if w.is_empty() || w.chars().any(|c| c.is_whitespace() || c == '\'' || c == '"') {
                format!("'{}'", w.replace('\'', "'\\''"))
            } else {
                w.clone()
            }
        };
        (!self.rest.is_empty()).then(|| self.rest.iter().map(quote).collect::<Vec<_>>().join(" "))
    }
}

/// `30`, `30%`: a split's share for the new pane.
fn percent(cmd: &str, v: &str) -> Result<u8, String> {
    let n: u8 = v
        .trim_end_matches('%')
        .parse()
        .map_err(|_| format!("{cmd}: {v:?} isn't a percentage"))?;
    if (1..=99).contains(&n) {
        Ok(n)
    } else {
        Err(format!("{cmd}: {n}% is out of range (1-99)"))
    }
}

pub fn parse(line: &str) -> Result<Action, String> {
    let w = words(line)?;
    let Some((cmd, args)) = w.split_first() else {
        return Err("empty command".into());
    };
    let cmd = cmd.as_str();
    let a = |takes: &str, allowed: &str| Args::parse(cmd, args, takes, allowed);
    Ok(match cmd {
        "new-window" | "neww" => {
            let a = a("nc", "d")?;
            Action::NewWindow {
                name: a.value('n'),
                cwd: a.value('c'),
                cmd: a.rest_shell(),
            }
        }
        "split-window" | "splitw" => {
            let a = a("plc", "hvdb")?;
            let dir = if a.has('h') { Dir::Horizontal } else { Dir::Vertical };
            let pct = match a.value('p').or(a.value('l')) {
                Some(v) => percent(cmd, &v)?,
                None => 50,
            };
            Action::SplitWindow {
                dir,
                pct,
                cwd: a.value('c'),
                cmd: a.rest_shell(),
            }
        }
        "next-window" | "next" => Action::NextWindow,
        "previous-window" | "prev" => Action::PrevWindow,
        "last-window" | "last" => Action::LastWindow,
        "select-window" | "selectw" => {
            let a = a("t", "")?;
            let t = a
                .value('t')
                .or(a.rest.first().cloned())
                .ok_or("select-window: which? -t <index>")?;
            let t = t.trim_start_matches(':');
            Action::SelectWindow(t.parse().map_err(|_| format!("select-window: {t:?} isn't an index"))?)
        }
        "rename-window" | "renamew" => Action::RenameWindow(a("", "")?.rest_line()),
        "kill-window" | "killw" => Action::KillWindow,
        "select-pane" | "selectp" => {
            let a = a("", "LRUDl")?;
            if a.has('l') {
                Action::LastPane
            } else {
                Action::SelectPane(a.side().ok_or("select-pane: which way? -L -R -U -D (or -l)")?)
            }
        }
        "next-pane" => Action::NextPane,
        "previous-pane" => Action::PrevPane,
        "last-pane" | "lastp" => Action::LastPane,
        "resize-pane" | "resizep" => {
            let a = a("", "LRUDZ")?;
            if a.has('Z') {
                Action::Zoom
            } else {
                let side = a.side().ok_or("resize-pane: which way? -L -R -U -D (or -Z to zoom)")?;
                let n = match a.rest.first() {
                    Some(n) => n.parse().map_err(|_| format!("resize-pane: {n:?} isn't a number"))?,
                    None => 1,
                };
                Action::Resize(side, n)
            }
        }
        "swap-pane" | "swapp" => {
            let a = a("", "UDLR")?;
            // -U/-D: previous/next in order (tmux); -L/-R and with -s: by direction (herdr's shift+hjkl).
            match (a.has('U'), a.has('D'), a.has('L'), a.has('R')) {
                (true, ..) => Action::SwapPrev,
                (_, true, ..) => Action::SwapNext,
                (_, _, true, _) => Action::SwapDir(Side::Left),
                (.., true) => Action::SwapDir(Side::Right),
                _ => match a.rest.first().map(String::as_str) {
                    Some("up") => Action::SwapDir(Side::Up),
                    Some("down") => Action::SwapDir(Side::Down),
                    Some("left") => Action::SwapDir(Side::Left),
                    Some("right") => Action::SwapDir(Side::Right),
                    _ => Action::SwapNext,
                },
            }
        }
        "break-pane" | "breakp" => Action::BreakPane,
        "kill-pane" | "killp" => Action::KillPane,
        "next-layout" | "nextl" => Action::NextLayout,
        "select-layout" | "selectl" => {
            let a = a("", "")?;
            let name = a
                .rest
                .first()
                .ok_or("select-layout: which? even-horizontal, even-vertical, main-horizontal, main-vertical, tiled")?;
            let p = Preset::ALL
                .into_iter()
                .find(|p| p.name() == name)
                .ok_or(format!("select-layout: no layout {name:?}"))?;
            Action::SelectLayout(p)
        }
        "new-workspace" | "new-session" | "new" => {
            let a = a("c", "dAw")?;
            Action::NewWorkspace {
                cwd: a.value('c'),
                worktree: a.has('w'),
            }
        }
        "rename-workspace" | "rename-session" | "rename" => Action::RenameWorkspace(a("", "")?.rest_line()),
        "rename-pane" => Action::RenamePane(a("", "")?.rest_line()),
        "kill-workspace" | "kill-session" => Action::KillWorkspace,
        "next-workspace" => Action::NextWorkspace,
        "previous-workspace" => Action::PrevWorkspace,
        "choose-tree" => Action::ChooseTree {
            workspaces: a("", "sw")?.has('s'),
        },
        "next-agent" => {
            let a = a("", "bdap")?;
            Action::NextAgent {
                done: a.has('d'),
                any: a.has('a'),
                // `-b` is the waiting ones (blocked), as it always was: back is only `-p`.
                back: a.has('p'),
            }
        }
        "previous-agent" => {
            let a = a("", "da")?;
            Action::NextAgent {
                done: a.has('d'),
                any: a.has('a'),
                back: true,
            }
        }
        "select-agent" => match a("", "")?.rest.first().and_then(|n| n.parse::<usize>().ok()) {
            Some(n) if n > 0 => Action::SelectAgent(n),
            _ => return Err("select-agent: which? 1 is the first".into()),
        },
        "detach-client" | "detach" => Action::Detach,
        "send-prefix" => Action::SendPrefix,
        "send-keys" | "send" => {
            let a = a("", "l")?;
            if a.rest.is_empty() {
                return Err("send-keys: which keys?".into());
            }
            Action::SendKeys(a.rest.clone(), a.has('l'))
        }
        "toggle-sidebar" => Action::ToggleSidebar,
        "source-file" | "source" | "reload-config" => Action::ReloadConfig(a("", "")?.rest.first().cloned()),
        "command-prompt" => {
            let a = a("pI", "")?;
            Action::CommandPrompt {
                label: a.value('p'),
                initial: a.value('I'),
                template: a.rest_line(),
            }
        }
        "confirm-before" | "confirm" => {
            let a = a("p", "")?;
            let inner = a.rest_line().ok_or("confirm-before: which command?")?;
            Action::ConfirmBefore {
                prompt: a.value('p'),
                action: Box::new(parse(&inner)?),
            }
        }
        "list-keys" | "lsk" => Action::ListKeys,
        "display-panes" | "displayp" => Action::DisplayPanes,
        "copy-mode" => Action::CopyMode {
            up: a("", "u")?.has('u'),
        },
        "paste-buffer" | "pasteb" => Action::PasteBuffer,
        "display-popup" | "popup" => {
            let a = a("whdT", "E")?;
            Action::Popup {
                width: a
                    .value('w')
                    .map(|v| size(cmd, &v))
                    .transpose()?
                    .unwrap_or(Size::Percent(80)),
                height: a
                    .value('h')
                    .map(|v| size(cmd, &v))
                    .transpose()?
                    .unwrap_or(Size::Percent(80)),
                cwd: a.value('d'),
                title: a.value('T'),
                close: a.has('E'),
                cmd: a.rest_shell(),
            }
        }
        "run-shell" | "run" => match a("", "")?.rest_shell() {
            Some(line) => Action::RunShell(line),
            None => return Err("run-shell: what to run?".into()),
        },
        "edit-scrollback" => Action::EditScrollback,
        "paste-image" => Action::PasteImage,
        "choose-worktree" | "worktrees" => Action::ChooseWorktree,
        "choose-machine" | "machines" => Action::ChooseMachine,
        "machine" => {
            let a = a("", "")?;
            let verb = match a.rest.first().map(String::as_str) {
                Some("connect") => MachineVerb::Connect,
                Some("login") => MachineVerb::Login,
                Some("update") => MachineVerb::Update,
                Some("rename") => MachineVerb::Rename,
                Some("remove" | "rm") => MachineVerb::Remove,
                Some("new-workspace" | "workspace") => MachineVerb::NewWorkspace,
                Some("add") => return Ok(Action::Dialog(DialogKind::NewMachine)),
                _ => return Err("machine: add, connect, login, update, rename, remove or new-workspace?".into()),
            };
            Action::Machine(verb, a.rest.get(1).cloned())
        }
        "display-menu" | "menu" | "menu-display" => Action::Menu,
        "display-usage" | "usage" => Action::Usage,
        "dialog" => {
            let a = a("", "")?;
            match a.rest.first().map(String::as_str) {
                Some("new-workspace" | "new-session" | "workspace") => Action::Dialog(DialogKind::NewWorkspace),
                Some("new-window" | "window") => Action::Dialog(DialogKind::NewWindow),
                Some("new-worktree" | "worktree") => Action::Dialog(DialogKind::NewWorktree),
                Some("new-machine" | "machine") => Action::Dialog(DialogKind::NewMachine),
                Some(other) => {
                    return Err(format!(
                        "dialog: no {other:?} dialog (new-workspace, new-worktree, new-window, new-machine)"
                    ));
                }
                None => return Err("dialog: which one? new-workspace or new-window".into()),
            }
        }
        "switch-client" | "switchc" => {
            let a = a("T", "np")?;
            match (a.value('T'), a.has('n'), a.has('p')) {
                (Some(t), ..) => Action::SwitchTable(t),
                (None, true, _) => Action::NextWorkspace,
                (None, _, true) => Action::PrevWorkspace,
                _ => return Err("switch-client: -T <table>, -n or -p".into()),
            }
        }
        other => {
            let mut near: Vec<(usize, &str)> = NAMES
                .iter()
                .map(|n| (distance(other, n), *n))
                .filter(|(d, _)| *d <= 3)
                .collect();
            near.sort();
            let hint = match near.first() {
                Some((_, n)) => format!(" (did you mean {n}?)"),
                None => String::new(),
            };
            return Err(format!("unknown command {other:?}{hint}"));
        }
    })
}

/// Edit distance, for "did you mean".
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + (ca != *cb) as usize).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

/// What `Tab` completes the first word to.
pub fn complete(prefix: &str) -> Vec<&'static str> {
    NAMES.iter().copied().filter(|n| n.starts_with(prefix)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_keep_their_quoting() {
        let Action::Popup { cmd, .. } = parse("display-popup -E sh -c 'echo hi; read x'").unwrap() else {
            panic!()
        };
        assert_eq!(cmd.as_deref(), Some("sh -c 'echo hi; read x'"));
        assert_eq!(
            parse("run-shell echo $((2+3))").unwrap(),
            Action::RunShell("echo $((2+3))".into()),
            "as typed"
        );
        assert_eq!(
            parse(r#"run-shell "it's""#).unwrap(),
            Action::RunShell("'it'\\''s'".into())
        );
        let Action::SplitWindow { cmd, .. } = parse("split-window -h vim 'my file'").unwrap() else {
            panic!()
        };
        assert_eq!(cmd.as_deref(), Some("vim 'my file'"));
    }

    #[test]
    fn popups_and_shells() {
        assert_eq!(
            parse("display-popup -E -w 60% -h 20 -T git lazygit --all").unwrap(),
            Action::Popup {
                cmd: Some("lazygit --all".into()),
                cwd: None,
                width: Size::Percent(60),
                height: Size::Cells(20),
                title: Some("git".into()),
                close: true,
            }
        );
        let Action::Popup { cmd, width, close, .. } = parse("popup").unwrap() else {
            panic!()
        };
        assert_eq!(
            (cmd, width, close),
            (None, Size::Percent(80), false),
            "the shell, 80%, stays"
        );
        assert!(parse("display-popup -w 150% x").unwrap_err().contains("percentage"));
        assert_eq!(
            parse("run-shell make test").unwrap(),
            Action::RunShell("make test".into())
        );
        assert!(parse("run-shell").is_err());
        assert_eq!(Size::Percent(50).of(81), 40);
    }

    #[test]
    fn copy_and_paste() {
        assert_eq!(parse("copy-mode").unwrap(), Action::CopyMode { up: false });
        assert_eq!(parse("copy-mode -u").unwrap(), Action::CopyMode { up: true });
        assert_eq!(parse("paste-buffer").unwrap(), Action::PasteBuffer);
    }

    #[test]
    fn rename_pane() {
        assert_eq!(
            parse("rename-pane build").unwrap(),
            Action::RenamePane(Some("build".into()))
        );
        assert_eq!(parse("rename-pane").unwrap(), Action::RenamePane(None));
    }

    #[test]
    fn dialogs() {
        assert_eq!(
            parse("dialog new-window").unwrap(),
            Action::Dialog(DialogKind::NewWindow)
        );
        assert_eq!(
            parse("dialog new-workspace").unwrap(),
            Action::Dialog(DialogKind::NewWorkspace)
        );
        assert!(
            parse("dialog settings")
                .unwrap_err()
                .contains("new-workspace, new-worktree, new-window, new-machine")
        );
        assert_eq!(
            parse("dialog new-machine").unwrap(),
            Action::Dialog(DialogKind::NewMachine)
        );
        assert_eq!(parse("machine add").unwrap(), Action::Dialog(DialogKind::NewMachine));
        assert_eq!(
            parse("machine connect vps").unwrap(),
            Action::Machine(MachineVerb::Connect, Some("vps".into()))
        );
        assert_eq!(parse("machine rm").unwrap(), Action::Machine(MachineVerb::Remove, None));
        assert!(parse("machine").is_err());
        assert_eq!(parse("choose-machine").unwrap(), Action::ChooseMachine);
        assert_eq!(parse("paste-image").unwrap(), Action::PasteImage);
        assert_eq!(
            parse("send-keys -l 'C-c' Enter").unwrap(),
            Action::SendKeys(vec!["C-c".into(), "Enter".into()], true)
        );
        assert_eq!(parse("source-file").unwrap(), Action::ReloadConfig(None));
        assert_eq!(
            parse("source-file ~/other.toml").unwrap(),
            Action::ReloadConfig(Some("~/other.toml".into()))
        );
        assert_eq!(
            parse("next-agent -a -p").unwrap(),
            Action::NextAgent {
                done: false,
                any: true,
                back: true
            }
        );
        assert_eq!(
            parse("previous-agent -a").unwrap(),
            Action::NextAgent {
                done: false,
                any: true,
                back: true
            }
        );
        assert_eq!(parse("select-agent 3").unwrap(), Action::SelectAgent(3));
        assert_eq!(
            parse("new-workspace -w").unwrap(),
            Action::NewWorkspace {
                cwd: None,
                worktree: true
            }
        );
        assert_eq!(
            parse("new -c ~/src").unwrap(),
            Action::NewWorkspace {
                cwd: Some("~/src".into()),
                worktree: false
            }
        );
        assert!(parse("select-agent 0").is_err());
        assert_eq!(
            parse("next-agent -b").unwrap(),
            Action::NextAgent {
                done: false,
                any: false,
                back: false
            },
            "-b: the waiting ones, forwards"
        );
        assert!(parse("dialog").is_err());
        assert_eq!(
            parse("dialog new-worktree").unwrap(),
            Action::Dialog(DialogKind::NewWorktree)
        );
        assert_eq!(parse("display-menu").unwrap(), Action::Menu);
    }

    #[test]
    fn words_split_like_a_shell() {
        assert_eq!(
            words(r#"new-window -n "my tests" -c '/tmp/a b' cargo test"#).unwrap(),
            ["new-window", "-n", "my tests", "-c", "/tmp/a b", "cargo", "test"]
        );
        assert_eq!(
            words(r#"send-keys "say \"hi\"" Enter"#).unwrap(),
            ["send-keys", r#"say "hi""#, "Enter"]
        );
        assert_eq!(words("rename-window ''").unwrap(), ["rename-window", ""]);
        assert!(words("split-window 'oops").is_err());
    }

    #[test]
    fn tmux_commands_parse() {
        assert_eq!(
            parse("split-window -h").unwrap(),
            Action::SplitWindow {
                dir: Dir::Horizontal,
                pct: 50,
                cwd: None,
                cmd: None
            }
        );
        assert_eq!(
            parse("splitw -v -p 30 -c /src htop").unwrap(),
            Action::SplitWindow {
                dir: Dir::Vertical,
                pct: 30,
                cwd: Some("/src".into()),
                cmd: Some("htop".into())
            }
        );
        assert_eq!(
            parse("split-window -l 25%").unwrap(),
            Action::SplitWindow {
                dir: Dir::Vertical,
                pct: 25,
                cwd: None,
                cmd: None
            }
        );
        assert_eq!(
            parse("new-window -n logs tail -f x.log").unwrap(),
            Action::NewWindow {
                name: Some("logs".into()),
                cwd: None,
                cmd: Some("tail -f x.log".into())
            }
        );
        assert_eq!(parse("select-pane -L").unwrap(), Action::SelectPane(Side::Left));
        assert_eq!(parse("select-pane -l").unwrap(), Action::LastPane);
        assert_eq!(parse("resize-pane -R 5").unwrap(), Action::Resize(Side::Right, 5));
        assert_eq!(parse("resize-pane -Z").unwrap(), Action::Zoom);
        assert_eq!(parse("select-window -t :3").unwrap(), Action::SelectWindow(3));
        assert_eq!(
            parse("select-layout tiled").unwrap(),
            Action::SelectLayout(Preset::Tiled)
        );
        assert_eq!(parse("swap-pane -U").unwrap(), Action::SwapPrev);
        assert_eq!(parse("swap-pane left").unwrap(), Action::SwapDir(Side::Left));
        assert_eq!(
            parse("rename-window my editor").unwrap(),
            Action::RenameWindow(Some("my editor".into()))
        );
        assert_eq!(parse("rename-window").unwrap(), Action::RenameWindow(None));
        assert_eq!(
            parse("switch-client -T resize").unwrap(),
            Action::SwitchTable("resize".into())
        );
        assert_eq!(parse("switch-client -n").unwrap(), Action::NextWorkspace);
        assert_eq!(
            parse("choose-tree -s").unwrap(),
            Action::ChooseTree { workspaces: true }
        );
        assert_eq!(
            parse("send-keys 'ls -la' Enter").unwrap(),
            Action::SendKeys(vec!["ls -la".into(), "Enter".into()], false)
        );
        assert_eq!(
            parse(r#"confirm-before -p "kill-pane #P? (y/n)" kill-pane"#).unwrap(),
            Action::ConfirmBefore {
                prompt: Some("kill-pane #P? (y/n)".into()),
                action: Box::new(Action::KillPane)
            }
        );
        assert_eq!(
            parse(r#"command-prompt -p index "select-window -t ':%%'""#).unwrap(),
            Action::CommandPrompt {
                label: Some("index".into()),
                initial: None,
                template: Some("select-window -t ':%%'".into())
            }
        );
    }

    #[test]
    fn mistakes_say_what_was_wrong() {
        assert!(parse("").unwrap_err().contains("empty"));
        assert!(parse("split-window -x").unwrap_err().contains("unknown option -x"));
        assert!(parse("split-window -p 150").unwrap_err().contains("out of range"));
        assert!(parse("select-pane").unwrap_err().contains("which way"));
        assert!(parse("select-layout spiral").unwrap_err().contains("no layout"));
        assert!(parse("resize-pane -L lots").unwrap_err().contains("isn't a number"));
        let e = parse("splt-window -h").unwrap_err();
        assert!(e.contains("unknown command") && e.contains("split-window"), "{e}");
        assert!(
            parse("confirm-before -p sure? frobnicate")
                .unwrap_err()
                .contains("frobnicate")
        );
    }

    #[test]
    fn completion() {
        assert_eq!(complete("split"), ["split-window"]);
        assert!(complete("next-").len() >= 4);
        assert!(complete("zzz").is_empty());
    }
}
