# Keybindings

Every default key in corgi, table by table, and how you change them with `[[keys.bind]]` and `keys.unbind`.

All bindings run [commands](commands.md): a key does exactly what typing its command at the `C-b :` prompt does. `C-b ?` lists every binding you have now, so you can always check what your config produced.

## Key notation

Keys are written in tmux notation, in `[[keys.bind]] key`, `keys.unbind`, `keys.prefix` and `send-keys`.

| Part | Meaning |
|---|---|
| `C-` | Ctrl held. `C-b`, `C-Left`. |
| `M-` | Alt (Meta) held. `M-1`, `M-Up`. |
| `C-M-x` | Both, in either order. |
| A single character | That character: `c`, `%`, `"`, `N`, `-`, `|`. Shift is part of the character (`N`, `%`), so there is no `S-` for characters. |
| `Up` `Down` `Left` `Right` | Arrow keys. |
| `Enter` `Tab` `Space` `BSpace` `Home` `End` | As named. |
| `BTab` or `S-Tab` | Shift+Tab. |
| `Escape` or `Esc` | Escape. |
| `PageUp` (`PPage`, `PgUp`), `PageDown` (`NPage`, `PgDn`) | Page keys, under tmux's names or the plain ones. |
| `Delete` (`DC`), `Insert` (`IC`) | As named. |
| `F1` … `F12` | Function keys (`F` and a number). |

With Ctrl held, letters match either case: some terminals report `C-b` as `C-B` with Shift.

Anything else (`Hyper-q`, `Nope`) is an error that names the binding.

## Key tables

| Table | When corgi looks in it |
|---|---|
| `prefix` | After you press the prefix key (`keys.prefix`, `C-b` by default). |
| `root` | Every key, with no prefix, before the focused pane sees it. Empty by default. |
| `resize` | After `switch-client -T resize` (`C-b r` in the herdr preset). |
| Your own | After `switch-client -T <name>`, for any table a `[[keys.bind]]` names. |

How each behaves:

- **Prefix.** Press the prefix, then one key. An unbound key does nothing and ends the prefix; `Esc` cancels it. The prefix pressed twice sends it to the pane (`send-prefix`).
- **Root.** A bound key runs its command and never reaches the pane. Unbound keys go to the pane as usual. A root binding of the prefix key itself never runs: the prefix is checked first.
- **Other tables** (`resize`, your own). The table stays in use while you press its *repeatable* bindings. A binding that isn't repeatable runs and returns you to normal keys. `Esc` or `Enter` leaves the table; any unbound key leaves it too and is dropped. The prefix key leaves the table and starts the prefix. The mode badge shows `RESIZE` for `resize` (`[mode] resize`) and the table's name in capitals for any other.

### Repeat keys

