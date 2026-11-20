# Workspaces and windows

Create, rename, switch between and close workspaces and windows, at once or through a dialog.

A workspace holds windows; a window holds panes. A workspace is what tmux calls a session: corgi
uses another word because a core session (`HC_SESSION`) is something else. See
[concepts](concepts.md) for how the three levels map to tmux and herdr.

Keys below are for the default `tmux` preset with the prefix `C-b`. The `herdr` column is for
`keys.preset = "herdr"`. Every action is also a command you can type at the `C-b :` prompt or bind
to a key (see [custom commands](custom-commands.md)).

## Workspaces

| What | tmux preset | herdr preset | Command |
|---|---|---|---|
| New workspace, with the dialog | `C-b N` | `C-b N` | `dialog new-workspace` |
| New workspace at once | none | none | `new-workspace [-c dir]` |
| Rename the workspace | `C-b $` | `C-b W` | `rename-workspace [name]` |
| Close the workspace | none (menu: `C-b m`, then `X`) | `C-b D` | `kill-workspace` |
| Previous / next workspace | `C-b (` / `C-b )` | none | `switch-client -p` / `-n` |
| Choose a workspace | `C-b s` | `C-b w` | `choose-tree -s` |

`previous-workspace` and `next-workspace` do the same as `switch-client -p` and `-n`. Switching
shows the workspace's name in the status line. corgi remembers which workspace you were in (in
its view file on this machine) and opens there next time.

You can also make a workspace from the tree chooser: its last row, "+ New workspace…", opens the
New Workspace dialog. If you typed a filter that matches nothing, the dialog starts with that text
as the name. See [navigation](navigation.md).

### Quick create vs the dialog

- `new-workspace` starts a shell in a new workspace straight away. Its folder is `-c dir`, else
  the focused pane's current folder, else the folder you started corgi in. It has no key in
  either preset; bind one with `[[keys.bind]]` and `command = "new-workspace"` if you want it.
- `C-b N` opens the New Workspace dialog, where you pick the folder, name, machine and first
  program.

### The New Workspace dialog

| Field | What it does |
|---|---|
| Folder | Where the workspace starts. Defaults to the focused pane's folder (home shown as `~`). Tab completes folder names on this machine; type a `.` to see hidden folders. A folder on this machine must exist. |
| Name | Optional. Empty: the workspace is named after its folder (see below). |
| Machine | "this machine", or another machine core knows, with its state when it isn't connected. Choosing another machine resets Folder to `~` there. See [remote machines](remote-machines.md). |
| Program | What the first pane runs: a shell, a core template (`template: name`), or an agent found on your `PATH` (`claude`, `codex`, `gemini`, `opencode`, `aider`, `amp`, `goose`, `cursor-agent`). |
| Worktree | Off by default. On: the workspace opens in a new git worktree, and the Branch and Path fields appear. See [worktrees](worktrees.md). |

Machines and templates come from core; the lists fill in a moment after the dialog opens.

### How a workspace is named

A name you give it always wins. Without one, a workspace takes the name of the git checkout its
first pane is in now (`~/src/corgi/crates` shows as `corgi`), else the folder's last part. In a
linked git worktree, the repository's name comes first: a worktree at
`~/.corgi/worktrees/corgi/feat-login` shows as `corgi/feat-login`. When that pane changes folder,
the name follows.

To go back to the automatic name, rename it to an empty name.

## Windows

| What | tmux preset | herdr preset | Command |
|---|---|---|---|
| New window at once | `C-b c` | `C-b c` | `new-window [-n name] [-c dir] [cmd]` |
| New window, with the dialog | `C-b C` | `C-b C` | `dialog new-window` |
| Next / previous window | `C-b n` / `C-b p` | `C-b n` / `C-b p` | `next-window` / `previous-window` |
| Last window | `C-b Tab` | none | `last-window` |
| Window 0 to 9 | `C-b 0` … `C-b 9` | `C-b 1` … `C-b 9` (windows 0 to 8) | `select-window -t N` |
| Window by index (prompt) | `C-b '` | none | `select-window -t N` |
| Rename the window | `C-b ,` | `C-b T` | `rename-window [name]` |
| Close the window | `C-b &` | `C-b X` | `kill-window` |

