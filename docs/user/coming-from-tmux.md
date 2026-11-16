# Coming from tmux

What carries over from tmux to corgi: the keys that match, the ones that differ, and the commands at the `:` prompt.

corgi's default key preset is `tmux`, with the prefix `C-b`. Most of what your fingers know works as it does in tmux. The names map like this:

| tmux | corgi |
|---|---|
| server | herdr-core (`hc`): it keeps the terminals |
| session | workspace |
| window | window |
| pane | pane |
| status line | tab bar (at the bottom by default) |
| `status-right` | usage segments ([usage-status.md](usage-status.md)) |
| `~/.tmux.conf` | `~/.config/corgi/config.toml` (TOML) |

Every workspace is in the sidebar at once. `C-b d` detaches: corgi quits, every program keeps running in core, and running `corgi` again brings it all back.

## Keys that match tmux

After the prefix:

| Key | What it does |
|---|---|
| `c` | New window |
| `n` / `p` | Next / previous window |
| `0`–`9` | Window by number |
| `'` | Window by index (asks) |
| `,` | Rename window |
| `&` | Kill window |
| `%` / `"` | Split right / split down |
| `←` `↑` `↓` `→` | Select pane (repeatable) |
| `o` | Next pane |
| `;` | Last pane |
| `z` | Zoom |
| `x` | Kill pane |
| `{` / `}` | Swap with previous / next pane |
| `!` | Break pane into its own window |
| `Space` | Next layout |
| `M-1`–`M-5` | even-horizontal, even-vertical, main-horizontal, main-vertical, tiled |
| `q` | Pane numbers; a digit focuses that pane |
| `C-←↑↓→` | Resize by 1 (repeatable) |
| `M-←↑↓→` | Resize by 5 (repeatable) |
| `[` / `PageUp` | Copy mode / copy mode a page up |
| `]` | Paste the last copy |
| `$` | Rename workspace (session) |
| `(` / `)` | Previous / next workspace |
| `w` | Choose from the tree |
| `s` | Choose a workspace |
| `d` | Detach |
| `:` | Command prompt |
| `?` | List keys |
| `C-b` | Send `C-b` to the pane |

Repeatable keys work as `bind -r` does: more presses within `keys.repeat_time` (500 ms) need no prefix.

## Keys that differ

| Key | tmux | corgi |
|---|---|---|
| `l` | Last window | Select pane right (`h j k l` select panes, not repeatable) |
| `Tab` | (unbound) | Last window |
| `m` | Mark pane | Menu for the focused pane, its window and workspace |
| `M` | Clear the mark | Machines ([remote-machines.md](remote-machines.md)) |
| `C` | Customize mode | New Window dialog |
| `h` `j` `k` | (unbound) | Select pane left, down, up |

Keys tmux leaves unbound that corgi uses:

| Key | What it does |
|---|---|
| `N` | New Workspace dialog |
| `W` | Git worktrees ([worktrees.md](worktrees.md)) |
| `a` / `A` | Next agent waiting for you / next that finished |
| `e` | The pane's scrollback in `$EDITOR` |
| `u` | Usage details |
| `b` | Toggle the sidebar |
| `R` | Reload the config |
| `C-v` | Paste the clipboard's image |

Other tmux defaults, such as `t` (clock), `i`, `f`, `D`, `L`, `r`, `~`, `#`, `=`, `-`, `.`, `E`, `C-o`, `M-o`, `<`, `>` and `C-z`, aren't bound in corgi.

Other differences:

- After the prefix, a key-hints panel shows what the next key can do. The next key works as normal; `Esc` cancels. Set `keys.hints = "delay"` or `"off"` if you don't want it.
- The mouse is on by default (`ui.mouse = true`): click to focus, wheel to scroll.
- `x` and `&` ask first only when an agent runs in the pane or window; closing a workspace always asks (`ui.confirm_close`).
- Copy mode has vi keys only. There's no emacs copy-mode table yet. `y` or Enter copies to corgi's buffer and to your system clipboard (OSC 52).
- `corgi attach t7` shows one core terminal. It isn't `tmux attach`: plain `corgi` reattaches to everything.

## Your own keys

`[[keys.bind]]` is corgi's `bind`, and `keys.unbind` its `unbind`:

```toml
[keys]
prefix = "C-a"           # set -g prefix C-a
repeat_time = "500ms"    # set -g repeat-time 500
unbind = ["%", "root M-h"]

[[keys.bind]]            # bind | split-window -h
key = "|"
command = "split-window -h"
note = "split right"     # what the key hints call it

[[keys.bind]]            # bind -n M-h select-pane -L
key = "M-h"
table = "root"
command = "select-pane -L"

[[keys.bind]]            # bind -r H resize-pane -L 5
key = "H"
command = "resize-pane -L 5"
repeat = true
```

