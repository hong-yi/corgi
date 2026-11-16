//! `corgi migrate`: a corgi config from the herdr TUI's (`~/.config/herdr/config.toml`).
//! The herdr key preset to start from; the keys you set there, as corgi bindings; the theme,
//! the sidebar, the tab bar, pane borders, toasts and worktrees where corgi has the same.
//! Whatever has no counterpart is listed, in the file and on the way out. Pure: TOML in,
//! TOML out.

use std::collections::BTreeMap;
use toml::Value;

/// The corgi config, and what wasn't carried over (and why).
pub struct Migrated {
    pub text: String,
    pub notes: Vec<String>,
}

/// One `[[keys.bind]]`.
struct Bind {
    key: String,
    table: &'static str,
    command: String,
    from: String,
}

/// herdr's key actions, as corgi commands. `{n}`: the index of a `1..9` range (from 1).
const ACTIONS: &[(&str, &str)] = &[
    ("help", "list-keys"),
    ("new_workspace", "dialog new-workspace"),
    ("new_worktree", "dialog new-worktree"),
    ("open_worktree", "choose-worktree"),
    ("remove_worktree", "choose-worktree"),
    ("rename_workspace", "rename-workspace"),
    ("close_workspace", "kill-workspace"),
    ("workspace_picker", "choose-tree -s"),
    ("goto", "choose-tree"),
    ("detach", "detach-client"),
    ("reload_config", "source-file"),
    ("previous_workspace", "switch-client -p"),
    ("next_workspace", "switch-client -n"),
    ("previous_agent", "previous-agent -a"),
    ("next_agent", "next-agent -a"),
    ("focus_agent", "select-agent {n}"),
    ("new_tab", "new-window"),
    ("rename_tab", "rename-window"),
    ("previous_tab", "previous-window"),
    ("next_tab", "next-window"),
    ("switch_tab", "select-window -t {n-1}"),
    ("close_tab", "kill-window"),
    ("rename_pane", "rename-pane"),
    ("edit_scrollback", "edit-scrollback"),
    ("clear_pane", "send-keys C-l"),
    ("copy_mode", "copy-mode"),
    ("focus_pane_left", "select-pane -L"),
    ("focus_pane_down", "select-pane -D"),
    ("focus_pane_up", "select-pane -U"),
    ("focus_pane_right", "select-pane -R"),
    ("swap_pane_left", "swap-pane -U"),
    ("swap_pane_up", "swap-pane -U"),
    ("swap_pane_down", "swap-pane -D"),
    ("swap_pane_right", "swap-pane -D"),
    ("cycle_pane_next", "next-pane"),
    ("cycle_pane_previous", "previous-pane"),
    ("last_pane", "last-pane"),
    ("split_vertical", "split-window -h"),
    ("split_horizontal", "split-window -v"),
    ("close_pane", "kill-pane"),
    ("zoom", "resize-pane -Z"),
    ("resize_mode", "switch-client -T resize"),
    ("resize_pane_left", "resize-pane -L 1"),
    ("resize_pane_down", "resize-pane -D 1"),
    ("resize_pane_up", "resize-pane -U 1"),
    ("resize_pane_right", "resize-pane -R 1"),
    ("toggle_sidebar", "toggle-sidebar"),
];

/// herdr's keys with no corgi counterpart yet, and what to say about them.
const NO_ACTION: &[(&str, &str)] = &[
    ("settings", "corgi has no settings dialog yet (the config file is it)"),
    ("open_notification_target", "click a toast instead, or C-b w"),
    ("move_tab_previous", "corgi doesn't reorder windows yet"),
    ("move_tab_next", "corgi doesn't reorder windows yet"),
    (
        "switch_workspace",
        "corgi has no workspace-by-number key yet (C-b w, or ( and ))",
    ),
    (
        "remote_image_paste",
        "corgi's is C-b C-v, and an empty paste into a pane on another machine",
    ),
];

