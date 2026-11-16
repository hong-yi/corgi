# Migrating from herdr

Move from the old herdr TUI to corgi: what the names map to, how `corgi migrate` carries your config over, and what changed.

corgi replaces the herdr TUI. The terminals now live in herdr-core (`hc`), and corgi is the window onto them. Closing or detaching corgi leaves every terminal running in core, as herdr's own server did.

## Concept map

| herdr | corgi | Notes |
|---|---|---|
| space (workspace) | workspace | A folder, its windows, maybe a machine |
| tab | window | Numbered from 0, as in tmux (herdr's tabs started at 1) |
| pane | pane | Each one is a core terminal (`t7`, or `vps/t7` on another machine) |
| `herdr --session` | `corgi --session` or `HC_SESSION` | A core session |
| herdr remote, saved machines | core machines, `C-b M` | See [remote-machines.md](remote-machines.md) |
| tab bar right-side entries (`hostname`, `datetime`, `command`, …) | usage segments | See [usage-status.md](usage-status.md) |
| agent detection | OSC 7501 only | See [Status detection](#status-detection) |

## Make a corgi config from herdr's

```sh
corgi migrate
```

This reads `~/.config/herdr/config.toml` (`$XDG_CONFIG_HOME/herdr/config.toml` if that's set) and writes a corgi config to the usual place, `~/.config/corgi/config.toml`. It prints what it wrote and what it couldn't carry over.

| Flag | What it does |
|---|---|
| `--print` | Print the new config instead of writing it. Read this first. |
| `--from <file>` | Read this herdr config instead |
| `--force` | Replace a corgi config that's already there. The old one is kept as `config.toml.bak`. |
| `--config <file>` | (A `corgi` option, before `migrate`) Write here instead of the usual place |

Without `--force`, `corgi migrate` won't touch an existing corgi config:

```sh
corgi migrate --print            # look at it
corgi migrate --force            # then replace yours, keeping a .bak
corgi --config ~/corgi-test.toml migrate --from ~/old/herdr.toml
```

The file it writes holds only what differs from corgi's defaults. `corgi --default-config` prints them all. At the end of the file, a comment lists everything that wasn't carried over, and why.

## The herdr key preset

The migrated config starts from corgi's herdr preset:

```toml
[keys]
preset = "herdr"
```

You can also set this by hand, without migrating. The herdr preset has the old herdr TUI's keys, after the prefix (`C-b` unless you set another):

| Key | What it does |
|---|---|
| `c` / `C` | New window / New Window dialog |
| `n` / `p` | Next / previous window |
| `1`–`9` | Window 0–8 |
| `T` | Rename window |
| `X` | Close window |
| `v` / `-` | Split right / split down |
| `h` `j` `k` `l` | Focus pane left, down, up, right |
| `H` `J` `K` `L` | Swap pane in that direction |
| `Tab` / `BTab` | Next / previous pane |
| `x` | Close pane |
| `z` | Zoom |
| `r` | Resize mode: `h j k l` or arrows by 1, `H J K L` by 5, until Esc or Enter |
| `[` / `]` | Copy mode / paste |
| `C-v` | Paste clipboard image |
| `N` | New Workspace dialog |
| `W` | Rename workspace |
| `D` | Close workspace |
| `M` | Machines |
| `w` | Choose workspace |
| `g` | Go to (the whole tree) |
| `q` | Detach |
| `:` | Command prompt |
| `?` | All keys |
| `e` | Scrollback in `$EDITOR` |
| `m` | Menu |
| `u` | Usage details |
| `b` | Toggle sidebar |
| `R` | Reload config |

In the herdr preset, the Worktrees dialog has no key of its own (`W` renames the workspace). Run it with `C-b :` `choose-worktree`, or bind it:

```toml
[[keys.bind]]
key = "G"
command = "choose-worktree"
```

`C-b ?` lists every key you have, and the key hints after the prefix show them too.

## What `corgi migrate` carries over

| herdr | corgi |
|---|---|
| `keys.prefix` | `keys.prefix` (in tmux notation: `ctrl+a` becomes `C-a`) |
| Each key you set under `[keys]` | A `[[keys.bind]]` with the matching corgi command. Keys without `prefix+` go in the `root` table. `1..9` ranges spread into one binding per key. Tab numbers become window numbers from 0. |
| `theme.name`, or `auto_switch` with `dark_name` / `light_name` | `theme.name`, or `name = "auto"` with `dark` / `light`, where corgi has the theme |
| `theme.custom`: `accent` `fg` `muted` `surface` `border` `working` `blocked` `done` `idle` | `[theme.custom]` |
| `ui.accent` | `theme.custom.accent` |
| `ui.sidebar_width` | `sidebar.width` |
| `ui.sidebar_start_collapsed`, `ui.sidebar_collapsed_mode` | `sidebar.mode = "collapsed"` or `"hidden"` |
| `ui.agent_panel_sort` | `sidebar.agent_sort` |
| `ui.sidebar.agents.rows` | `sidebar.agent_row` (on one line) |
| `ui.tab_bar_position` | `tab_bar.position` |
| `ui.pane_borders` off | `panes.style = "plain"` |
| `ui.pane_gaps` | `panes.separate` |
| `ui.show_agent_labels_on_pane_borders` | `panes.title = "auto"` |
| `ui.mobile_width_threshold` | `ui.narrow_width` |
| `ui.mouse_capture` | `ui.mouse` |
| `ui.confirm_close` | `ui.confirm_close` (`"auto"` or `"never"`) |
| `ui.status_indicators` | `ui.indicators` |
| `ui.window_title = ""` | `ui.window_title = false` |
| `ui.toast.delivery` | `"herdr"`: toasts on. `"terminal"` or `"system"`: toasts off, `notifications.host = "auto"`. `"off"`: both off. |
| `worktrees.directory` | `worktrees.directory` |

Example:

```toml
# from keys.split_vertical = "prefix+v"
[[keys.bind]]
key = "v"
command = "split-window -h"

# from keys.next_agent = "alt+j"
[[keys.bind]]
key = "M-j"
table = "root"
command = "next-agent -a"
```

## What isn't carried over

Each of these is listed in the file and when it's written:

| herdr | Why |
|---|---|
| `keys.command` (custom commands) | No direct counterpart: bind a corgi command with `[[keys.bind]]` (see [custom-commands.md](custom-commands.md)) |
| Plugin key bindings (`type = "plugin_action"`) | Plugins aren't built yet |
| `keys.indexed` | Not carried; check `C-b ?` |
| `keys.navigate_*` | The sidebar's keys are fixed: `j k h l` and the arrows |
| `keys.settings` | No settings dialog yet: the config file is it |
| `keys.open_notification_target` | Click a toast instead, or `C-b w` |
| `keys.move_tab_previous`, `keys.move_tab_next` | corgi doesn't reorder windows yet |
| `keys.switch_workspace` | No workspace-by-number key yet: `C-b w`, or `(` and `)` |
| `keys.remote_image_paste` | corgi's is `C-b C-v`, and an empty paste into a remote pane does it too |
| A theme corgi doesn't have | corgi has 7 built-in themes (see [themes.md](themes.md)) |
| `ui.sound` | corgi plays no sounds yet |
| Other `ui.*` keys | Not carried |
| Any other section (`[plugins]`, …) | herdr's own |

`ui.prompt_new_tab_name`, `ui.prompt_new_workspace_name`, `ui.pane_outer_borders` and `[onboarding]` are dropped without a note.

## What changed

### Status detection

herdr worked out what agents were doing from their screens, their processes and installed hooks. corgi reads status only from OSC 7501, the sequence agents (or their integrations) send to say working, blocked, done or idle. corgi doesn't scrape screens, and there's no integration installer.

An agent that never sends OSC 7501 shows no status in corgi. See [agent-status.md](agent-status.md) for what to do about it.

The "seen" mark is the shared `herdr.seen` label on the core terminal.

### Plugins

corgi doesn't run plugins yet. They're planned, in herdr's manifest format, so herdr plugins that only use actions and hooks can work. Until then, use custom commands, popups and the `hc` CLI ([custom-commands.md](custom-commands.md)).

### Other things not there yet

- A settings dialog
- Reordering windows by key
- Sounds, and a log of past notifications
- 11 of herdr's 18 themes
- Kitty graphics passthrough

## Related

- [coming-from-tmux.md](coming-from-tmux.md): the default key preset
- [keybindings.md](keybindings.md): every key in both presets
- [cli.md](cli.md): `corgi migrate` and the other subcommands
- [configuration.md](configuration.md): every config key
