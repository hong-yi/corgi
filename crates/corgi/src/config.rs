//! `~/.config/corgi/config.toml`. Every key has a default, so an empty or
//! missing file is a full config. Unknown keys are warnings (shown, then
//! ignored); wrong values are errors that keep the previous config.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DEFAULT_TOML: &str = include_str!("default-config.toml");

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub keys: Keys,
    pub ui: Ui,
    pub tab_bar: TabBar,
    pub panes: Panes,
    pub mode: ModeLabels,
    pub sidebar: Sidebar,
    pub theme: ThemeConfig,
    pub notifications: Notifications,
    pub worktrees: Worktrees,
    pub usage: Usage,
    pub paste: Paste,
}

/// Usage in the tab bar's free space (`[[usage.segment]]`).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Usage {
    pub enabled: bool,
    /// None given: agents, claude and codex.
    pub segment: Vec<Segment>,
}

impl Default for Usage {
    fn default() -> Usage {
        Usage {
            enabled: true,
            segment: Vec::new(),
        }
    }
}

impl Usage {
    /// The segments to show: as configured, or the defaults.
    pub fn segments(&self) -> Vec<Segment> {
        if !self.enabled {
            return Vec::new();
        }
        if !self.segment.is_empty() {
            return self.segment.clone();
        }
        [("agents", 90), ("claude", 60), ("codex", 50)]
            .into_iter()
            .map(|(p, priority)| Segment {
                provider: p.into(),
                priority,
                ..Segment::default()
            })
            .collect()
    }
}

/// One thing in the tab bar: where its value comes from, how it reads, how much it matters.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Segment {
    /// `agents`, `claude`, `codex`, `command`, `clock`, `host`, `system`.
    pub provider: String,
    /// Higher keeps its place (and its full text) first when the bar is short.
    pub priority: i32,
    #[serde(deserialize_with = "duration")]
    pub interval: Duration,
    /// The full text, from the provider's fields (`{tokens_today}`); else the provider's own.
    pub format: Option<String>,
    /// The short text, for when the full one doesn't fit.
    pub short: Option<String>,
    /// `command`: the shell line whose first line of output is shown.
    pub cmd: Option<String>,
    #[serde(deserialize_with = "duration")]
    pub timeout: Duration,
    /// Its block's colours (`#rrggbb`, `#rgb` or a name); `bg = "none"`: plain text.
    /// Unset: the provider's own.
    pub bg: Option<String>,
    pub fg: Option<String>,
    /// claude, codex: what the segment shows: `left` (percent of the limit left; the
    /// default), `used` (percent used) or `tokens` (tokens today).
    pub show: Show,
    /// claude, without Claude Code's own numbers (see `corgi claude-statusline`): estimate the
    /// 5-hour block's share left of a token budget: `"max"` (your biggest block in the last
    /// week, as ccusage does) or a number. Unset: no estimate, tokens instead.
    pub limit: Option<Limit>,
    /// command, host, system: where it's read: `here` (this machine) or `workspace` (the
    /// active workspace's machine, through core). Unset: `workspace` for host and system,
    /// `here` for command.
    pub on: Option<On>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum On {
    Here,
    Workspace,
}

