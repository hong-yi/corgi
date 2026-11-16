# Actions

Everything corgi can do, and every way to reach it: a key, the `C-b :` prompt, a menu, or the mouse.

Each action is a [command](commands.md); keys, menus and `[[keys.bind]]` all run commands, so an action is reachable from the `:` prompt whenever it has a command. The key columns are the defaults of the tmux and herdr [presets](keybindings.md#presets), after the prefix (`C-b`).

## Every action

| Action | Command | tmux key | herdr key | Menu | Mouse |
|---|---|---|---|---|---|
| New window | `new-window` | `c` | `c` | Window `c` | Tab bar `+` |
| New Window dialog | `dialog new-window` | `C` | `C` | Window `C` | |
| Next / previous window | `next-window`, `previous-window` | `n`, `p` | `n`, `p` | | |
| Last window | `last-window` | `Tab` | | | |
| Window by index | `select-window -t N` | `0`-`9`, `'` | `1`-`9` (windows 0-8) | | Click a tab |
| Rename window | `rename-window [name]` | `,` | `T` | Window `,` | |
| Close window | `kill-window` | `&` | `X` | Window `&` | |
| Split | `split-window -h` / `-v` | `%`, `"` | `v`, `-` | Pane `%`, `"` | |
| Focus a pane by direction | `select-pane -L/-R/-U/-D` | arrows, `h j k l` | `h j k l` | | Click a pane or its tab |
| Last pane | `select-pane -l`, `last-pane` | `;` | | | |
| Next / previous pane | `next-pane`, `previous-pane` | `o` | `Tab`, `BTab` | | |
| Focus by number | `display-panes`, then a digit | `q` | | | |
| Zoom | `resize-pane -Z` | `z` | `z` | Pane `z` | |
| Resize | `resize-pane -L/-R/-U/-D [n]` | `C-` / `M-` arrows | `r`, then `h j k l` / `H J K L` | | |
| Resize table | `switch-client -T resize` | | `r` | | |
| Swap with previous / next | `swap-pane -U` / `-D` | `{`, `}` | | | |
| Swap by direction | `swap-pane left/right/up/down`, `-L`, `-R` | | `H J K L` | | |
| Break into its own window | `break-pane` | `!` | | Pane `!` | |
| Rename pane | `rename-pane [name]` | | | Pane `r` | |
| Close pane | `kill-pane` | `x` | `x` | Pane `x` | |
| Next layout | `next-layout` | `Space` | | Window `l` | |
| A preset layout | `select-layout <name>` | `M-1`-`M-5` | | | |
| New workspace (no dialog) | `new-workspace [-c dir]` | | | | |
| New Workspace dialog | `dialog new-workspace` | `N` | `N` | Workspace `N` | Chooser "+ New workspace" |
| New worktree workspace | `dialog new-worktree` | | | Workspace `W` | |
| Rename workspace | `rename-workspace [name]` | `$` | `W` | Workspace `$` | |
| Close workspace | `kill-workspace` | | `D` | Workspace `X` | |
| Next / previous workspace | `switch-client -n` / `-p`, `next-workspace`, `previous-workspace` | `)`, `(` | | | Click a workspace in the sidebar (goes straight to it) |
| Tree chooser, at the window | `choose-tree` | `w` | `g` | | Narrow layout's "switch" |
| Tree chooser, at the workspace | `choose-tree -s` | `s` | `w` | | |
| Agent waiting for you | `next-agent` (`-p`, `-b` back) | `a` (runs `-b`: the previous one) | | | Click an agent in the sidebar |
| Agent finished unseen | `next-agent -d` | `A` | | | |
| Any agent | `next-agent -a`, `previous-agent -a` | | | | |
| The Nth agent | `select-agent N` | | | | |
| Worktrees | `choose-worktree` | `W` | | | |
| Machines | `choose-machine` | `M` | `M` | Machines `M` | |
| Add a machine | `dialog new-machine`, `machine add` | | | Machines `a` | |
| Act on a machine | `machine connect/login/update/rename/remove/new-workspace [name]` | | | Machine row | |
| Copy mode | `copy-mode`, `copy-mode -u` | `[`, `PageUp` | `[` | | Drag in a pane |
| Paste the last copy | `paste-buffer` | `]` | `]` | | |
| Paste a clipboard image | `paste-image` | `C-v` | `C-v` | | |
| Scrollback in `$EDITOR` | `edit-scrollback` | `e` | `e` | | |
| Popup terminal | `display-popup …` | | | | |
| Shell line in the background | `run-shell <line>` | | | | |
| Type keys into the pane | `send-keys …` | | | | |
| Send the prefix | `send-prefix` | prefix twice | prefix twice | | |
| Command prompt | `command-prompt …` | `:` | `:` | | |
| Ask first | `confirm-before …` | | | | |
| Use a key table | `switch-client -T <table>` | | `r` (resize) | | |
| All keys | `list-keys` | `?` | `?` | | |
| Action menu | `display-menu` | `m` | `m` | | Right-click a pane, tab or sidebar row |
| Usage details | `display-usage` | `u` | `u` | | Click the usage segments |
| Toggle the sidebar | `toggle-sidebar` | `b` | `b` | | |
| Reload the config | `source-file` | `R` | `R` | | |
| Detach | `detach-client` | `d` | `q` | | |

The `none` preset binds only `d`, `:`, `?`, `m`, `u` and the prefix twice.

## Mouse

| Where | What it does |
|---|---|
| A pane, or its tab | Click: focus it. Wheel: scroll it. |
| A pane, dragging | Select (copy mode starts); letting go copies. Double-click copies a word, triple-click a line. |
| A tab in the tab bar | Click: select that window. |
| `+` in the tab bar | New window. |
| The usage segments | Usage details. |
| A sidebar row | Click: go there. `▾`/`▸`: fold. |
| The sidebar's right edge | Drag: resize the sidebar. |
| A toast | Click: go to its pane. |
| Right-click a pane, tab or sidebar row | Its action menu. |

A program that asks for mouse reports (vim with `mouse=a`, htop) gets the mouse instead; hold Shift for corgi's. `ui.mouse = false` leaves the mouse to your terminal. See [mouse](mouse.md).

## Menus

`C-b m` opens the menu for the focused pane; `m` on a sidebar row, or a right-click, opens the one for that row, tab or pane. Each entry runs on its own key.

| Section | Key | Entry | Command |
|---|---|---|---|
| Pane | `%` | split right | `split-window -h` |
| Pane | `"` | split down | `split-window -v` |
| Pane | `z` | zoom | `resize-pane -Z` |
| Pane | `r` | rename pane… | `rename-pane` |
| Pane | `!` | break into its own window | `break-pane` |
| Pane | `x` | close pane | `kill-pane` |
| Window | `c` | new window | `new-window` |
| Window | `C` | new window… | `dialog new-window` |
| Window | `,` | rename window… | `rename-window` |
| Window | `l` | next layout | `next-layout` |
| Window | `&` | close window | `kill-window` |
| Workspace | `N` | new workspace… | `dialog new-workspace` |
| Workspace | `W` | new worktree workspace… | `dialog new-worktree` |
| Workspace | `$` | rename workspace… | `rename-workspace` |
| Workspace | `X` | close workspace | `kill-workspace` |
| Machine | `N` | new workspace on it… | `machine new-workspace <name>` |
| Machine | `c` | connect now | `machine connect <name>` |
| Machine | `l` | log in… | `machine login <name>` |
| Machine | `u` | update hc there… | `machine update <name>` |
| Machine | `r` | rename… | `machine rename <name>` |
| Machine | `X` | forget it… | `machine remove <name>` |
| Machines | `M` | machines… | `choose-machine` |
| Machines | `a` | add a machine… | `dialog new-machine` |

Which sections a menu has:

| Opened on | Sections |
|---|---|
| A pane | Pane, Window, Workspace |
| A window (tab or sidebar row) | Window, Workspace |
| A workspace row | Workspace |
| This machine's row | Workspace, Machines |
| Another machine's row | Machine, Machines |
| Nothing open | Workspace |

A menu opened on a row first goes to that row, then runs the entry.

Not yet: plugin actions in menus, and the settings dialog.
