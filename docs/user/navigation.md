# Navigation

Find and jump to any workspace, window, pane or agent: the sidebar, the tree chooser, agent jumps,
action menus, and the narrow layout for small screens.

Keys below are for the default `tmux` preset with the prefix `C-b`; herdr-preset keys are noted
where they differ.

## The workspace tree

The sidebar and the tree chooser show the same tree: workspaces, their windows, and the panes of
windows that have more than one. Each row has a status dot, rolled up from the panes below it (see
[agent status](agent-status.md)). Once core knows other machines, machines are the top level: this
machine first, then each other one with the state of its line (see
[remote machines](remote-machines.md)).

Folding is shared. Workspaces start open and windows start closed; a window with one pane has
nothing to fold. Folding a row in the chooser folds it in the sidebar too, and the other way
round. corgi saves what is folded in its view file, per client.

## The sidebar

The sidebar sits on the left: the tree on top, and every running agent below it (with
`sidebar.agents = "split"`, the default). `C-b b` (`toggle-sidebar`) hides and shows it. It hides
on its own when the terminal is under 60 columns wide. See [configuration](configuration.md) for
`[sidebar]` and [sidebar tokens](sidebar-tokens.md) for what its rows show.

With the mouse, click `▾` or `▸` to fold a row, and click a row or an agent to go there.

### Giving the sidebar the keyboard

Press `C-b ←` (or `C-b h`) in the left-most pane. The sidebar's border lights up, the badge says
SIDEBAR, and a cursor appears on the current window's row. Keys now go to the sidebar, not the
pane:

| Key | Does |
|---|---|
| `↑` `↓`, `k` `j` | Move the cursor. |
| `Home` / `End` | First / last row. |
| `→` `l` / `←` `h` | Open / close the row. `←` on a closed or leaf row goes to its parent. |
| `Tab` | Open or close the row. |
| `Enter` | Go to that workspace, window or pane. The keyboard goes back to the panes. |
| `m` | The action menu for that row (see [below](#action-menus)). |
| `Esc`, or `C-b →` | Back to the panes. |

The sidebar doesn't filter: typed letters do nothing here. Use the chooser to filter. The prefix
still works. Prefix keys other than moves and resizes hand the keyboard back to the panes and then
run as usual. The agents list is reached with the mouse or the agent keys below, not with the
sidebar cursor.

### Resizing the sidebar

While the sidebar has the keyboard:

| Key | Does |
|---|---|
| `C-b C-→` / `C-b C-←` | Wider / narrower by 1 column (repeats). |
| `C-b M-→` / `C-b M-←` | Wider / narrower by 5 columns (repeats). |

In the herdr preset, use resize mode: `C-b r`, then `l` / `h` (by 1) or `L` / `H` (by 5).

With the mouse, drag the sidebar's right edge. Either way, the width is kept between 12 columns
and the screen width less 20, saved per client, and used instead of `sidebar.width` from then on.

## The tree chooser

| What | tmux preset | herdr preset | Command |
|---|---|---|---|
| The tree, cursor on the current window | `C-b w` | `C-b g` | `choose-tree` |
| The tree, cursor on the current workspace | `C-b s` | `C-b w` | `choose-tree -s` |

The chooser shows the whole tree in a dialog over the window, about three quarters of the screen.
The badge says CHOOSE.

| Key | Does |
|---|---|
| `↑` `↓`, `C-p` `C-n`, `C-k` `C-j` | Move. |
| `Home` / `End` | First row / the "+ New workspace" row. |
| `→` / `←` | Open / close the row. `←` on a closed or leaf row goes to its parent. |
| `Tab` | Open or close the row. |
| Typing | Filter. |
| `Backspace`, `C-u` | Delete a character / clear the filter. |
| `Enter` | Go there. |
| `Esc` | Clear the filter; with no filter, close. `C-c` and `C-g` close. |

The filter ignores case. It matches a workspace's name and git branch, a window's `index:name`,
and a pane's address, agent and title. Matches are shown with their parents, unfolded.

Below the tree, "+ New workspace…" opens the New Workspace dialog. Move past the last row (or
press `End`) to reach it. When the filter matches nothing, the cursor is already there, and the
dialog starts with the filter as the workspace's name.

## Jumping to agents

| What | tmux preset | herdr preset | Command |
|---|---|---|---|
| Cycle through agents waiting for you | `C-b a` | none | `next-agent -b` |
| Cycle through agents that finished and you haven't seen | `C-b A` | none | `next-agent -d` |
| Cycle through every agent, any state | none | none | `next-agent -a` |
| The agent at place N in the sidebar's list | none | none | `select-agent N` (1 is the first) |

`next-agent` on its own picks agents waiting for you; `-d` picks those that finished unseen; `-a`
picks any agent. `-p` or `-b` goes backwards, as does `previous-agent` (which takes `-d` and `-a`
too). Jumps go in the sidebar's agent order, starting from the focused pane, and wrap round. When
none matches, the status line says so ("no agent is waiting for you"). To put these on keys, see
[custom commands](custom-commands.md).

## Action menus

| What | tmux preset | herdr preset | Command |
|---|---|---|---|
| Menu for the focused pane, its window and workspace | `C-b m` | `C-b m` | `display-menu` |

You also get a menu with `m` on a row of the focused sidebar, or by right-clicking a pane, a tab or
a sidebar row. A pane's menu has Pane, Window and Workspace sections; a window's has Window and
Workspace; a workspace's has Workspace only. The badge says MENU.

| Section | Key: entry |
|---|---|
| Pane | `%` split right, `"` split down, `z` zoom, `r` rename pane…, `!` break into its own window, `x` close pane |
| Window | `c` new window, `C` new window…, `,` rename window…, `l` next layout, `&` close window |
| Workspace | `N` new workspace…, `W` new worktree workspace…, `$` rename workspace…, `X` close workspace |

A machine row's menu has the machine's own actions (see [remote machines](remote-machines.md)).

In a menu, press an entry's key to run it at once, or move with `j` `k` (or the arrows, `Tab`,
`C-n` `C-p`) and press `Enter`. `Esc`, `C-c` or `C-g` closes it, as does a mouse click anywhere.
A menu opened for another row first goes to that row, then runs the entry there.

## The narrow layout

On a small screen (a phone over ssh, a narrow split), corgi switches to a layout made for it. It
starts at `ui.narrow_width` columns or fewer: 64 by default; `0` turns it off. Make the terminal
wider again and the usual layout comes back.

In the narrow layout:

- There is no sidebar. Panes have no boxes, gaps, insets or titles.
- Only the focused pane shows, as if zoomed. Move between panes with the usual keys.
- A two-row header at the top takes the tab bar's place.
- The chooser (`C-b w`) fills the whole screen.

The header's first row shows the mode badge (unless it is NORMAL), the workspace with its status
and machine, the window as `index:name` and its place (`2/3`), and the pane's place (`pane 1/2`)
when there are several. The second row shows a message, or the agents everywhere ("1 waiting · 2
working"), and usage segments in the space left. On the right, a "switch" button opens the chooser
when you tap or click it; it is marked when an agent is waiting for you.

## Not built yet

- Status filters in the chooser (show only waiting, working, idle, done agents).
- An "Unplaced" list of core terminals that aren't in any window.
- A preview of the selected pane in the chooser.
- Peek: opening an agent's worker terminal as a popup.

## Related

- [Workspaces and windows](workspaces-and-windows.md)
- [Panes and layouts](panes-and-layouts.md)
- [Mouse](mouse.md)
- [Agent status](agent-status.md)
