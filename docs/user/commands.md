# Commands

The command language that key bindings, `[[keys.bind]]` and the `C-b :` prompt all run, and every command with its flags.

Commands look like tmux's: `split-window -h -c ~/src`, `select-window -t 3`. You can type any of them at the `C-b :` prompt, bind them to a key (see [keybindings](keybindings.md)), or pick them from `C-b ?`.

## Syntax

### Words

A command line is split into words like a shell does it:

| Rule | Example | Words |
|---|---|---|
| Spaces separate words. | `new-window -n logs` | `new-window`, `-n`, `logs` |
| Single or double quotes group. | `new-window -n "my tests"` | …, `my tests` |
| Inside double quotes, and outside quotes, `\` escapes the next character. | `send-keys "say \"hi\""` | …, `say "hi"` |
| Inside single quotes, `\` is literal. | `'a\b'` | `a\b` |
| Empty quotes are an empty word. | `rename-window ''` | …, `` (empty) |

An unterminated quote or a trailing `\` is an error. There is no `;` to chain commands and no variable expansion: anything shell-like is passed to the shell by the commands that run one.

### Options

| Rule | Example |
|---|---|
| Options are `-` and one letter. | `-h` |
| Letters can be grouped. | `-hd` is `-h -d` |
| An option that takes a value takes the rest of its word, or the next word. | `-p30`, `-p 30` |
| Options end at the first word that isn't one, or at `--`. | `split-window -- -x` runs `-x` |
| An option a command doesn't know is an error. | `split-window -x`: "split-window: unknown option -x" |

Commands that take no options at all ignore any words after them (`next-window anything` is `next-window`).

### Shell commands

`new-window`, `split-window`, `display-popup` and `run-shell` take a shell line after their options. A word you quoted to keep it together (it has spaces or quotes in it) is quoted again for the shell; other words go as typed, so the shell still expands them:

| You type | The shell gets |
|---|---|
| `split-window -h vim 'my file'` | `vim 'my file'` |
| `display-popup -E sh -c 'echo hi; read x'` | `sh -c 'echo hi; read x'` |
| `run-shell echo $((2+3))` | `echo $((2+3))` |

### Mistakes

An error names the problem; at the prompt it shows in the status line. An unknown command suggests the nearest one: `splt-window` gives `unknown command "splt-window" (did you mean split-window?)`.

### `%%` templates

`command-prompt` can take a template: a command with `%%` in it. What you type at the prompt replaces every `%%`, and the result runs. The tmux preset's `'` key uses one:

```
command-prompt -p index "select-window -t ':%%'"
```

Typing `3` runs `select-window -t ':3'`.

### `#` placeholders

`confirm-before`'s prompt text expands these. Nothing else does.

| Placeholder | Becomes |
|---|---|
| `#P` | The focused pane's terminal address (`t7`, `vps/t7`). |
| `#I` | The window's index (from 0). |
| `#W` | The window's name (its own, else its focused pane's name). |
| `#S` | The workspace's name. |

## Windows

### new-window

`new-window [-n name] [-c dir] [-d] [command]` (alias `neww`)

A new window in the current workspace, with a shell, or running `command`.

| Option | Meaning |
|---|---|
| `-n name` | The window's name. |
| `-c dir` | Where it starts (`~` is home). Default: the focused pane's folder. |
| `-d` | Accepted, ignored. |

### next-window, previous-window, last-window

`next-window` (alias `next`), `previous-window` (alias `prev`): the next or previous window, wrapping round. `last-window` (alias `last`): the window you were in before.

### select-window

`select-window -t N` or `select-window N` (alias `selectw`)

Window `N`, counted from 0. A leading `:` is dropped (`-t :3`). No such window: "no such window".

### rename-window

`rename-window [name]` (alias `renamew`)

With a name, sets it; an empty name (`rename-window ''`) goes back to the automatic name. Without one, opens the Rename Window dialog.

### kill-window

`kill-window` (alias `killw`)

