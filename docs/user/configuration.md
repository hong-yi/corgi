# Configuration

Where corgi's config file lives, how it is read and reloaded, and every section and key with its type and default.

## The file

corgi reads one TOML file. The first of these that applies:

| Source | Path |
|---|---|
| `--config <file>` | That file. |
| `CORGI_CONFIG` | That file. |
| Linux, macOS | `$XDG_CONFIG_HOME/corgi/config.toml`, else `~/.config/corgi/config.toml`. |
| Windows | `%APPDATA%\corgi\config.toml`. |

Every key is optional. A missing file, or an empty one, is the defaults. To start from the defaults with their comments:

```sh
corgi --default-config > ~/.config/corgi/config.toml
```

See also [files and paths](files-and-paths.md) and [CLI](cli.md).

## Errors and warnings

| Problem | What happens |
|---|---|
| A key or section corgi doesn't know (`[ui] sidebarr = true`) | A warning, `unknown key ui.sidebarr`; the rest of the file applies. On reload the warnings show in the status line. |
| A bad value (`position = "left"`, a number out of range, a key that isn't a key, a command that doesn't parse, an unknown theme or colour) | An error naming the key and the reason. |
| A file that isn't TOML | An error. |

At startup an error stops corgi before it takes the screen, printing `corgi: <file>: <reason>` and the log's path; it exits with status 1. On reload an error keeps the config you had and says `config not reloaded: <reason>`.

## Reloading

`C-b R` (`source-file`) reads the same file again. The theme, keys, sidebar, tab bar, panes, notifications and usage segments change at once; usage segments start reading again. The sidebar goes back to its configured `mode` (undoing `C-b b`); a width you set by dragging or with keys still applies.

`ui.mouse` applies on reload too: corgi takes the mouse from your terminal, or gives it back, at once.

## Value types