/// corgi's built-in themes, by herdr's names for them.
fn theme(name: &str) -> Option<&'static str> {
    Some(match name {
        "catppuccin" | "catppuccin-mocha" => "catppuccin-mocha",
        "catppuccin-latte" | "latte" | "light" => "catppuccin-latte",
        "tokyo-night" | "tokyonight" => "tokyo-night",
        "gruvbox" | "gruvbox-dark" => "gruvbox-dark",
        "nord" => "nord",
        "dracula" => "dracula",
        "terminal" => "terminal",
        _ => return None,
    })
}

/// A herdr key (`prefix+shift+n`, `ctrl+alt+[`, `minus`) in tmux notation, and its table.
fn key(spec: &str) -> Option<(&'static str, String)> {
    let spec = spec.trim();
    // `+` alone (or ` + `) is the plus key.
    let parts: Vec<&str> = if spec == "+" {
        vec!["+"]
    } else {
        spec.split('+').map(str::trim).filter(|p| !p.is_empty()).collect()
    };
    let (last, mods) = parts.split_last()?;
    let (mut table, mut ctrl, mut alt, mut shift) = ("root", false, false, false);
    for m in mods {
        match m.to_ascii_lowercase().as_str() {
            "prefix" => table = "prefix",
            "ctrl" | "control" => ctrl = true,
            "alt" | "option" | "meta" => alt = true,
            "shift" => shift = true,
            _ => return None,
        }
    }
    let lower = last.to_ascii_lowercase();
    let name = match lower.as_str() {
        "space" => "Space".to_string(),
        "enter" | "return" => "Enter".into(),
        "esc" | "escape" => "Escape".into(),
        "tab" if shift => "BTab".into(),
        "tab" => "Tab".into(),
        "backspace" | "bs" => "BSpace".into(),
        "minus" => "-".into(),
        "plus" => "+".into(),
        "up" => "Up".into(),
        "down" => "Down".into(),
        "left" => "Left".into(),
        "right" => "Right".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        "pageup" | "page_up" | "pgup" => "PPage".into(),
        "pagedown" | "page_down" | "pgdn" => "NPage".into(),
        "delete" | "del" => "DC".into(),
        "insert" => "IC".into(),
        f if f.len() > 1 && f.starts_with('f') && f[1..].parse::<u8>().is_ok() => format!("F{}", &f[1..]),
        _ if last.chars().count() == 1 => {
            let c = last.chars().next()?;
            // Characters carry their shift: shift+n is N.
            if shift {
                c.to_ascii_uppercase().to_string()
            } else {
                c.to_string()
            }
        }
        _ => return None,
    };
    let mut out = String::new();
    if ctrl {
        out.push_str("C-");
    }
    if alt {
        out.push_str("M-");
    }
    out.push_str(&name);
    Some((table, out))
}