Closes the window and ends its programs. Whether it asks first is `ui.confirm_close`'s to say (see [configuration](configuration.md#ui)).

## Panes

### split-window

`split-window [-h|-v] [-p pct|-l pct] [-c dir] [-d] [-b] [command]` (alias `splitw`)

Splits the focused pane, with a shell or `command` in the new one.

| Option | Meaning |
|---|---|
| `-h` | Side by side: the new pane on the right. |
| `-v` | One above the other: the new pane below. The default. |
| `-p pct`, `-l pct` | The new pane's share, 1 to 99 (a `%` sign is allowed: `-l 25%`). Default 50. Both are percentages; `-l` is not a cell count as in tmux. |
| `-c dir` | Where it starts. Default: the focused pane's folder. |
| `-d`, `-b` | Accepted, ignored. |

### select-pane

`select-pane -L|-R|-U|-D|-l` (alias `selectp`)

Focuses the pane to the left, right, above or below; `-l` the last pane. Left from the left-most pane gives the sidebar the keyboard, when the sidebar is shown.

### next-pane, previous-pane, last-pane

`next-pane`, `previous-pane`: the next or previous pane in the window, wrapping round. `last-pane` (alias `lastp`): the pane focused before.

### resize-pane

`resize-pane -L|-R|-U|-D [n]` or `resize-pane -Z` (alias `resizep`)

Moves the focused pane's edge `n` cells (default 1) that way. `-Z` toggles zoom (the focused pane full size; only with more than one pane). While the sidebar has the keyboard, `-L` and `-R` narrow and widen the sidebar instead.

### swap-pane

`swap-pane [-U|-D|-L|-R] [up|down|left|right]` (alias `swapp`)

| Form | Swaps the focused pane with |
|---|---|
| `-U` | The previous pane in order. |
| `-D` | The next pane in order. |
| `-L`, `-R` | Its neighbour on the left, right. |
| `up`, `down`, `left`, `right` | Its neighbour that way. |
| nothing | The next pane in order. |

### break-pane

`break-pane` (alias `breakp`)

Moves the focused pane into a window of its own, in the same workspace. Needs at least two panes.

### kill-pane

`kill-pane` (alias `killp`)

Closes the pane and ends its program. Asks first per `ui.confirm_close`.

### rename-pane

`rename-pane [name]`

Names the focused pane. The name is stored on its core terminal (`corgi.name`), so every corgi viewing it sees it, and it shows in the pane's tab. An empty name goes back to what it runs. Without a name: the Rename Pane dialog.

### next-layout, select-layout

`next-layout` (alias `nextl`): the next preset layout, in the order below, starting from the first.

`select-layout <name>` (alias `selectl`): `even-horizontal`, `even-vertical`, `main-horizontal`, `main-vertical` or `tiled`.

### display-panes

`display-panes` (alias `displayp`)

Shows each pane's number for 1 second; a digit focuses that pane.

## Workspaces

### new-workspace

`new-workspace [-c dir] [-d] [-A]` (aliases `new-session`, `new`)

A new workspace at once, with no dialog, in `dir` (`~` is home; default: the focused pane's folder, else corgi's own). `-d` and `-A` are accepted and ignored. For the dialog, `dialog new-workspace`.

### rename-workspace

`rename-workspace [name]` (aliases `rename-session`, `rename`)

As `rename-window`, for the workspace.

### kill-workspace

`kill-workspace` (alias `kill-session`)

Closes the workspace and ends its programs. With `ui.confirm_close = "auto"` this always asks.

### next-workspace, previous-workspace

The next or previous workspace, wrapping round. Also `switch-client -n` / `-p`.

### switch-client

`switch-client -T table | -n | -p` (alias `switchc`)