In tmux, `last-window` is on `l`. corgi uses `h j k l` for panes, so it is on `Tab` here.

Clicking a tab selects that window, and clicking the `+` after the tabs makes a new window (see
[mouse](mouse.md)).

### Quick create vs the dialog

- `C-b c` starts a shell in a new window, in the focused pane's folder. `new-window -n logs tail
  -f app.log` names the window and runs a command in it.
- `C-b C` opens the New Window dialog.

### The New Window dialog

| Field | What it does |
|---|---|
| Workspace | Which workspace the window goes in. Shown only when there is more than one; defaults to the current one. |
| Name | Optional. Empty: the window is named after its focused pane. |
| Folder | Where the window starts. Defaults to the focused pane's folder. |
| Program | A shell, a core template, or an agent found on your `PATH`, as in New Workspace. |
| Layout | `single pane`, `2 side by side`, `2 stacked`, `3: main + 2` or `4 tiled`. |

With a layout of more than one pane, the program runs in the first pane and the others are shells
in the same folder. The first pane keeps the focus.

### Window numbers and the tab bar

Windows are numbered from 0 in the order they were made. A new window goes at the end. Closing a
window renumbers the ones after it. Each tab shows `index:name` and a status dot, plus a flag:

| Flag | Meaning |
|---|---|
| `*` | The current window. |
| `-` | The last window (where `C-b Tab` goes). |
| `Z` | The window has a zoomed pane. |

A window without a name of its own is named after its focused pane: the pane's name if you set
one, else its title or the program running in it.

## Dialog keys

The New Workspace, New Window and Rename dialogs share these keys:

| Key | Does |
|---|---|
| `Tab` / `S-Tab`, `↓` / `↑`, `C-n` / `C-p` | Next / previous field. In a folder field, `Tab` completes first and moves on only when there is nothing to add. |
| `Enter` | Create (or rename). |
| `Esc`, `C-c`, `C-g` | Cancel. |
| `←` / `→`, `Space` | Change a select field; `Space` flips a switch. |
| `C-a C-e C-b C-f C-d C-k C-u C-w`, arrows, `Home` / `End` | Edit a text field, emacs style. |

A click on a field focuses it. If a value is wrong (a folder that doesn't exist here, say), the
dialog says why and stays open.

## Renaming

`C-b ,` and `C-b $` open a Rename dialog with the current name filled in. Its placeholder shows
the automatic name you get by leaving it empty. To rename without the dialog, give the name to the
command: `rename-window build` or `rename-workspace api`.

## Closing, and when corgi asks first

`kill-workspace`, `kill-window` and `kill-pane` end the programs in what they close. Whether corgi
asks first is set by `ui.confirm_close`:

| Value | Asks before closing |
|---|---|
| `"auto"` (default) | A workspace, always. A window or pane, only when an agent runs in it. |
| `"always"` | Every time. |
| `"never"` | Never. |

The Close dialog lists each agent that would end and what it is doing (working, waiting for you,
finished). `Enter` or `y` closes; `Esc`, `n` or `q` keeps it.

When the last pane of a window closes, the window goes. When the last window of a workspace goes,
the workspace goes. When the last workspace goes, corgi exits and prints `[exited]`, as tmux
does.

## Not built yet

- Reordering windows or workspaces, by key or by dragging. Windows stay in the order they were
  made; workspaces in the order they were made, with worktree workspaces listed under their
  repository's.

## Related

- [Panes and layouts](panes-and-layouts.md)
- [Navigation](navigation.md): the sidebar, the tree chooser and action menus
- [Worktrees](worktrees.md) and [remote machines](remote-machines.md)
- [Configuration](configuration.md): `ui.confirm_close`, `[keys]`