| Type | Written as |
|---|---|
| duration | `"400ms"`, `"2s"`, `"1.5s"`, `"1m"`, or a number of milliseconds (`400`). |
| key | tmux notation: `"C-b"`, `"M-Space"`. See [keybindings](keybindings.md#key-notation). |
| colour | `"#rrggbb"`, `"#rgb"`, or a name: `reset` (or `default`), `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white` (or `gray`), `bright-black` (or `dark-gray`), `bright-red`, `bright-green`, `bright-yellow`, `bright-blue`, `bright-magenta`, `bright-cyan`, `bright-white`. `_` works for `-`. |

## `[keys]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `prefix` | key | `"C-b"` | The prefix. Pressing it twice sends it to the pane. |
| `preset` | `"tmux"`, `"herdr"`, `"none"` | `"tmux"` | The bindings to start from. |
| `repeat_time` | duration | `"500ms"` | How long repeatable keys (arrows, resize) keep working without the prefix. |
| `unbind` | list of strings | `[]` | Keys to remove from the preset: `"%"` (prefix table) or `"root M-h"`. |
| `hints` | `"always"`, `"delay"`, `"off"` | `"always"` | The key-hints dialog after the prefix. `delay`: only if no key comes within `hint_delay`. |
| `hints_position` | `"bottom"`, `"center"` | `"bottom"` | Across the window above a bottom tab bar, or a dialog in the middle. |
| `hint_delay` | duration | `"400ms"` | The wait for `hints = "delay"`. |

### `[[keys.bind]]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `key` | key | `""` (required) | The key. |
| `command` | string | `""` (required) | A corgi [command](commands.md). |
| `table` | string | `"prefix"` | `prefix`, `root`, `resize`, or your own table. |
| `repeat` | bool | `false` | Repeatable. |
| `note` | string | the command | Its label in the key-hints dialog and `C-b ?`. |
| `group` | string | `"Custom"` | Its group in the key-hints dialog. |

Details and examples: [keybindings](keybindings.md).

## `[ui]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `corners` | `"sharp"`, `"rounded"` | `"sharp"` | Box corners on the sidebar, panes, tab bar and dialogs. |
| `indicators` | `"dots"`, `"symbols"` | `"dots"` | Agent status: dots `● ● ● ○ ·`, or symbols `× ✓ ◐ ○ ·` (waiting, done, working, idle, unknown). |
| `window_title` | bool | `true` | Set your terminal window's title to `corgi · workspace:window`. |
| `mouse` | bool | `true` | corgi takes the mouse (focus, scroll, select, drag). `false`: your terminal handles it. Read at startup only. |
| `confirm_close` | `"auto"`, `"always"`, `"never"` | `"auto"` | When closing asks first. `auto`: workspaces always; panes and windows when an agent runs in them. |
| `narrow_width` | integer | `64` | At this many columns or fewer, the narrow layout: a two-row header instead of the sidebar and tab bar, only the focused pane. `0`: never. |

## `[tab_bar]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `position` | `"top"`, `"bottom"` | `"bottom"` | Where the window list goes. |
| `style` | `"plain"`, `"box"` | `"plain"` | One row, or a box like the sidebar's (three rows; plain again on short or narrow terminals). |
| `span` | `"window"`, `"panes"` | `"window"` | The full width (under the sidebar too), or only beside the sidebar. |
| `padding` | integer, 0-5 | `0` | Empty rows between the tab bar and the panes. |

## `[panes]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `style` | `"box"`, `"plain"` | `"box"` | A border around the panes, the focused pane's edges lit; or none. |
| `title` | `"auto"`, `"cwd"`, `"agent"`, `"off"` | `"auto"` | What each pane's tab says. See [pane tabs](pane-tabs.md). |
| `title_shape` | `"breadcrumbs"`, `"line"`, `"rounded"`, `"angled"`, `"slant"`, `"plain"` | `"breadcrumbs"` | How it's drawn. See [pane tabs](pane-tabs.md). |
| `separate` | bool | `true` | Each pane in its own box (herdr's look), or one-line shared dividers (tmux's). |
| `padding` | integer, 0-10 | `0` | Blank cells between separate panes. `0`: their boxes touch. |
| `inset` | integer, 0-4 | `1` | Blank columns between each pane's border and its terminal, each side. |
| `inset_rows` | integer, 0-4 | `0` | Blank rows, the same way. |

## `[mode]`

The badge at the start of the tab bar: what your keys do now. Any text; `""` hides the badge in that state. Its colours are `mode_normal`, `mode_prefix`, `mode_sidebar` and `mode_dialog` in `[theme.custom]`.

| Key | Default | Shown when |
|---|---|---|
| `normal` | `"NORMAL"` | Keys go to the focused pane. |
| `prefix` | `"PREFIX"` | After the prefix, and while repeat keys work. |
| `sidebar` | `"SIDEBAR"` | The sidebar has the keyboard. |
| `choose` | `"CHOOSE"` | The tree chooser, the worktrees and machines dialogs. |
| `rename` | `"RENAME"` | A rename dialog. |
| `confirm` | `"CONFIRM"` | A y/n question or a close confirmation. |
| `new` | `"NEW"` | The New Workspace, New Window and Add Machine dialogs. |
| `command` | `"COMMAND"` | The `:` prompt. |
| `keys` | `"KEYS"` | `C-b ?`. |
| `menu` | `"MENU"` | An action menu. |
| `usage` | `"USAGE"` | The usage details. |
| `panes` | `"PANES"` | Pane numbers (`C-b q`). |
| `copy` | `"COPY"` | Copy mode. |
| `popup` | `"POPUP"` | A popup has the keys. |
| `resize` | `"RESIZE"` | The `resize` table. Other tables show their name in capitals. |

## `[sidebar]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `mode` | `"show"`, `"collapsed"`, `"hidden"` | `"show"` | `collapsed`: status icons only. `C-b b` toggles. It hides below 60 columns. |
| `style` | `"box"`, `"line"` | `"box"` | A box around it, or a line between it and the panes. |
| `header` | `"row"`, `"border"` | `"row"` | With `box`: each part's name ("Workspaces", "Agents") on a row of its own with a line under it, or in the box's top border. |
| `padding` | integer, 0-10 | `1` | Empty columns between the sidebar and the panes. |
| `width` | integer, 10-80 | `26` | Starting width. Dragging its edge or resizing it with keys overrides this for your client. |
| `workspace_row` | template | `"{state} {workspace} {branch}"` | A workspace's row. |
| `window_row` | template | `"{state} {window}"` | A window's row. |
| `pane_row` | template | `"{state} {agent}"` | A pane's row. |
| `agent_row` | template | `"{state} {agent} {workspace}:{window}"` | A row in the agents list. |
| `agent_sort` | `"grouped"`, `"priority"` | `"grouped"` | Agents in workspace order, or most urgent first (waiting, done, working, idle). |
| `agents` | `"split"`, `"below"` | `"split"` | The sidebar in two (tree on top, agents below, each boxed), or the agents straight after the tree. |
| `agents_height` | integer, 20-80 | `50` | With `split`: the agents' share of the height, in percent. |

The templates' tokens: [sidebar tokens](sidebar-tokens.md).

## `[theme]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | `"auto"` | A built-in theme, or `auto`: `light` or `dark` by your terminal's background. |
| `light` | string | `"catppuccin-latte"` | The theme `auto` uses on a light background. |
| `dark` | string | `"catppuccin-mocha"` | The theme `auto` uses on a dark background, or when the background isn't known. |

Built in: `catppuccin-mocha`, `catppuccin-latte`, `tokyo-night`, `gruvbox-dark`, `nord`, `dracula`, `terminal` (your terminal's own colours). Themes colour corgi's chrome, never your panes. See [themes](themes.md).

### `[theme.custom]`

Single colours over the theme, `name = colour`. Names: `fg`, `muted`, `surface`, `accent`, `accent_fg`, `badge`, `badge_fg`, `prefix`, `border`, `border_active`, `working`, `blocked`, `done`, `idle`, `unknown`, `mode_normal`, `mode_prefix`, `mode_sidebar`, `mode_dialog`. Any other name is an error.

```toml
[theme.custom]
accent = "#89b4fa"
blocked = "red"
```

## `[notifications]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `toasts` | bool | `true` | Toasts inside corgi, over the panes. |
| `position` | `"top-left"`, `"top-center"`, `"top-right"`, `"bottom-left"`, `"bottom-center"`, `"bottom-right"` | `"top-right"` | Where. `"top"`, `"top-middle"`, `"bottom"`, `"bottom-middle"` also work. |
| `duration` | duration | `"6s"` | How long a toast stays. |
| `agents` | bool | `true` | Also toast when an agent needs you or finishes, not only for a program's OSC 9 / OSC 777. |
| `host` | `"auto"`, `"osc777"`, `"osc9"`, `"osc99"`, `"bell"`, `"off"` | `"auto"` | Pass notices on to your terminal while its window isn't focused. `auto` goes by the environment, else the bell. |

See [notifications](notifications.md).

## `[worktrees]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `directory` | string | `"~/.corgi/worktrees"` | Where New Workspace's git worktrees go: `<directory>/<repository>/<branch>`. `~` is home on the workspace's machine. |

## `[paste]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `images` | `"remote"`, `"always"`, `"off"` | `"remote"` | When an empty paste (the clipboard holds only an image) or a dropped image file is sent beside the pane's terminal and its path pasted instead: for panes on other machines, for every pane, or never. `C-b C-v` works either way. |
| `image_command` | string | unset | A shell line that prints the clipboard's image (`"pngpaste -"`), instead of osascript (macOS) or wl-paste / xclip (Linux). |

## `[usage]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `enabled` | bool | `true` | Usage segments in the tab bar. |
| `segment` | array of tables | none | `[[usage.segment]]`. None given: `agents` (priority 90), `claude` (60), `codex` (50). Any given replace those. |

### `[[usage.segment]]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `provider` | string | required | `agents`, `claude`, `codex`, `command`, `clock`, `host` or `system`. |
| `priority` | integer | `50` | Higher keeps its place (and its full text) first when the bar is short. |
| `interval` | duration, at least 1s | `"30s"` | How often it's read. |
| `format` | string | the provider's | The full text, from the provider's fields (`{tokens_today}`). |
| `short` | string | the provider's | The text when the full one doesn't fit. |
| `cmd` | string | unset | `command`: the shell line. Required for `command`. |
| `timeout` | duration | `"5s"` | `command`: how long it may take. |
| `bg`, `fg` | colour, or `"none"` for `bg` | the provider's | The block's colours; `bg = "none"`: plain text. |
| `show` | `"left"`, `"used"`, `"tokens"` | `"left"` | `claude`, `codex`: percent of the limit left, percent used, or tokens today. |
| `limit` | `"max"` or a number of tokens | unset | `claude`, without Claude Code's own numbers: estimate the 5-hour block's share left of this budget. |
| `on` | `"here"`, `"workspace"` | `workspace` for `host` and `system`, else `here` | `command`, `host`, `system`: read on this machine, or on the active workspace's machine. |

Fields, colours and defaults per provider: [usage providers](usage-providers.md).

## Not in the config

There is no `[core]` section: the core session and socket are set on the command line (`--session`, `--socket`) or with `HC_SESSION` / `HC_SOCKET` (see [CLI](cli.md)). Sidebar settings live in `[sidebar]` and tab bar settings in `[tab_bar]` (not `[ui.sidebar]` / `[ui.tab_bar]`). Not yet: a settings dialog, sounds, and importing `~/.tmux.conf`.