| Option | Does |
|---|---|
| `-T table` | Use that key table for the next keys (see [keybindings](keybindings.md#key-tables)). |
| `-n` | Next workspace. |
| `-p` | Previous workspace. |

### choose-tree

`choose-tree [-s] [-w]`

The tree chooser, at the current window; with `-s`, at the current workspace. `-w` is accepted and ignored.

## Agents

### next-agent

`next-agent [-d] [-a] [-p] [-b]`

Goes to the next agent waiting for you, after the focused pane in tree order, wrapping round.

| Option | Meaning |
|---|---|
| `-d` | Agents that finished a turn you haven't seen, instead of those waiting. |
| `-a` | Any agent. |
| `-p` | The one before instead. |
| `-b` | Agents waiting for you (blocked): the default, said explicitly. |

When there's none: "no agent is waiting for you", "no agent has finished unseen" or "no agents running".

### previous-agent

`previous-agent [-d] [-a]`: `next-agent -p`, with the same `-d` and `-a`.

### select-agent

`select-agent N`

The `N`th agent (1 is the first) in the workspace tree's order: workspaces, windows, then panes in the order they were made. This is the sidebar's agent order with `sidebar.agent_sort = "grouped"`.

## Copying and pasting

### copy-mode

`copy-mode [-u]`

Copy mode in the focused pane; `-u` starts a screen up. See [copy mode](copy-mode.md).

### paste-buffer

`paste-buffer` (alias `pasteb`)

Pastes the last copy into the focused pane (bracketed, when the program asks for it). Nothing copied yet: "nothing copied yet (C-b [ to copy)".

### paste-image

`paste-image`

Sends this machine's clipboard image beside the focused pane's terminal (on its machine) and pastes the path it landed at.

### edit-scrollback

`edit-scrollback`

The focused pane's scrollback in a temporary file, opened at its end in `$VISUAL`, else `$EDITOR`, else `vi`, in a popup. The file is removed when the popup closes.

## Running things

### display-popup

`display-popup [-E] [-w W] [-h H] [-d dir] [-T title] [command]` (alias `popup`)

A terminal floating over everything, running `command` (default: your shell) on the current workspace's machine. Every key goes to it; prefix `x` closes it. One popup at a time.

| Option | Meaning |
|---|---|
| `-E` | Close it when the program exits. Without it, it says the program ended and waits for a key. |
| `-w W`, `-h H` | Width and height: a percentage (`60%`, 1 to 100) or cells (3 or more). Default `80%` each. |
| `-d dir` | Where it starts. Default: the focused pane's folder. |
| `-T title` | Its title. Default: the command, else `shell`. |

```
display-popup -E -w 60% -h 20 -T git lazygit --all
```

### run-shell

`run-shell <line>` (alias `run`)

Runs the line in the background on the current workspace's machine (in the focused pane's folder, up to 60 s). Its first line of output shows in the status line; with no output, `<line>: done`; on failure, `<line>: exit <code> <first line of stderr>`.

### send-keys

`send-keys [-l] key…` (alias `send`)

Types into the focused pane. Each word that is a key name in tmux notation (`Enter`, `C-c`, `M-x`, `Up`) is sent as that key; any other word is typed as text. With `-l`, the words are typed as text, a space between them, key names included (`send-keys -l Enter` types the word "Enter").

```
send-keys 'ls -la' Enter
```

### send-prefix

`send-prefix`

Sends the prefix key to the focused pane.

## Dialogs and lists

### dialog

`dialog <name>`

| Name | Opens |
|---|---|
| `new-workspace` (`new-session`, `workspace`) | New Workspace. |
| `new-worktree` (`worktree`) | New Workspace with the worktree switch on, from the current workspace's folder. |
| `new-window` (`window`) | New Window. |
| `new-machine` (`machine`) | Add Machine. |

### choose-worktree

`choose-worktree` (alias `worktrees`): the current workspace's repository's git worktrees. See [worktrees](worktrees.md).

### choose-machine

`choose-machine` (alias `machines`): other machines and their lines. See [remote machines](remote-machines.md).

### machine

`machine <verb> [name]`

Acts on the machine named, else on the current workspace's machine.

| Verb | Does |
|---|---|
| `add` | The Add Machine dialog (no name). |
| `connect` | Dial it now instead of at its next retry. |
| `login` | `hc machine login` in a popup. |
| `update` | `hc machine update` in a popup. |
| `rename` | Rename it (a dialog). |
| `remove`, `rm` | Forget it (asked first). |
| `new-workspace`, `workspace` | The New Workspace dialog, on that machine. |

### display-menu

`display-menu` (aliases `menu`, `menu-display`): the action menu for the focused pane, its window and workspace. See [actions](actions.md#menus).

### display-usage

`display-usage` (alias `usage`): every usage segment's details. See [usage providers](usage-providers.md).

### list-keys

`list-keys` (alias `lsk`): every binding, by table, filterable; `Enter` runs one.

## Prompts

### command-prompt

`command-prompt [-p label] [-I text] [template]`

The prompt in the status line.

| Option | Meaning |
|---|---|
| `-p label` | Its label. Default `:`. |
| `-I text` | Text already typed. |
| `template` | A command with `%%`: what you type replaces it (see [templates](#-templates)). Without one, what you type runs as a command. |

### confirm-before

`confirm-before [-p prompt] command` (alias `confirm`)

Asks y/n, then runs `command` on `y`. The prompt defaults to `are you sure? (y/n)` and expands `#P #I #W #S`.

```
confirm-before -p "kill-pane #P? (y/n)" kill-pane
```

## Other

### detach-client

`detach-client` (alias `detach`): quits corgi. Your programs keep running in core; run `corgi` again to come back.

### toggle-sidebar

`toggle-sidebar`: hides the sidebar, or shows it again in its configured mode (`show` if it's configured `hidden`).

### source-file

`source-file [path]` (aliases `source`, `reload-config`)

Reads the config file again (the one corgi started with). With a path (`~` works), reads that file instead, and keeps using it: `C-b R` reads it again from then on. A path that isn't a file is refused. A broken file keeps the current config, and the reason shows in the status line. See [configuration](configuration.md#reloading).

## All commands

| Command | Aliases |
|---|---|
| `break-pane` | `breakp` |
| `choose-machine` | `machines` |
| `choose-tree` | |
| `choose-worktree` | `worktrees` |
| `command-prompt` | |
| `confirm-before` | `confirm` |
| `copy-mode` | |
| `detach-client` | `detach` |
| `dialog` | |
| `display-menu` | `menu`, `menu-display` |
| `display-panes` | `displayp` |
| `display-popup` | `popup` |
| `display-usage` | `usage` |
| `edit-scrollback` | |
| `kill-pane` | `killp` |
| `kill-window` | `killw` |
| `kill-workspace` | `kill-session` |
| `last-pane` | `lastp` |
| `last-window` | `last` |
| `list-keys` | `lsk` |
| `machine` | |
| `new-window` | `neww` |
| `new-workspace` | `new-session`, `new` |
| `next-agent` | |
| `next-layout` | `nextl` |
| `next-pane` | |
| `next-window` | `next` |
| `next-workspace` | |
| `paste-buffer` | `pasteb` |
| `paste-image` | |
| `previous-agent` | |
| `previous-pane` | |
| `previous-window` | `prev` |
| `previous-workspace` | |
| `rename-pane` | |
| `rename-window` | `renamew` |
| `rename-workspace` | `rename-session`, `rename` |
| `resize-pane` | `resizep` |
| `run-shell` | `run` |
| `select-agent` | |
| `select-layout` | `selectl` |
| `select-pane` | `selectp` |
| `select-window` | `selectw` |
| `send-keys` | `send` |
| `send-prefix` | |
| `source-file` | `source`, `reload-config` |
| `split-window` | `splitw` |
| `swap-pane` | `swapp` |
| `switch-client` | `switchc` |
| `toggle-sidebar` | |

`Tab` at the prompt completes the names in the first column; aliases work when typed in full.

Not yet: `corgi cmd` (running a command in a running corgi from outside), targets (`-t workspace:window.pane`) for commands other than `select-window`, and a settings dialog.
