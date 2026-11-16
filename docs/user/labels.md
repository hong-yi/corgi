# Labels

The `corgi.*` and `herdr.*` labels corgi reads and writes on core terminals, and what each is for.

Labels are key-value pairs core keeps on each terminal. Every client sees them, and they survive corgi restarting. You can see them with `hc get <t>` and set them with `hc label <t> k=v`. Any label can be shown in the sidebar with `{label:KEY}` (see [sidebar tokens](sidebar-tokens.md)).

## corgi's labels

| Label | Value | Written | Read |
|---|---|---|---|
| `corgi.workspace` | The workspace's id (`s1`). | On every terminal corgi starts for a pane; again when a pane moves (`break-pane`) or is adopted. | To put a terminal back in its workspace when the layout document lacks it; to know a terminal is a corgi pane (nesting checks). |
| `corgi.window` | The window's id (`w2`). | With `corgi.workspace`. | To put the terminal back in its window. |
| `corgi.name` | The pane's name. | `rename-pane`; an empty value clears it. | Pane titles, tree rows, window names, menus. Every corgi viewing the pane shows the name. |
| `corgi.popup` | `1` | On a popup's terminal (`display-popup`, `C-b e`). | Marks it as not part of the layout. |
| `corgi.pane` | `main` | No longer written (corgi's first single-pane version did). | A terminal carrying it is adopted into the first workspace, in a window of its own. Counts as a corgi pane for the nesting checks. |

## herdr's labels

| Label | Value | Written | Read |
|---|---|---|---|
| `herdr.seen` | The agent's turn number you last looked at. | While you look at a pane whose agent ended a turn since. | An agent whose turn is newer than `herdr.seen` is "done" (finished unseen); once seen it is "idle". Shared with the herdr app, so looking in either clears it in both. See [agent status](agent-status.md). |

## How labels make the layout recoverable

When corgi reconciles the layout document with core's terminals:

| A live terminal | What happens |
|---|---|
| In the layout already | Nothing. |
| Not in the layout, with `corgi.workspace` and `corgi.window` | Put back in that window (the workspace and window are made again if they're gone). |
| With `corgi.workspace` only | Put in a new window of that workspace, and relabelled. |
| With `corgi.pane` only | Put in a new window of the first workspace, and relabelled. |
| With none of these (started with `hc run`, a popup, another app's) | Left alone: corgi never pulls in a terminal uninvited. |

A pane whose terminal is gone or finished leaves the layout, except a pane on another machine (an address like `vps/t7`): it stays while that machine's line is down.

Not yet: listing the terminals that are left alone ("Unplaced") in the chooser.