Keys use tmux notation: `C-b`, `M-Up`, `F5`, `Space`, `BTab`, `PPage`. Tables are `prefix` (the default), `root` (no prefix), `resize`, or your own, entered with `switch-client -T name`. Pressing the prefix twice sends it to the pane. See [keybindings.md](keybindings.md).

### No .tmux.conf import

corgi doesn't read `~/.tmux.conf` and has no importer for it. Copy the bindings you want into `[[keys.bind]]` by hand, using the table above. `source-file` reloads corgi's own config; it doesn't take a file.

## Commands at the `:` prompt

`C-b :` opens the prompt. Tab completes command names, `↑` `↓` go through history, and a mistake shows in the status line with a suggestion (`splt-window`: did you mean split-window?). Words are split shell-style: quotes group, backslash escapes, `--` ends options.

| Command | tmux alias | Options |
|---|---|---|
| `new-window` | `neww` | `-n name`, `-c dir`, a command |
| `split-window` | `splitw` | `-h` / `-v`, `-p 30` or `-l 30` (both percent), `-c dir`, a command |
| `next-window` | `next` | |
| `previous-window` | `prev` | |
| `last-window` | `last` | |
| `select-window` | `selectw` | `-t N` (or `-t :N`) |
| `rename-window` | `renamew` | a name (none: asks) |
| `kill-window` | `killw` | |
| `select-pane` | `selectp` | `-L` `-R` `-U` `-D`, `-l` (last) |
| `last-pane` | `lastp` | |
| `resize-pane` | `resizep` | `-L` `-R` `-U` `-D` and a count, or `-Z` (zoom) |
| `swap-pane` | `swapp` | `-U` / `-D`, or a direction: `-L` `-R`, `up` `down` `left` `right` |
| `break-pane` | `breakp` | |
| `kill-pane` | `killp` | |
| `next-layout` | `nextl` | |
| `select-layout` | `selectl` | `even-horizontal`, `even-vertical`, `main-horizontal`, `main-vertical`, `tiled` |
| `new-workspace` | `new-session`, `new` | `-c dir` |
| `rename-workspace` | `rename-session`, `rename` | a name |
| `kill-workspace` | `kill-session` | |
| `switch-client` | `switchc` | `-n`, `-p`, `-T table` |
| `choose-tree` | | `-s` (workspaces) |
| `detach-client` | `detach` | |
| `send-keys` | `send` | key names (`Enter`, `C-c`) and text |
| `send-prefix` | | |
| `source-file` | `source` | none: reloads corgi's config |
| `command-prompt` | | `-p label`, `-I text`, a template with `%%` |
| `confirm-before` | `confirm` | `-p prompt`, then a command |
| `list-keys` | `lsk` | |
| `display-panes` | `displayp` | |
| `copy-mode` | | `-u` |
| `paste-buffer` | `pasteb` | |
| `display-popup` | `popup` | `-E`, `-w`, `-h`, `-d dir`, `-T title`, a command |
| `run-shell` | `run` | a shell line |
| `display-menu` | `menu` | |

Prompts in `confirm-before` and `command-prompt` expand `#P` (pane), `#I` (window index), `#W` (window) and `#S` (workspace).

Commands act on the focused pane, window and workspace. There are no `-t` targets beyond `select-window -t N`. Some tmux flags are accepted and do nothing: `-d` and `-b` on `split-window`, `-d` on `new-window`, `-d` and `-A` on `new-workspace`.

corgi's own commands, with no tmux counterpart:

| Command | What it does |
|---|---|
| `next-workspace`, `previous-workspace` | As `switch-client -n` / `-p` |
| `next-pane`, `previous-pane` | Cycle panes |
| `rename-pane [name]` | Name the focused pane |
| `next-agent [-b] [-d] [-a] [-p]`, `previous-agent`, `select-agent N` | Jump between agents |
| `toggle-sidebar` | Show or hide the sidebar |
| `dialog new-workspace` / `new-window` / `new-worktree` / `new-machine` | Open a dialog |
| `choose-worktree` (`worktrees`), `choose-machine` (`machines`) | The worktree and machine dialogs |
| `machine <verb> [name]` | See [remote-machines.md](remote-machines.md) |
| `display-usage` (`usage`) | Usage details |
| `paste-image` | Paste the clipboard's image |
| `edit-scrollback` | The scrollback in `$EDITOR` |

[commands.md](commands.md) has every command and flag.

## Related

- [keybindings.md](keybindings.md): every default key, table by table
- [commands.md](commands.md): the command language
- [concepts.md](concepts.md): core vs corgi
- [migrating-from-herdr.md](migrating-from-herdr.md): the other preset