impl Segment {
    /// Read on the active workspace's machine when that's another one.
    pub fn follows_workspace(&self) -> bool {
        match self.on {
            Some(on) => on == On::Workspace,
            None => matches!(self.provider.as_str(), "host" | "system"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Show {
    #[default]
    Left,
    Used,
    Tokens,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Limit {
    Tokens(u64),
    /// Only `"max"`.
    Named(String),
}

impl Default for Segment {
    fn default() -> Segment {
        Segment {
            provider: String::new(),
            priority: 50,
            interval: Duration::from_secs(30),
            format: None,
            short: None,
            cmd: None,
            timeout: Duration::from_secs(5),
            bg: None,
            fg: None,
            show: Show::Left,
            limit: None,
            on: None,
        }
    }
}

pub const PROVIDERS: &[&str] = &["agents", "claude", "codex", "command", "clock", "host", "system"];

/// Images pasted into panes.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Paste {
    pub images: PasteImages,
    /// A shell line that prints the clipboard's image (`pngpaste -`), instead of the
    /// system's (osascript on macOS; wl-paste or xclip on Linux).
    pub image_command: Option<String>,
}

/// When a paste with nothing in it (your clipboard holds an image) or a dropped image file is
/// sent beside the pane's terminal, and its path there pasted instead.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PasteImages {
    /// For panes on another machine, which can't see this one's clipboard or files.
    #[default]
    Remote,
    /// For every pane.
    Always,
    /// Never (`paste-image` still works).
    Off,
}

/// Git worktrees made from the New Workspace dialog.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Worktrees {
    /// Where they go: `<directory>/<repository>/<branch>`. `~` is home on the machine.
    pub directory: String,
}

impl Default for Worktrees {
    fn default() -> Worktrees {
        Worktrees {
            directory: "~/.corgi/worktrees".into(),
        }
    }
}

/// Notices about panes you aren't looking at: toasts inside corgi, and the host terminal's own.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Notifications {
    /// Show toasts inside corgi.
    pub toasts: bool,
    /// Where toasts go, over the panes.
    pub position: ToastPosition,
    /// How long a toast stays.
    #[serde(deserialize_with = "duration")]
    pub duration: Duration,
    /// Toast when an agent needs you or finishes (from its status), not only when a program
    /// sends a notification (OSC 9 / OSC 777).
    pub agents: bool,
    /// Pass notifications on to the terminal corgi runs in, while its window isn't focused.
    pub host: HostNotify,
}

impl Default for Notifications {
    fn default() -> Notifications {
        Notifications {
            toasts: true,
            position: ToastPosition::TopRight,
            duration: Duration::from_secs(6),
            agents: true,
            host: HostNotify::Auto,
        }
    }
}

/// A corner, or the middle of the top or bottom edge. Newest nearest the edge.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ToastPosition {
    TopLeft,
    #[serde(alias = "top", alias = "top-middle")]
    TopCenter,
    TopRight,
    BottomLeft,
    #[serde(alias = "bottom", alias = "bottom-middle")]
    BottomCenter,
    BottomRight,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HostNotify {
    /// By what the environment says the terminal is; the bell when it can't tell.
    Auto,
    Off,
    Bell,
    Osc9,
    Osc777,
    Osc99,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Keys {
    pub prefix: String,
    /// The bindings to start from: `tmux`, `herdr` (the old herdr TUI's), or `none`.
    pub preset: KeyPreset,
    pub hints: Hints,
    /// Where the key-hints dialog goes.
    pub hints_position: HintsPosition,
    #[serde(deserialize_with = "duration")]
    pub hint_delay: Duration,
    /// How long repeatable keys (arrows, resizing) keep working without the prefix.
    #[serde(deserialize_with = "duration")]
    pub repeat_time: Duration,
    /// Extra bindings, applied after the preset and `unbind`; same key replaces.
    pub bind: Vec<BindConfig>,
    /// Keys to remove from the preset: `"%"` (prefix table) or `"root M-h"`.
    pub unbind: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KeyPreset {
    Tmux,
    Herdr,
    None,
}

impl KeyPreset {
    /// The number the first window is shown with, so the labels match the keys that pick
    /// windows: tmux's `0..9` start at 0, herdr's `1..9` at 1.
    pub fn base_index(self) -> usize {
        match self {
            KeyPreset::Herdr => 1,
            KeyPreset::Tmux | KeyPreset::None => 0,
        }
    }
}

/// One `[[keys.bind]]`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct BindConfig {
    /// tmux notation: `|`, `C-h`, `M-Left`, `F5`.
    pub key: String,
    /// A corgi command, as at the `C-b :` prompt: `split-window -h -c ~/src`.
    pub command: String,
    /// `prefix` (after the prefix), `root` (no prefix), `resize`, or your own table.
    pub table: String,
    /// Repeatable: more presses within `repeat_time` need no prefix.
    pub repeat: bool,
    /// What the key-hints dialog calls it (default: the command).
    pub note: Option<String>,
    /// Which group of the key-hints dialog it's in (default "Custom").
    pub group: Option<String>,
}

impl Default for BindConfig {
    fn default() -> BindConfig {
        BindConfig {
            key: String::new(),
            command: String::new(),
            table: "prefix".into(),
            repeat: false,
            note: None,
            group: None,
        }
    }
}

impl Default for Keys {
    fn default() -> Keys {
        Keys {
            prefix: "C-b".into(),
            preset: KeyPreset::Tmux,
            hints: Hints::Always,
            hints_position: HintsPosition::Bottom,
            hint_delay: Duration::from_millis(400),
            repeat_time: Duration::from_millis(500),
            bind: Vec::new(),
            unbind: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HintsPosition {
    /// Across the bottom of the window, above a bottom tab bar, as wide as the window.
    Bottom,
    /// A dialog in the middle of the window.
    Center,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Hints {
    Always,
    Delay,
    Off,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Corners {
    /// ┌ ┐ └ ┘: edges meet cleanly where boxes sit side by side.
    Sharp,
    /// ╭ ╮ ╰ ╯
    Rounded,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Indicators {
    Dots,
    Symbols,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Ui {
    pub indicators: Indicators,
    /// Box corners everywhere (sidebar, panes, tab bar, dialogs).
    pub corners: Corners,
    /// Set the host terminal's window title to the workspace and window.
    pub window_title: bool,
    pub mouse: bool,
    /// When closing asks first.
    pub confirm_close: ConfirmClose,
    /// At this many columns or fewer (a phone over ssh): the narrow layout. 0: never.
    pub narrow_width: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConfirmClose {
    /// Workspaces always; panes and windows when an agent runs in them.
    Auto,
    Always,
    Never,
}

impl Default for Ui {
    fn default() -> Ui {
        Ui {
            indicators: Indicators::Dots,
            corners: Corners::Sharp,
            window_title: true,
            mouse: true,
            confirm_close: ConfirmClose::Auto,
            narrow_width: 64,
        }
    }
}

impl Config {
    /// What the narrow layout draws with: no sidebar, plain panes without boxes, insets or
    /// titles, a header at the top instead of the tab bar.
    pub fn narrowed(&self) -> Config {
        let mut c = self.clone();
        c.panes.style = TabBarStyle::Plain;
        c.panes.separate = false;
        c.panes.inset = 0;
        c.panes.inset_rows = 0;
        c.panes.title = PaneTitle::Off;
        c.tab_bar.style = TabBarStyle::Plain;
        c.tab_bar.position = Position::Top;
        c.tab_bar.span = TabBarSpan::Window;
        c.tab_bar.padding = 0;
        c
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TabBarStyle {
    /// A rounded box around it, like the sidebar's (three rows).
    Box,
    /// One row.
    Plain,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TabBarSpan {
    /// The whole width of the window, under (or over) the sidebar too.
    Window,
    /// Only beside the sidebar, over the panes.
    Panes,
}

/// The window list (with the workspace badge, messages and, later, usage).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct TabBar {
    pub position: Position,
    pub style: TabBarStyle,
    pub span: TabBarSpan,
    /// Empty rows between the tab bar and the panes.
    pub padding: u16,
}

impl Default for TabBar {
    fn default() -> TabBar {
        TabBar {
            position: Position::Bottom,
            style: TabBarStyle::Plain,
            span: TabBarSpan::Window,
            padding: 0,
        }
    }
}

/// The area the panes share.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Panes {
    pub style: TabBarStyle,
    /// The tab at the top of each pane.
    pub title: PaneTitle,
    /// Its ends: powerline caps (need a Nerd Font) or none.
    pub title_shape: TitleShape,
    /// Split panes apart, each in its own box (herdr's look), rather than sharing
    /// one-line dividers (tmux's).
    pub separate: bool,
    /// Blank cells between separate panes.
    pub padding: u16,
    /// Blank columns between each pane's border and its terminal (each side), and rows.
    pub inset: u16,
    pub inset_rows: u16,
}

impl Default for Panes {
    fn default() -> Panes {
        Panes {
            style: TabBarStyle::Box,
            title: PaneTitle::Auto,
            title_shape: TitleShape::Breadcrumbs,
            separate: true,
            padding: 0,
            inset: 1,
            inset_rows: 0,
        }
    }
}

/// What a pane's tab says.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PaneTitle {
    /// The agent's summary while one runs, else the directory.
    Auto,
    /// The directory's name.
    Cwd,
    /// Only agents get a tab: their summary.
    Agent,
    /// No tabs.
    Off,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TitleShape {
    /// A row of its own at the top of the pane, as breadcrumbs (`~ › src › corgi`,
    /// `corgi › claude › Fixing the bug`), its activity as a circle at the end.
    Breadcrumbs,
    /// The title as text on the line above the pane, its activity as a circle at the end.
    Line,
    /// `` … ``
    Rounded,
    /// `` … ``
    Angled,
    /// `` … ``
    Slant,
    /// No caps: plain text on a coloured block (any font).
    Plain,
}

/// The mode badge at the start of the tab bar: what your keys do right now. `""` hides it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct ModeLabels {
    /// Keys go to the focused pane.
    pub normal: String,
    /// After the prefix (and while repeat keys still work).
    pub prefix: String,
    /// The sidebar has the keyboard.
    pub sidebar: String,
    /// The tree chooser (C-b w, C-b s).
    pub choose: String,
    /// Typing a new name (C-b , and C-b $).
    pub rename: String,
    /// A y/n question (C-b x, C-b &).
    pub confirm: String,
    /// The New Window / New Workspace dialogs (M6).
    pub new: String,
    /// The `:` command prompt.
    pub command: String,
    /// The list of every key (`C-b ?`).
    pub keys: String,
    /// An action menu (`C-b m`).
    pub menu: String,
    /// The usage details (`C-b u`).
    pub usage: String,
    /// Pane numbers shown (`C-b q`).
    pub panes: String,
    /// Copy mode (`C-b [`).
    pub copy: String,
    /// A popup has the keys.
    pub popup: String,
    /// The resize table (`switch-client -T resize`). Other tables show their name.
    pub resize: String,
}

impl Default for ModeLabels {
    fn default() -> ModeLabels {
        let s = |x: &str| x.to_string();
        ModeLabels {
            normal: s("NORMAL"),
            prefix: s("PREFIX"),
            sidebar: s("SIDEBAR"),
            choose: s("CHOOSE"),
            rename: s("RENAME"),
            confirm: s("CONFIRM"),
            new: s("NEW"),
            command: s("COMMAND"),
            keys: s("KEYS"),
            menu: s("MENU"),
            usage: s("USAGE"),
            panes: s("PANES"),
            copy: s("COPY"),
            popup: s("POPUP"),
            resize: s("RESIZE"),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SidebarMode {
    Show,
    /// Status icons only.
    Collapsed,
    Hidden,
}

impl SidebarMode {
    /// What C-b b brings back: the configured mode, or `show` if it's configured hidden.
    pub fn max_shown(self) -> SidebarMode {
        if self == SidebarMode::Hidden {
            SidebarMode::Show
        } else {
            self
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SidebarStyle {
    /// A rounded box around it.
    Box,
    /// A line between it and the panes.
    Line,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentSort {
    /// In workspace and window order.
    Grouped,
    /// Most urgent first: blocked, done, working, idle.
    Priority,
}

/// Row templates take tokens: `{state}`, `{workspace}`, `{branch}`, `{machine}`, `{window}`,
/// `{pane}`, `{agent}`, `{title}`, and `{label:KEY}` (a core label on the terminal).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Sidebar {
    pub mode: SidebarMode,
    pub style: SidebarStyle,
    /// With `style = "box"`: where each part's name goes.
    pub header: SidebarHeader,
    /// Empty columns between the sidebar and the panes.
    pub padding: u16,
    pub width: u16,
    pub workspace_row: String,
    pub window_row: String,
    pub pane_row: String,
    pub agent_row: String,
    pub agent_sort: AgentSort,
    /// Where the agents go: the bottom part of the sidebar, or straight below the tree.
    pub agents: AgentsPlace,
    /// With `agents = "split"`: how much of the sidebar's height they get, in percent.
    pub agents_height: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SidebarHeader {
    /// A row of its own at the top of the box, a line under it (like the panes' breadcrumbs).
    Row,
    /// In the box's top border.
    Border,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentsPlace {
    /// The sidebar in two: the tree on top, the agents in the bottom part, each in its own box.
    Split,
    /// Right after the tree.
    Below,
}

impl Default for Sidebar {
    fn default() -> Sidebar {
        Sidebar {
            mode: SidebarMode::Show,
            style: SidebarStyle::Box,
            header: SidebarHeader::Row,
            padding: 1,
            width: 26,
            workspace_row: "{state} {workspace} {branch}".into(),
            window_row: "{state} {window}".into(),
            pane_row: "{state} {agent}".into(),
            agent_row: "{state} {agent} {workspace}:{window}".into(),
            agent_sort: AgentSort::Grouped,
            agents: AgentsPlace::Split,
            agents_height: 50,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct ThemeConfig {
    /// A theme name, or `auto` to pick `light`/`dark` from the terminal's background.
    pub name: String,
    pub light: String,
    pub dark: String,
    /// Per-colour overrides, e.g. `accent = "#89b4fa"`.
    pub custom: BTreeMap<String, String>,
}

impl Default for ThemeConfig {
    fn default() -> ThemeConfig {
        ThemeConfig {
            name: "auto".into(),
            light: "catppuccin-latte".into(),
            dark: "catppuccin-mocha".into(),
            custom: BTreeMap::new(),
        }
    }
}

/// `"400ms"`, `"2s"`, `"1m"`, or a number of milliseconds.
fn duration<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Ms(u64),
        Text(String),
    }
    match Raw::deserialize(d)? {
        Raw::Ms(n) => Ok(Duration::from_millis(n)),
        Raw::Text(s) => parse_duration(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("bad duration {s:?}; use e.g. \"400ms\" or \"2s\""))),
    }
}

pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    let (num, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit() && c != '.')?);
    let n: f64 = num.parse().ok()?;
    let ms = match unit.trim() {
        "ms" => n,
        "s" => n * 1000.0,
        "m" => n * 60_000.0,
        _ => return None,
    };
    Some(Duration::from_millis(ms as u64))
}

/// A config plus what was wrong with it but survivable.
#[derive(Debug)]
pub struct Loaded {
    pub config: Config,
    pub warnings: Vec<String>,
}

pub fn default_path() -> PathBuf {
    crate::paths::config_dir().join("config.toml")
}

/// Parse TOML text. Unknown keys become warnings; bad values are an error.
pub fn parse(text: &str) -> Result<Loaded, String> {
    let mut warnings = Vec::new();
    let de = toml::Deserializer::new(text);
    let config: Config = serde_ignored::deserialize(de, |path| warnings.push(format!("unknown key {path}")))
        .map_err(|e| e.message().trim().to_string())?;
    if config.tab_bar.padding > 5 {
        return Err(format!(
            "tab_bar.padding {} is out of range (0-5)",
            config.tab_bar.padding
        ));
    }
    for s in &config.usage.segment {
        if !PROVIDERS.contains(&s.provider.as_str()) {
            return Err(format!(
                "usage.segment: no provider {:?} ({})",
                s.provider,
                PROVIDERS.join(", ")
            ));
        }
        if s.provider == "command" && s.cmd.as_deref().is_none_or(|c| c.trim().is_empty()) {
            return Err("usage.segment: a command segment needs cmd".into());
        }
        for c in [&s.bg, &s.fg].into_iter().flatten() {
            if c != "none" && crate::theme::parse_color(c).is_none() {
                return Err(format!("usage.segment {}: {c:?} isn't a colour", s.provider));
            }
        }
        match &s.limit {
            Some(Limit::Named(n)) if n != "max" => {
                return Err(format!(
                    "usage.segment {}: limit {n:?}: \"max\" or a number of tokens",
                    s.provider
                ));
            }
            Some(Limit::Tokens(0)) => return Err(format!("usage.segment {}: limit 0", s.provider)),
            _ => {}
        }
        if s.interval < Duration::from_secs(1) {
            return Err(format!("usage.segment {}: interval under a second", s.provider));
        }
    }
    if config.panes.padding > 10 {
        return Err(format!("panes.padding {} is out of range (0-10)", config.panes.padding));
    }
    if config.panes.inset > 4 || config.panes.inset_rows > 4 {
        return Err(format!(
            "panes.inset {} / inset_rows {}: out of range (0-4)",
            config.panes.inset, config.panes.inset_rows
        ));
    }
    if !(20..=80).contains(&config.sidebar.agents_height) {
        return Err(format!(
            "sidebar.agents_height {} is out of range (20-80)",
            config.sidebar.agents_height
        ));
    }
    if config.sidebar.padding > 10 {
        return Err(format!(
            "sidebar.padding {} is out of range (0-10)",
            config.sidebar.padding
        ));
    }
    if !(10..=80).contains(&config.sidebar.width) {
        return Err(format!(
            "sidebar.width {} is out of range (10-80)",
            config.sidebar.width
        ));
    }
    if crate::keys::Key::parse(&config.keys.prefix).is_none() {
        return Err(format!(
            "keys.prefix {:?} isn't a key (e.g. \"C-b\", \"C-a\", \"M-Space\")",
            config.keys.prefix
        ));
    }
    crate::keys::Keymap::build(&config.keys)?;
    crate::theme::Theme::resolve(&config.theme, None)?;
    Ok(Loaded { config, warnings })
}

/// Read the file; a missing file is the defaults.
pub fn load(path: &Path) -> Result<Loaded, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Loaded {
            config: Config::default(),
            warnings: vec![],
        }),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_default_files_are_the_defaults() {
        assert_eq!(parse("").unwrap().config, Config::default());
        let d = parse(DEFAULT_TOML).unwrap();
        assert!(d.warnings.is_empty(), "{:?}", d.warnings);
        assert_eq!(
            d.config,
            Config::default(),
            "the printed defaults match the built-in ones"
        );
    }

    #[test]
    fn values_parse() {
        let c = parse(
            r##"
            [keys]
            prefix = "C-a"
            hints = "delay"
            hint_delay = "1.5s"
            [ui]
            indicators = "symbols"
            [tab_bar]
            position = "bottom"
            style = "plain"
            [sidebar]
            mode = "collapsed"
            agent_sort = "priority"
            [theme]
            name = "nord"
            custom = { accent = "#ff0000" }
            [notifications]
            host = "osc9"
            position = "top"
            duration = "10s"
            "##,
        )
        .unwrap()
        .config;
        assert_eq!(c.keys.prefix, "C-a");
        assert_eq!(c.keys.hint_delay, Duration::from_millis(1500));
        assert_eq!(
            (c.tab_bar.position, c.sidebar.mode),
            (Position::Bottom, SidebarMode::Collapsed)
        );
        assert_eq!(c.sidebar.agent_sort, AgentSort::Priority);
        assert_eq!(c.theme.custom["accent"], "#ff0000");
        assert_eq!(c.notifications.host, HostNotify::Osc9);
        assert_eq!(c.notifications.position, ToastPosition::TopCenter);
        assert_eq!(c.notifications.duration, Duration::from_secs(10));
        assert_eq!(c.sidebar.width, 26, "unset keys keep their defaults");
    }

    #[test]
    fn unknown_keys_warn_bad_values_fail() {
        let l = parse("[ui]\nsidebarr = true\n[tab_bar]\nposition = \"bottom\"\n[nope]\nx = 1\n").unwrap();
        assert_eq!(l.warnings, ["unknown key ui.sidebarr", "unknown key nope"]);
        assert_eq!(l.config.tab_bar.position, Position::Bottom, "the rest still applies");
        assert!(parse("[tab_bar]\nposition = \"left\"").unwrap_err().contains("left"));
        assert!(parse("[tab_bar]\nstyle = \"fancy\"").unwrap_err().contains("fancy"));
        assert!(parse("[tab_bar]\npadding = 6").unwrap_err().contains("tab_bar.padding"));
        let m = parse("[mode]\nnormal = \"-- N --\"\nprefix = \"\"\n")
            .unwrap()
            .config
            .mode;
        assert_eq!(
            (m.normal.as_str(), m.prefix.as_str(), m.sidebar.as_str()),
            ("-- N --", "", "SIDEBAR")
        );
        assert_eq!(
            parse("[panes]\nstyle = \"plain\"").unwrap().config.panes.style,
            TabBarStyle::Plain
        );
        assert_eq!(
            parse("[tab_bar]\nspan = \"panes\"").unwrap().config.tab_bar.span,
            TabBarSpan::Panes
        );
        assert_eq!(
            parse("[ui]\ncorners = \"rounded\"").unwrap().config.ui.corners,
            Corners::Rounded
        );
        assert_eq!(Config::default().ui.corners, Corners::Sharp);
        assert_eq!(Config::default().panes.title, PaneTitle::Auto);
        assert_eq!(Config::default().keys.hints_position, HintsPosition::Bottom);
        assert!(Config::default().panes.separate);
        assert_eq!(
            (Config::default().panes.inset, Config::default().panes.inset_rows),
            (1, 0)
        );
        assert!(parse("[panes]\ninset = 9").unwrap_err().contains("inset"));
        let u = Config::default().usage.segments();
        assert_eq!(
            u.iter().map(|s| s.provider.as_str()).collect::<Vec<_>>(),
            ["agents", "claude", "codex"]
        );
        let c = parse("[[usage.segment]]\nprovider = \"command\"\ncmd = \"date\"\ninterval = \"1m\"\npriority = 5")
            .unwrap()
            .config;
        assert_eq!(c.usage.segments().len(), 1, "configured ones replace the defaults");
        assert_eq!(c.usage.segment[0].interval, Duration::from_secs(60));
        assert!(
            parse("[[usage.segment]]\nprovider = \"weather\"")
                .unwrap_err()
                .contains("weather")
        );
        assert!(
            parse("[[usage.segment]]\nprovider = \"command\"")
                .unwrap_err()
                .contains("needs cmd")
        );
        assert!(
            parse("[usage]\nenabled = false")
                .unwrap()
                .config
                .usage
                .segments()
                .is_empty()
        );
        assert!(parse("[[usage.segment]]\nprovider = \"clock\"\nbg = \"#123456\"\nfg = \"white\"").is_ok());
        assert!(parse("[[usage.segment]]\nprovider = \"clock\"\nbg = \"none\"").is_ok());
        let c = parse("[[usage.segment]]\nprovider = \"claude\"\nshow = \"used\"\nlimit = 88000")
            .unwrap()
            .config;
        assert_eq!(
            (c.usage.segment[0].show, c.usage.segment[0].limit.clone()),
            (Show::Used, Some(Limit::Tokens(88000)))
        );
        assert_eq!(
            Config::default().usage.segments()[1].limit,
            None,
            "no estimate unless asked"
        );
        assert!(
            parse("[[usage.segment]]\nprovider = \"claude\"\nlimit = \"lots\"")
                .unwrap_err()
                .contains("lots")
        );
        assert!(parse("[[usage.segment]]\nprovider = \"claude\"\nshow = \"all\"").is_err());
        assert!(
            parse("[[usage.segment]]\nprovider = \"clock\"\nbg = \"blurple\"")
                .unwrap_err()
                .contains("blurple")
        );
        assert_eq!(Config::default().worktrees.directory, "~/.corgi/worktrees");
        assert_eq!(
            parse("[worktrees]\ndirectory = \"~/trees\"")
                .unwrap()
                .config
                .worktrees
                .directory,
            "~/trees"
        );
        assert_eq!(Config::default().ui.confirm_close, ConfirmClose::Auto);
        assert_eq!(
            parse("[ui]\nconfirm_close = \"never\"")
                .unwrap()
                .config
                .ui
                .confirm_close,
            ConfirmClose::Never
        );
        assert_eq!(Config::default().sidebar.agents, AgentsPlace::Split);
        let sb = parse("[sidebar]\nagents = \"below\"\nagents_height = 30")
            .unwrap()
            .config
            .sidebar;
        assert_eq!((sb.agents, sb.agents_height), (AgentsPlace::Below, 30));
        assert!(
            parse("[sidebar]\nagents_height = 95")
                .unwrap_err()
                .contains("agents_height")
        );
        let p = parse("[panes]\nseparate = false\npadding = 2").unwrap().config.panes;
        assert_eq!((p.separate, p.padding), (false, 2));
        assert!(parse("[panes]\npadding = 11").unwrap_err().contains("panes.padding"));
        assert_eq!(
            parse("[keys]\nhints_position = \"center\"")
                .unwrap()
                .config
                .keys
                .hints_position,
            HintsPosition::Center
        );
        let p = parse("[panes]\ntitle = \"cwd\"\ntitle_shape = \"plain\"")
            .unwrap()
            .config
            .panes;
        assert_eq!((p.title, p.title_shape), (PaneTitle::Cwd, TitleShape::Plain));
        assert!(parse("[panes]\ntitle_shape = \"round\"").unwrap_err().contains("round"));
        assert!(parse("[tab_bar]\nspan = \"half\"").unwrap_err().contains("half"));
        assert!(parse("[sidebar]\nwidth = 3").unwrap_err().contains("out of range"));
        assert!(parse("[sidebar]\npadding = 11").unwrap_err().contains("padding"));
        assert_eq!(parse("[sidebar]\npadding = 0").unwrap().config.sidebar.padding, 0);
        assert!(parse("[keys]\nprefix = \"C-\"").unwrap_err().contains("isn't a key"));
        assert!(parse("[keys]\nhint_delay = \"soon\"").is_err());
        assert!(parse("[theme]\nname = \"nope\"").unwrap_err().contains("nope"));
        assert!(
            parse("[theme.custom]\naccent = \"blue-ish\"")
                .unwrap_err()
                .contains("blue-ish")
        );
        assert!(parse("not toml [").is_err());
    }

    #[test]
    fn windows_are_numbered_like_the_keys_that_pick_them() {
        // tmux: prefix 0 is window 0. herdr: prefix 1 is the first window, so it's shown as 1.
        assert_eq!(KeyPreset::Tmux.base_index(), 0);
        assert_eq!(KeyPreset::Herdr.base_index(), 1);
    }

    #[test]
    fn key_bindings_in_the_config() {
        let c = parse(
            r#"
            [keys]
            preset = "herdr"
            repeat_time = "1s"
            unbind = ["v", "root M-x"]
            [[keys.bind]]
            key = "|"
            command = "split-window -h"
            note = "split right"
            [[keys.bind]]
            key = "M-h"
            table = "root"
            command = "select-pane -L"
            "#,
        )
        .unwrap()
        .config;
        assert_eq!(c.keys.preset, KeyPreset::Herdr);
        assert_eq!(c.keys.repeat_time, Duration::from_secs(1));
        assert_eq!(c.keys.bind.len(), 2);
        assert_eq!(
            (c.keys.bind[1].table.as_str(), c.keys.bind[0].table.as_str()),
            ("root", "prefix")
        );
        let e = parse("[[keys.bind]]\nkey = \"x\"\ncommand = \"split-windo\"\n")
            .err()
            .unwrap();
        assert!(
            e.contains("keys.bind") && e.contains("split-window"),
            "the reason and a suggestion: {e}"
        );
        assert!(parse("[keys]\npreset = \"emacs\"").is_err());
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("400ms"), Some(Duration::from_millis(400)));
        assert_eq!(parse_duration("2s"), Some(Duration::from_secs(2)));
        assert_eq!(parse_duration("1m"), Some(Duration::from_secs(60)));
        assert_eq!(parse_duration("fast"), None);
    }
}