/// A binding's keys: a string or a list of them; `1..9` ranges spread out, with their numbers.
fn keys_of(v: &Value) -> Vec<(String, Option<u32>)> {
    let list: Vec<&str> = match v {
        Value::String(s) => vec![s.as_str()],
        Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for s in list {
        if s.trim().is_empty() {
            continue;
        }
        match s.rsplit_once('+').map(|(head, tail)| (head, tail.split_once(".."))) {
            Some((head, Some((a, b)))) if a.parse::<u32>().is_ok() && b.parse::<u32>().is_ok() => {
                let (a, b) = (a.parse::<u32>().unwrap(), b.parse::<u32>().unwrap());
                out.extend((a..=b).map(|n| (format!("{head}+{n}"), Some(n))));
            }
            _ => match s.split_once("..") {
                Some((a, b)) if a.parse::<u32>().is_ok() && b.parse::<u32>().is_ok() => {
                    let (a, b) = (a.parse::<u32>().unwrap(), b.parse::<u32>().unwrap());
                    out.extend((a..=b).map(|n| (n.to_string(), Some(n))));
                }
                _ => out.push((s.to_string(), None)),
            },
        }
    }
    out
}

type Sections = BTreeMap<&'static str, Vec<(String, String)>>;

fn put(out: &mut Sections, section: &'static str, k: &str, val: String) {
    out.entry(section).or_default().push((k.to_string(), val));
}

fn quote(s: &str) -> String {
    Value::String(s.to_string()).to_string()
}

/// herdr's config (TOML text) as corgi's. `from` names it in the file's header.
pub fn migrate(herdr: &str, from: &str) -> Result<Migrated, String> {
    let v: Value = herdr.parse::<Value>().map_err(|e| format!("{from}: {e}"))?;
    let mut notes: Vec<String> = Vec::new();
    // Section → (key, TOML value).
    let mut out: Sections = BTreeMap::new();
    let mut binds: Vec<Bind> = Vec::new();
    let get = |path: &[&str]| path.iter().try_fold(&v, |at, k| at.get(*k));

    // Keys: herdr's preset, the prefix, then what you set.
    put(&mut out, "keys", "preset", quote("herdr"));
    if let Some(keys) = v.get("keys").and_then(Value::as_table) {
        for (action, val) in keys {
            match action.as_str() {
                "prefix" => match keys_of(val).first().and_then(|(k, _)| key(k)) {
                    Some((_, k)) => put(&mut out, "keys", "prefix", quote(&k)),
                    None => notes.push(format!("keys.prefix {val}: not a key corgi knows")),
                },
                "command" => {
                    for c in val.as_array().into_iter().flatten() {
                        let k = c.get("key").and_then(Value::as_str).unwrap_or("?");
                        let cmd = c.get("command").and_then(Value::as_str).unwrap_or("?");
                        let what = match c.get("description").and_then(Value::as_str) {
                            Some(d) => format!("{d}: {cmd}"),
                            None => cmd.to_string(),
                        };
                        let kind = c.get("type").and_then(Value::as_str).unwrap_or("");
                        let why = if kind == "plugin_action" {
                            "plugins come later in corgi (M10)"
                        } else {
                            "no corgi counterpart; bind a corgi command with [[keys.bind]]"
                        };
                        notes.push(format!("keys.command {k} ({what}): {why}"));
                    }
                }
                "indexed" => notes.push("keys.indexed: not carried over; check C-b ? for corgi's".into()),
                a if a.starts_with("navigate_") => notes.push(format!(
                    "keys.{a}: the sidebar's keys are fixed in corgi (j k h l and the arrows)"
                )),
                a => {
                    if let Some((_, why)) = NO_ACTION.iter().find(|(n, _)| *n == a) {
                        notes.push(format!("keys.{a}: {why}"));
                        continue;
                    }
                    let Some((_, cmd)) = ACTIONS.iter().find(|(n, _)| *n == a) else {
                        notes.push(format!("keys.{a}: corgi doesn't know this herdr key"));
                        continue;
                    };
                    for (spec, n) in keys_of(val) {
                        let Some((table, k)) = key(&spec) else {
                            notes.push(format!("keys.{a} = {spec:?}: not a key corgi can bind"));
                            continue;
                        };
                        let command = match n {
                            Some(n) => cmd
                                .replace("{n-1}", &(n.saturating_sub(1)).to_string())
                                .replace("{n}", &n.to_string()),
                            None => cmd.to_string(),
                        };
                        binds.push(Bind {
                            key: k,
                            table,
                            command,
                            from: format!("keys.{a} = {spec:?}"),
                        });
                    }
                }
            }
        }
    }

    // The theme.
    if let Some(t) = v.get("theme").and_then(Value::as_table) {
        let auto = t.get("auto_switch").and_then(Value::as_bool).unwrap_or(false);
        let names: &[(&str, &str)] = if auto {
            put(&mut out, "theme", "name", quote("auto"));
            &[("dark_name", "dark"), ("light_name", "light")]
        } else {
            &[("name", "name")]
        };
        for (k, corgi_key) in names {
            let Some(n) = t.get(*k).and_then(Value::as_str) else {
                continue;
            };
            match theme(n) {
                Some(h) => put(&mut out, "theme", corgi_key, quote(h)),
                None => notes.push(format!(
                    "theme.{k} = {n:?}: not one of corgi's (catppuccin-mocha, catppuccin-latte, tokyo-night, gruvbox-dark, nord, dracula, terminal)"
                )),
            }
        }
        if let Some(custom) = t.get("custom").and_then(Value::as_table) {
            for (k, c) in custom {
                match (k.as_str(), c.as_str()) {
                    (
                        "accent" | "fg" | "muted" | "surface" | "border" | "working" | "blocked" | "done" | "idle",
                        Some(c),
                    ) => put(&mut out, "theme.custom", k, quote(c)),
                    _ => notes.push(format!("theme.custom.{k}: corgi's theme has no such colour")),
                }
            }
        }
    }

    // The UI.
    let ui = |k: &str| get(&["ui", k]);
    if let Some(Value::Integer(w)) = ui("sidebar_width") {
        put(&mut out, "sidebar", "width", w.to_string());
    }
    let collapsed = ui("sidebar_start_collapsed").and_then(Value::as_bool).unwrap_or(false);
    let mode = ui("sidebar_collapsed_mode")
        .and_then(Value::as_str)
        .unwrap_or("compact");
    if collapsed {
        put(
            &mut out,
            "sidebar",
            "mode",
            quote(if mode == "hidden" { "hidden" } else { "collapsed" }),
        );
    }
    if let Some(Value::Integer(n)) = ui("mobile_width_threshold") {
        put(&mut out, "ui", "narrow_width", n.to_string());
    }
    if let Some(Value::Boolean(b)) = ui("mouse_capture") {
        put(&mut out, "ui", "mouse", b.to_string());
    }
    if let Some(Value::Boolean(b)) = ui("confirm_close") {
        put(
            &mut out,
            "ui",
            "confirm_close",
            quote(if *b { "auto" } else { "never" }),
        );
    }
    if let Some(s) = ui("status_indicators").and_then(Value::as_str) {
        put(&mut out, "ui", "indicators", quote(s));
    }
    if let Some(s) = ui("window_title").and_then(Value::as_str)
        && s.is_empty()
    {
        put(&mut out, "ui", "window_title", "false".into());
    }
    if let Some(s) = ui("agent_panel_sort").and_then(Value::as_str) {
        put(
            &mut out,
            "sidebar",
            "agent_sort",
            quote(if s == "priority" { "priority" } else { "grouped" }),
        );
    }
    if let Some(s) = ui("tab_bar_position").and_then(Value::as_str) {
        put(&mut out, "tab_bar", "position", quote(s));
    }
    match ui("pane_borders") {
        Some(Value::Boolean(false)) => put(&mut out, "panes", "style", quote("plain")),
        Some(Value::String(s)) if s == "off" => put(&mut out, "panes", "style", quote("plain")),
        _ => {}
    }
    if let Some(Value::Boolean(b)) = ui("pane_gaps") {
        put(&mut out, "panes", "separate", b.to_string());
    }
    if let Some(Value::Boolean(true)) = ui("show_agent_labels_on_pane_borders") {
        put(&mut out, "panes", "title", quote("auto"));
    }
    if let Some(s) = ui("accent").and_then(Value::as_str)
        && !s.is_empty()
    {
        put(&mut out, "theme.custom", "accent", quote(s));
    }
    if let Some(rows) = get(&["ui", "sidebar", "agents", "rows"]).and_then(Value::as_array) {
        let mut tokens = Vec::new();
        for t in rows
            .iter()
            .flat_map(|r| r.as_array().into_iter().flatten())
            .filter_map(Value::as_str)
        {
            let h = match t {
                "state_icon" | "state" => "{state}",
                "machine" => "{machine}",
                "tab" => "{window}",
                "workspace" => "{workspace}",
                "agent" => "{agent}",
                "title" => "{title}",
                "branch" => "{branch}",
                other => {
                    notes.push(format!("ui.sidebar.agents.rows: {other:?} has no corgi token"));
                    continue;
                }
            };
            if !tokens.contains(&h) {
                tokens.push(h);
            }
        }
        if !tokens.is_empty() {
            put(&mut out, "sidebar", "agent_row", quote(&tokens.join(" ")));
            if rows.len() > 1 {
                notes.push("ui.sidebar.agents.rows: corgi's agent rows are one line: put on one".into());
            }
        }
    }
    match ui("toast").and_then(|t| t.get("delivery")).and_then(Value::as_str) {
        Some("off") => {
            put(&mut out, "notifications", "toasts", "false".into());
            put(&mut out, "notifications", "host", quote("off"));
        }
        Some("terminal" | "system") => {
            put(&mut out, "notifications", "toasts", "false".into());
            put(&mut out, "notifications", "host", quote("auto"));
        }
        Some("herdr") => put(&mut out, "notifications", "toasts", "true".into()),
        _ => {}
    }
    if ui("sound").and_then(|s| s.get("enabled")).and_then(Value::as_bool) == Some(true) {
        notes.push("ui.sound: corgi plays no sounds yet".into());
    }
    if let Some(d) = get(&["worktrees", "directory"]).and_then(Value::as_str) {
        put(&mut out, "worktrees", "directory", quote(d));
    }
    // Anything else of herdr's, by section: said, not carried.
    let known_ui = [
        "sidebar_width",
        "sidebar_start_collapsed",
        "sidebar_collapsed_mode",
        "mobile_width_threshold",
        "mouse_capture",
        "confirm_close",
        "status_indicators",
        "window_title",
        "agent_panel_sort",
        "tab_bar_position",
        "pane_borders",
        "pane_gaps",
        "show_agent_labels_on_pane_borders",
        "accent",
        "sidebar",
        "toast",
        "sound",
        // Asked for by corgi anyway (it never asks names first; borders are its own).
        "prompt_new_tab_name",
        "prompt_new_workspace_name",
        "pane_outer_borders",
    ];
    if let Some(t) = v.get("ui").and_then(Value::as_table) {
        for k in t.keys().filter(|k| !known_ui.contains(&k.as_str())) {
            notes.push(format!("ui.{k}: not carried over"));
        }
    }
    for k in v.as_table().into_iter().flat_map(|t| t.keys()) {
        if !["keys", "theme", "ui", "worktrees", "onboarding"].contains(&k.as_str()) {
            notes.push(format!("[{k}]: herdr's own (corgi has none)"));
        }
    }

    // Written out: the sections in corgi's order, the bindings after [keys].
    let mut text = format!(
        "# corgi config, made by `corgi migrate` from {from}.\n\
         # Only what differs from corgi's defaults: `corgi --default-config` has them all.\n"
    );
    let order = [
        "keys",
        "ui",
        "tab_bar",
        "panes",
        "sidebar",
        "theme",
        "theme.custom",
        "notifications",
        "worktrees",
    ];
    for section in order {
        if let Some(rows) = out.get(section) {
            text.push_str(&format!("\n[{section}]\n"));
            for (k, val) in rows {
                text.push_str(&format!("{k} = {val}\n"));
            }
        }
        if section == "keys" {
            for b in &binds {
                text.push_str(&format!(
                    "\n# from {}\n[[keys.bind]]\nkey = {}\n",
                    b.from,
                    quote(&b.key)
                ));
                if b.table != "prefix" {
                    text.push_str(&format!("table = {}\n", quote(b.table)));
                }
                text.push_str(&format!("command = {}\n", quote(&b.command)));
            }
        }
    }
    if !notes.is_empty() {
        text.push_str("\n# Not carried over from herdr:\n");
        for n in &notes {
            text.push_str(&format!("# - {n}\n"));
        }
    }
    Ok(Migrated { text, notes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_in_tmux_notation() {
        assert_eq!(key("prefix+shift+n"), Some(("prefix", "N".into())));
        assert_eq!(key("ctrl+alt+["), Some(("root", "C-M-[".into())));
        assert_eq!(key("prefix+minus"), Some(("prefix", "-".into())));
        assert_eq!(key("prefix+|"), Some(("prefix", "|".into())));
        assert_eq!(key("ctrl+space"), Some(("root", "C-Space".into())));
        assert_eq!(key("prefix+shift+tab"), Some(("prefix", "BTab".into())));
        assert_eq!(key("f5"), Some(("root", "F5".into())));
        assert_eq!(key("super+k"), None, "no Super in a terminal");
        assert_eq!(
            keys_of(&Value::String("prefix+alt+1..3".into())),
            [
                ("prefix+alt+1".to_string(), Some(1)),
                ("prefix+alt+2".to_string(), Some(2)),
                ("prefix+alt+3".to_string(), Some(3))
            ]
        );
    }

    /// A herdr config like the one this was written against.
    const HERDR: &str = r#"
onboarding = false
[theme]
name = "rose-pine"
auto_switch = false
[keys]
prefix = "ctrl+space"
previous_agent = "ctrl+alt+["
next_agent = "ctrl+alt+]"
focus_agent = "prefix+alt+1..9"
navigate_workspace_up = "k"
split_vertical = "prefix+|"
split_horizontal = "prefix+minus"
goto = "prefix+w"
workspace_picker = "prefix+g"
switch_tab = "prefix+1..9"
[[keys.command]]
key = "prefix+f"
type = "plugin_action"
command = "herdr-bar.open"
description = "fuzzy jump"
[ui]
sidebar_start_collapsed = true
sidebar_collapsed_mode = "hidden"
status_indicators = "symbols"
agent_panel_sort = "priority"
show_agent_labels_on_pane_borders = true
prompt_new_tab_name = false
pane_outer_borders = true
pane_gaps = true
tab_bar_position = "bottom"
host_cursor = "auto"
[ui.sidebar.agents]
rows = [["state_icon", "machine", "tab"], ["workspace", "agent"]]
[ui.sound]
enabled = false
[ui.toast]
delivery = "terminal"
[server]
headless_rows = 50
"#;

    #[test]
    fn a_herdr_config_becomes_corgis() {
        let m = migrate(HERDR, "~/.config/herdr/config.toml").unwrap();
        let t = &m.text;
        assert!(t.contains("preset = \"herdr\"\nprefix = \"C-Space\""), "{t}");
        assert!(
            t.contains("key = \"C-M-[\"\ntable = \"root\"\ncommand = \"previous-agent -a\""),
            "{t}"
        );
        assert!(t.contains("key = \"M-9\"\ncommand = \"select-agent 9\""), "{t}");
        assert!(
            t.contains("key = \"1\"\ncommand = \"select-window -t 0\""),
            "1-based tabs, 0-based windows: {t}"
        );
        assert!(t.contains("key = \"|\"\ncommand = \"split-window -h\""), "{t}");
        assert!(t.contains("key = \"-\"\ncommand = \"split-window -v\""), "{t}");
        assert!(t.contains("key = \"g\"\ncommand = \"choose-tree -s\""), "{t}");
        assert!(t.contains("[sidebar]\nmode = \"hidden\""), "{t}");
        assert!(t.contains("agent_sort = \"priority\""), "{t}");
        assert!(
            t.contains("agent_row = \"{state} {machine} {window} {workspace} {agent}\""),
            "{t}"
        );
        assert!(t.contains("[tab_bar]\nposition = \"bottom\""), "{t}");
        assert!(t.contains("indicators = \"symbols\""), "{t}");
        assert!(t.contains("[notifications]\ntoasts = false\nhost = \"auto\""), "{t}");
        // What couldn't come along, said.
        for n in [
            "rose-pine",
            "herdr-bar.open",
            "navigate_workspace_up",
            "ui.host_cursor",
            "[server]",
        ] {
            assert!(m.notes.iter().any(|x| x.contains(n)), "{n}: {:?}", m.notes);
            assert!(t.contains(n), "{n} listed in the file");
        }
        assert!(
            !m.notes
                .iter()
                .any(|x| x.contains("prompt_new_tab_name") || x.contains("sound"))
        );
        // And corgi reads it, without a word of complaint.
        let loaded = crate::config::parse(t).unwrap();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.config.keys.prefix, "C-Space");
        crate::keys::Keymap::build(&loaded.config.keys).unwrap();
        assert!(migrate("not = [toml", "x").is_err());
    }
}