A repeatable binding (tmux's `bind -r`, `repeat = true` in the config):

- In the `prefix` table: after it runs, more presses of any repeatable prefix binding within `keys.repeat_time` (default `500ms`) need no prefix. Each press restarts the time. The badge keeps saying `PREFIX` until the time runs out. Any other key ends the repeat and is handled as a normal key.
- In other tables: the table stays in use after it runs.
- In `root`: no effect.

## Changing bindings

The keymap is built in this order: the preset, then the prefix's `send-prefix` binding, then `keys.unbind`, then each `[[keys.bind]]`. A binding for a key that is already bound in that table replaces it. A mistake (a key that isn't a key, a command that doesn't parse) is an error that names the binding, and the config isn't used: at startup corgi stops before taking the screen, and on `C-b R` it keeps the config it had. See [configuration](configuration.md).

### `[keys]` keys for bindings

| Key | Type | Default | Meaning |
|---|---|---|---|
| `prefix` | key | `"C-b"` | The prefix key. |
| `preset` | `"tmux"`, `"herdr"`, `"none"` | `"tmux"` | The bindings to start from. |
| `repeat_time` | duration | `"500ms"` | How long repeat keys work without the prefix. |
| `unbind` | list of strings | `[]` | Keys to remove from the preset. |
| `bind` | array of tables | none | Your bindings (`[[keys.bind]]`). |

### `[[keys.bind]]`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `key` | key | required | The key, in the notation above. |
| `command` | string | required | A corgi command, as at the `C-b :` prompt. See [commands](commands.md). |
| `table` | string | `"prefix"` | `prefix`, `root`, `resize`, or a table of your own. |
| `repeat` | bool | `false` | Repeatable (see above). |
| `note` | string | the command | What the key-hints dialog and `C-b ?` call it. |
| `group` | string | `"Custom"` | Which group of the key-hints dialog it goes in. A name that matches a preset group (`Panes`, `Other`, …) joins that group. |

```toml
[[keys.bind]]
key = "|"
command = "split-window -h"
note = "split right"

[[keys.bind]]
key = "M-h"
table = "root"
command = "select-pane -L"

# A table of your own: C-b g enters it, then l or s run something.
[[keys.bind]]
key = "g"
command = "switch-client -T git"
note = "git…"

[[keys.bind]]
key = "l"
table = "git"
command = "display-popup -E -w 90% -h 90% lazygit"
note = "lazygit"

[[keys.bind]]
key = "s"
table = "git"
command = "run-shell git status --short"
note = "status"
```

### `keys.unbind`

Each entry is a key in the `prefix` table (`"%"`), or a table name, a space and a key (`"root M-h"`, `"resize h"`). A table or key that isn't bound is ignored.

```toml
[keys]
unbind = ["%", "\"", "resize H"]
```

Unbinding the first key of a hint group's entry also removes that hint line: for example, unbinding `Up` in the tmux preset removes the `h j k l  select pane (or ←↑↓→)` line, though `h j k l` still work. Add a `[[keys.bind]]` with a `note` if you want a line back.

### Presets

| `keys.preset` | What you get |
|---|---|
| `tmux` (default) | tmux's defaults where corgi has the command, plus corgi's own (dialogs, worktrees, machines, usage). |
| `herdr` | The old herdr TUI's keys: `v`/`-` splits, `hjkl` focus, `HJKL` swap, `Tab`, `r` for resize mode, `1-9` windows. |
| `none` | Only `d` `:` `?` `m` `u` and the prefix twice. Build the rest with `[[keys.bind]]`. |

`corgi migrate` writes a config on the herdr preset from your herdr config; see [migrating from herdr](migrating-from-herdr.md).

## The tmux preset

### `prefix` table

The "Hint" column is the key-hints dialog's group and label. A dash means the key has no line of its own: another line covers it.

| Key | Command | Repeat | Hint |
|---|---|---|---|
| `c` | `new-window` | | Windows: `c` new window |
| `C` | `dialog new-window` | | Windows: `C` new window… |
| `n` | `next-window` | | Windows: `n` next window |
| `p` | `previous-window` | | Windows: `p` prev window |
| `Tab` | `last-window` | | Windows: `Tab` last window |
| `'` | `command-prompt -p index "select-window -t ':%%'"` | | Windows: `'` window by index |
| `,` | `rename-window` | | Windows: `,` rename window |
| `&` | `kill-window` | | Windows: `&` kill window |
| `0` … `9` | `select-window -t 0` … `select-window -t 9` | | Windows: `0-9` select window |
| `%` | `split-window -h` | | Panes: `%` split right |
| `"` | `split-window -v` | | Panes: `"` split down |
| `Up` | `select-pane -U` | yes | Panes: `h j k l` select pane (or ←↑↓→) |
| `Down` | `select-pane -D` | yes | - |
| `Left` | `select-pane -L` | yes | - |
| `Right` | `select-pane -R` | yes | - |
| `h` | `select-pane -L` | | - |
| `j` | `select-pane -D` | | - |
| `k` | `select-pane -U` | | - |
| `l` | `select-pane -R` | | - |
| `o` | `next-pane` | | Panes: `o` next pane |
| `;` | `last-pane` | | Panes: `;` last pane |
| `z` | `resize-pane -Z` | | Panes: `z` zoom |
| `[` | `copy-mode` | | Panes: `[ ]` copy mode / paste |
| `]` | `paste-buffer` | | - |
| `C-v` | `paste-image` | | Panes: `C-v` paste clipboard image |
| `x` | `kill-pane` | | Panes: `x` kill pane |
| `{` | `swap-pane -U` | | Panes: `{ }` swap prev / next |
| `}` | `swap-pane -D` | | - |
| `!` | `break-pane` | | Panes: `!` break to window |
| `Space` | `next-layout` | | Panes: `Space` next layout |
| `M-1` | `select-layout even-horizontal` | | Panes: `M-1..5` select layout |
| `M-2` | `select-layout even-vertical` | | - |
| `M-3` | `select-layout main-horizontal` | | - |
| `M-4` | `select-layout main-vertical` | | - |
| `M-5` | `select-layout tiled` | | - |
| `q` | `display-panes` | | Panes: `q` pane numbers |
| `C-Up` | `resize-pane -U 1` | yes | Panes: `C-←↑↓→` resize |
| `C-Down` | `resize-pane -D 1` | yes | - |
| `C-Left` | `resize-pane -L 1` | yes | - |
| `C-Right` | `resize-pane -R 1` | yes | - |
| `M-Up` | `resize-pane -U 5` | yes | Panes: `M-←↑↓→` resize by 5 |
| `M-Down` | `resize-pane -D 5` | yes | - |
| `M-Left` | `resize-pane -L 5` | yes | - |
| `M-Right` | `resize-pane -R 5` | yes | - |
| `PageUp` | `copy-mode -u` | | - (no line) |
| `N` | `dialog new-workspace` | | Workspaces: `N` new workspace… |
| `W` | `choose-worktree` | | Workspaces: `W` worktrees |
| `M` | `choose-machine` | | Workspaces: `M` machines |
| `$` | `rename-workspace` | | Workspaces: `$` rename workspace |
| `(` | `switch-client -p` | | Workspaces: `( )` prev / next workspace |
| `)` | `switch-client -n` | | - |
| `w` | `choose-tree` | | Workspaces: `w` choose from the tree |
| `s` | `choose-tree -s` | | Workspaces: `s` choose workspace |
| `a` | `next-agent -b` | | Agents: `a` next blocked agent |
| `A` | `next-agent -d` | | Agents: `A` next finished agent |
| `d` | `detach-client` | | Other: `d` detach |
| `:` | `command-prompt` | | Other: `:` command |
| `?` | `list-keys` | | Other: `?` all keys |
| `e` | `edit-scrollback` | | Other: `e` scrollback in $EDITOR |
| `m` | `display-menu` | | Other: `m` menu… |
| `u` | `display-usage` | | Other: `u` usage |
| `b` | `toggle-sidebar` | | Other: `b` toggle sidebar |
| `R` | `source-file` | | Other: `R` reload config |
| the prefix (`C-b`) | `send-prefix` | | Other: `C-b` send C-b |

Differences from tmux: `l` is a pane direction (as are `h j k`), so `last-window` is on `Tab`. `C-b a` runs `next-agent -b`: the next agent waiting for you (blocked), in tree order, wrapping round. See [next-agent](commands.md#next-agent).

### `resize` table

In the tmux preset no key enters it; bind one (`command = "switch-client -T resize"`) or type `:switch-client -T resize`.

| Key | Command | Repeat | Hint |
|---|---|---|---|
| `h` | `resize-pane -L 1` | yes | Resize: `h j k l` resize |
| `Left` | `resize-pane -L 1` | yes | - |
| `H` | `resize-pane -L 5` | yes | Resize: `H J K L` resize by 5 |
| `j` | `resize-pane -D 1` | yes | - |
| `Down` | `resize-pane -D 1` | yes | - |
| `J` | `resize-pane -D 5` | yes | - |
| `k` | `resize-pane -U 1` | yes | - |
| `Up` | `resize-pane -U 1` | yes | - |
| `K` | `resize-pane -U 5` | yes | - |
| `l` | `resize-pane -R 1` | yes | - |
| `Right` | `resize-pane -R 1` | yes | - |
| `L` | `resize-pane -R 5` | yes | - |

`Esc` or `Enter` leaves the table (hint: Other: `Esc` leave).

### `root` table

Empty.

## The herdr preset

### `prefix` table

| Key | Command | Repeat | Hint |
|---|---|---|---|
| `c` | `new-window` | | Windows: `c` new window |
| `C` | `dialog new-window` | | Windows: `C` new window… |
| `n` | `next-window` | | Windows: `n` next window |
| `p` | `previous-window` | | Windows: `p` prev window |
| `T` | `rename-window` | | Windows: `T` rename window |
| `X` | `kill-window` | | Windows: `X` close window |
| `1` … `9` | `select-window -t 0` … `select-window -t 8` | | Windows: `1-9` select window |
| `v` | `split-window -h` | | Panes: `v` split right |
| `-` | `split-window -v` | | Panes: `-` split down |
| `h` | `select-pane -L` | | Panes: `h j k l` focus pane |
| `j` | `select-pane -D` | | - |
| `k` | `select-pane -U` | | - |
| `l` | `select-pane -R` | | - |
| `H` | `swap-pane left` | | Panes: `H J K L` swap pane |
| `J` | `swap-pane down` | | - |
| `K` | `swap-pane up` | | - |
| `L` | `swap-pane right` | | - |
| `Tab` | `next-pane` | | Panes: `Tab` next pane |
| `BTab` | `previous-pane` | | - |
| `x` | `kill-pane` | | Panes: `x` close pane |
| `z` | `resize-pane -Z` | | Panes: `z` zoom |
| `[` | `copy-mode` | | Panes: `[ ]` copy mode / paste |
| `]` | `paste-buffer` | | - |
| `C-v` | `paste-image` | | Panes: `C-v` paste clipboard image |
| `r` | `switch-client -T resize` | | Panes: `r` resize mode |
| `N` | `dialog new-workspace` | | Workspaces: `N` new workspace… |
| `M` | `choose-machine` | | Workspaces: `M` machines |
| `W` | `rename-workspace` | | Workspaces: `W` rename workspace |
| `D` | `kill-workspace` | | Workspaces: `D` close workspace |
| `w` | `choose-tree -s` | | Workspaces: `w` workspaces |
| `g` | `choose-tree` | | Workspaces: `g` go to |
| `q` | `detach-client` | | Other: `q` detach |
| `:` | `command-prompt` | | Other: `:` command |
| `?` | `list-keys` | | Other: `?` all keys |
| `e` | `edit-scrollback` | | Other: `e` scrollback in $EDITOR |
| `m` | `display-menu` | | Other: `m` menu… |
| `u` | `display-usage` | | Other: `u` usage |
| `b` | `toggle-sidebar` | | Other: `b` toggle sidebar |
| `R` | `source-file` | | Other: `R` reload config |
| the prefix (`C-b`) | `send-prefix` | | Other: `C-b` send C-b |

`W` renames the workspace, as in herdr, so no key opens the worktrees dialog in this preset. Use `:choose-worktree`, the action menu, or bind a key:

```toml
[[keys.bind]]
key = "G"
command = "choose-worktree"
note = "worktrees"
group = "Workspaces"
```

The herdr preset has no arrow keys, no `C-`/`M-` resize keys, no agent keys (`a`, `A`), no `display-panes` and no layout keys. `corgi migrate` carries herdr's agent keys over as `next-agent` / `previous-agent` / `select-agent` bindings.

### `resize` table

The same as the tmux preset's `resize` table above. `C-b r` enters it; the key-hints dialog shows its keys while you're in it.

### `root` table

Empty.

## The none preset

| Table | Key | Command | Hint |
|---|---|---|---|
| `prefix` | `d` | `detach-client` | Other: `d` detach |
| `prefix` | `:` | `command-prompt` | Other: `:` command |
| `prefix` | `?` | `list-keys` | Other: `?` all keys |
| `prefix` | `m` | `display-menu` | Other: `m` menu… |
| `prefix` | `u` | `display-usage` | Other: `u` usage |
| `prefix` | the prefix | `send-prefix` | Other: `C-b` send C-b |

There is no `resize` table in this preset until you bind one.

## The sidebar, while it has the keyboard

`C-b ←` (`select-pane -L`) from the left-most pane gives the sidebar the keyboard (badge `SIDEBAR`). See [navigation](navigation.md).

| Key | Does |
|---|---|
| `↑` `↓`, `k` `j`, `C-p` `C-n`, `C-k` `C-j` | Move. |
| `Home` `End` | First, last row. |
| `→` `l` | Unfold. |
| `←` `h` | Fold; on a folded row or a leaf, go to its parent row. |
| `Tab` | Fold or unfold. |
| `Enter` | Go there; the keyboard goes back to the panes. |
| `m` | The action menu for the row. |
| `Esc`, `C-c`, `C-g` | Back to the panes. |

Other characters do nothing (the sidebar doesn't filter). The `root` table still applies, and the prefix works:

| Prefix key | Does |
|---|---|
| `select-pane -R` (`C-b →`, `C-b l`) | Back to the panes. |
| Other `select-pane` keys | Nothing. |
| `resize-pane -L n` / `-R n` (`C-b C-←/→` by 1, `C-b M-←/→` by 5) | Narrow or widen the sidebar. Repeatable. The width is kept per client. |
| Other `resize-pane` keys | Nothing. |
| Anything else | The keyboard goes back to the panes, and the command runs. |

## Copy mode

`C-b [` (`copy-mode`), `C-b PageUp` (`copy-mode -u`, starting a screen up). Badge `COPY`. These keys are fixed: there is no copy-mode key table to rebind yet, and no emacs keys (not yet). See [copy mode](copy-mode.md).

A count before a motion repeats it (`5j`, `3w`). `0` is a motion unless a count is being typed.

| Key | Does |
|---|---|
| `h` `←`, `l` `→` | Left, right. |
| `j` `↓`, `k` `↑` | Down, up (keeping the column). |
| `w` `b` `e` | Next word, word back, word end. |
| `W` `B` `E` | The same for WORDs (anything but spaces). |
| `0` `Home` | Line start. |
| `^` | First non-blank character. |
| `$` `End` | Line end. In a block selection, every line's end. |
| `f` `F` `t` `T` then a character | To that character on the line, forward or back; `t` `T` stop just before it. |
| `;` `,` | The last `f F t T` again, the same way or the other way. |
| `{` `}` | To the blank line before or after the paragraph. |
| `gg` `G` | Top, bottom. |
| `H` `M` `L` | Top, middle, bottom of the screen. |
| `C-u` `C-d` | Half a screen up, down. |
| `C-b` `C-f`, `PageUp` `PageDown` | A screen up, down. With the default prefix `C-b`, `C-b` is the prefix (see below): use `PageUp`. |
| `/` `?` | Search down, up. Type the text, `Enter` to search, `Esc` to cancel, `BSpace` to delete, `C-u` to clear. All-lowercase text matches any case. The search wraps round. |
| `n` `N` | The last search again, the same way or the other. |
| `v` | Select characters (again: stop selecting). |
| `V` | Select lines. |
| `C-v` | Select a block (a rectangle). |
| `o` | Go to the selection's other end. |
| `y` | Copy the selection and leave; with nothing selected, copy the cursor's line. |
| `Enter` | Copy the selection and leave; with nothing selected, just leave. |
| `Esc` | Clear the selection; with none, leave. |
| `q`, `C-c` | Leave. |
| The prefix key | Leave copy mode and start the prefix (not while typing a search). |

The mouse wheel scrolls copy mode's view by 3 rows. A copy goes to corgi's paste buffer (`C-b ]`) and to your terminal's clipboard over OSC 52.

## Dialogs

### Form dialogs

New Workspace, New Window, Add Machine and the rename dialogs. Badge `NEW` or `RENAME`.

| Key | Does |
|---|---|
| `Tab` | Next field. In a path field, completes first: to the only match, or the matches' common start, and moves on only when there's nothing to add. |
| `S-Tab`, `↑`, `C-p` | Previous field. |
| `↓`, `C-n` | Next field. |
| `Enter` | Submit. |
| `Esc`, `C-c`, `C-g` | Cancel. |
| `C-t` | Add Machine only: run `hc machine add …` in a popup instead, where ssh can ask for passwords or host keys. |

In text and path fields:

| Key | Does |
|---|---|
| `←` `C-b`, `→` `C-f` | Move a character. |
| `Home` `C-a`, `End` `C-e` | Start, end. |
| `BSpace` | Delete before the cursor. |
| `Delete`, `C-d` | Delete at the cursor. |
| `C-k` | Delete to the end. |
| `C-u` | Delete to the start. |
| `C-w` | Delete the word before the cursor (words end at spaces and `/`). |

In a select field, `←` and `→` (or `Space`) change the choice. A toggle flips with `Space`, `←` or `→`.

While a dialog waits on something (making a worktree, reaching a machine), only `Esc` does anything.

### Questions and read-only dialogs

Close confirmations, removing a worktree or machine, and the usage details (`C-b u`).

| Key | Does |
|---|---|
| `Enter`, `y`, `Y` | Yes (or close, for read-only ones). |
| `Esc`, `n`, `N`, `q`, `C-c`, `C-g` | No. |

### The y/n prompt (`confirm-before`)

Badge `CONFIRM`. `y` runs the command; any other key cancels.

### The command prompt (`C-b :`)

Badge `COMMAND`.

| Key | Does |
|---|---|
| `Enter` | Run it. Empty does nothing. |
| `Esc`, `C-c`, `C-g` | Cancel. |
| `↑` `↓` | History (this session's), newest first. |
| `Tab` | Complete the command name (while there's no space yet): the only match, or the matches' common start; else the matches are listed. |
| `BSpace` | Delete the last character. |
| `C-u` | Clear. |

### Tree chooser (`C-b w`, `C-b s`)

Badge `CHOOSE`.

| Key | Does |
|---|---|
| `↑` `C-p` `C-k`, `↓` `C-n` `C-j` | Move. `↓` from the last row reaches "+ New workspace". |
| `Home` | First row. |
| `End` | The "+ New workspace" row. |
| `→` | Unfold. |
| `←` | Fold; on a folded row or a leaf, go to its parent. |
| `Tab` | Fold or unfold. |
| `Enter` | Go there. On "+ New workspace" (also chosen when the filter matches nothing): the New Workspace dialog, named after the filter. |
| Typing | Filter. |
| `BSpace` | Delete from the filter. |
| `C-u` | Clear the filter. |
| `Esc` | Clear the filter; with none, close. |
| `C-c`, `C-g` | Close. |

Folding here folds the sidebar too.

### All keys (`C-b ?`)

Badge `KEYS`.

| Key | Does |
|---|---|
| Typing | Filter by key, command, description or table. |
| `BSpace`, `C-u` | Delete from, clear the filter. |
| `↑` `C-p` `C-k`, `↓` `C-n` `C-j` | Move. |
| `PageUp` `PageDown` | Move by 10. |
| `Home` `End` | First, last. |
| `Enter` | Run the selected binding's command. |
| `Esc` | Clear the filter; with none, close. |
| `C-c`, `C-g` | Close. |

### Action menus (`C-b m`, `m` in the sidebar, right-click)

Badge `MENU`. Each entry has its own key, shown beside it.

| Key | Does |
|---|---|
| An entry's key | Run it at once. |
| `↑` `k` `C-p` `S-Tab`, `↓` `j` `C-n` `Tab` | Move (wrapping round). |
| `Home` `End` | First, last. |
| `Enter` | Run the selected entry. |
| `Esc`, `C-c`, `C-g` | Close. A click anywhere else closes it too. |

The entries are listed in [actions](actions.md#menus).

### Worktrees (`C-b W`)

Badge `CHOOSE`.

| Key | Does |
|---|---|
| `↑` `k` `C-p`, `↓` `j` `C-n` | Move. |
| `Enter` | Open the worktree as a workspace, or go to its workspace. |
| `n` | New worktree (New Workspace with the worktree switch on). |
| `d`, `Delete` | Remove it (asked first). The main checkout can't be removed. |
| `Esc`, `q`, `C-c`, `C-g` | Close. |

### Machines (`C-b M`)

Badge `CHOOSE`. The keys that act on a machine work only when there is one.

| Key | Does |
|---|---|
| `↑` `k` `C-p`, `↓` `j` `C-n` | Move. |
| `a`, `n` | Add a machine. |
| `Enter`, `c` | Connect now. |
| `w` | New workspace on it. |
| `r` | Rename it. |
| `d`, `Delete` | Forget it (asked first). |
| `l` | `hc machine login` in a popup. |
| `u` | `hc machine update` in a popup. |
| `Esc`, `q`, `C-c`, `C-g` | Close. |

### Pane numbers (`C-b q`)

Each pane's number shows for 1 second (badge `PANES`). A digit focuses that pane (numbered from 0); any other key puts the numbers away.

### Popups (`display-popup`, `C-b e`)

Every key goes to the popup's program (badge `POPUP`), except:

| Key | Does |
|---|---|
| prefix `x` | Close the popup. |
| prefix prefix | Send the prefix to the popup's program. |
| prefix, any other key | Nothing. |
| Any key, after the program ended | Close the popup. |

## Key-hints dialog

When you press the prefix, corgi shows what the next key can do. It is built from your keymap (the preset, `unbind` and `[[keys.bind]]`), so it always matches your bindings. It is information only: the next key runs as it would without the dialog, and the dialog closes either way. `Esc` cancels the prefix; `?` opens the list of every key (`list-keys`).

What it shows:

- One group per hint group (Windows, Panes, Workspaces, Agents, Other for the presets; Resize in the resize table), in the order the table first uses them. Your bindings go in `Custom` (or their `group`), placed last. `Esc  cancel` (in the prefix table) or `Esc  leave` (other tables) ends the Other group.
- Groups are kept whole and packed into columns; each column is as wide as its own entries.
- A footer: `Esc cancel · ? all keys`, plus `· N more groups` when some groups don't fit.

Which tables show it:

| Table | Shown |
|---|---|
| `prefix` | After the prefix, per `keys.hints`. |
| `resize` and your own tables | As soon as you enter them, unless `keys.hints = "off"`. `"delay"` doesn't apply to them. |
| `root` | Never. |

When the terminal is too small for the dialog (under 28 columns or 8 rows), a one-line strip at the bottom shows the prefix and every `key:label` instead.

The footer mentions `?` in every table, but `?` opens all keys only where it is bound (the prefix table). In the `resize` table it leaves the table.

Settings, in `[keys]`:

| Key | Values | Default | Meaning |
|---|---|---|---|
| `hints` | `"always"`, `"delay"`, `"off"` | `"always"` | `always`: at once after the prefix. `delay`: only if no key comes within `hint_delay`. `off`: never (other tables included). |
| `hint_delay` | duration | `"400ms"` | The wait for `"delay"`. |
| `hints_position` | `"bottom"`, `"center"` | `"bottom"` | `bottom`: across the whole window, just above a bottom tab bar. `center`: a dialog in the middle of the window, up to 160 columns wide. |

```toml
[keys]
hints = "delay"
hint_delay = "1s"
hints_position = "center"
```

## Related

- [Commands](commands.md): what each command does and its flags.
- [Actions](actions.md): every action and where it is reachable.
- [Configuration](configuration.md): every config key.
