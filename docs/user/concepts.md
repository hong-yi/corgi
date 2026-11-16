# Concepts

How corgi and herdr-core split the work, what corgi's names mean, and where your layout is kept.

## Core runs the terminals, corgi arranges them

corgi has two halves, in two processes:

- **herdr-core** (`hc`) runs your terminals and keeps them alive. It relays their bytes, answers their terminal queries, stores the status agents report, connects to other machines, and stores documents. It draws nothing.
- **corgi** draws everything you see: workspaces, windows, the pane layout, the keys, dialogs, the tab bar, the sidebar, usage segments and themes. It keeps its own screen for each pane and sends your keys to core.

corgi is a viewer. When you detach (`C-b d`), close the terminal window it runs in, or lose your ssh connection, only corgi stops. Every program keeps running in core, and the next `corgi` shows them again.

corgi starts its terminals as *kept* terminals. If the program in a kept terminal is lost instead of exiting (for example, it is killed by a signal, or core is stopped and started again), core starts a new shell in the same terminal. A program that exits normally ends its terminal, and its pane closes.

## What a pane shows

Most of the time a pane shows its program. A message row at the bottom of the pane tells you when it doesn't:

| Message | What's happening |
|---|---|
| `attaching t7…` | corgi is connecting the pane to its terminal. Keys you type now are queued, not lost. |
| `t7 exited (0)` | The program ended, with its exit code. The pane closes straight after. |
| `t7 lost its program (…); waiting for its new shell…` | A kept terminal lost its program. corgi shows the new shell as soon as core starts it. |
| `herdr-core connection lost · reconnecting…` | corgi lost its line to core. It retries every second, then reattaches every pane. The tab bar says `core: reconnecting…` meanwhile. |
| `not attached` | The pane isn't connected to its terminal. |
| `t7: corgi runs in this terminal, so it isn't shown here` | The pane is the terminal this corgi runs in. Showing it would draw corgi inside itself forever. |

A pane on another machine whose connection is down keeps its last screen, greyed, with the machine's state, for how long, and core's reason. It reattaches when the machine is back. See [Remote machines](remote-machines.md).

corgi also notices when core stops answering: after 45 seconds of silence it treats the line as dead and reconnects. A core restart or `hc update` keeps your terminals, their programs and their screens.

## Workspace, window, pane

corgi arranges terminals in three levels:

- A **pane** shows one terminal.
- A **window** is a screen of panes, split into a layout. You see one window at a time.
- A **workspace** is a group of windows, usually for one project folder. It's named after its folder unless you name it.

Compared with tmux and the herdr TUI:

| corgi | tmux | herdr |
|---|---|---|
| workspace | session | space |
| window | window | tab |
| pane | pane | pane |

corgi follows tmux's keys and names, except for "session". In herdr-core, a session is something else (see below), so corgi calls tmux's session a workspace.

A workspace is named after the folder its first pane is in now. Inside a git checkout, it's the checkout's name: a pane in `~/src/corgi/crates` names the workspace `corgi`. A window is named after its focused pane: the title the program set, else the program in front, else the command it started with. You can rename either.

## Terminal addresses

Every terminal in core has an address:

- `t7` is terminal 7 on this machine.
- `vps/t7` is terminal 7 on the machine you added to core as `vps`.

Each pane points at one terminal by its address. Messages and dialogs use the address, for example `claude (t3)`. `hc ls` lists the terminals core runs, with their addresses.

To show a terminal that isn't in your layout, give corgi its address:

```sh
corgi attach t7
```

corgi adds it as a new window in your current workspace (or in a new workspace if you have none), and labels it so it stays in the layout.

## Core sessions and workspaces

A core **session** is a separate herdr-core, with its own socket, its own terminals and its own documents. You pick one with `HC_SESSION` or `--session`. The default is `default`:

```sh
corgi                      # the "default" core session
corgi --session scratch    # another core, with other terminals
HC_SESSION=scratch corgi   # the same
```

Workspaces are corgi's grouping inside one core session. You'll usually have many workspaces and one session. Use another session only when you want a fully separate set of terminals and layout.

`--socket <path>` (or `HC_SOCKET`) connects to a core socket directly instead of running `hc connect`. This works on Unix only.

## Where the layout lives

corgi saves the layout (workspaces, windows, splits, names, the focused pane of each window) as a document named `corgi` in the core on this machine. Core keeps it, so the layout survives corgi quitting and core restarting.

- corgi writes the document with compare-and-swap: a write fails if another corgi wrote first, so two clients never overwrite each other's changes unseen.
- After each write, corgi keeps a copy of the layout. The last 20 are in `~/.local/state/corgi/backups/` (`$XDG_STATE_HOME/corgi/backups/`).
- If the document can't be read, corgi says so and never writes over it.

Some state is yours and stays on this machine, in `~/.local/state/corgi/view-<session>.json`: the workspace you are looking at, how the workspace tree is folded, and the sidebar's width.

### Several corgi clients at once

You can run corgi in several terminals at once, for example one on your desk and one over ssh to the same machine. They connect to the same core, so they all show the same layout. A change in one appears in the others.

As with tmux clients attached to one session, focus is shared. If two clients show the same workspace and you select a window or a pane in one, the other follows. Each client picks its own workspace.

## Labels make the layout recoverable

corgi labels every terminal it starts with where it belongs: `corgi.workspace` and `corgi.window`. A pane's name is the `corgi.name` label. Labels live on the terminals in core, not in the layout document.

When corgi loads the layout, it compares it with core's terminals:

- A pane whose terminal has ended is removed.
- A live terminal labelled for a workspace but missing from the layout is put back: into its window if that still exists (as a new split), else as a new window, in a new workspace if its workspace is gone too.
- A pane on another machine is kept while that machine is unreachable, because its terminals vanish from core's lists until the line is back.

So if the layout document is lost, or a conflict drops a pane, your terminals come back where they were.

Terminals without corgi's labels, such as ones you start with `hc run`, are left alone. corgi never pulls them into your layout uninvited. Use `corgi attach <address>` to add one. A list of these "unplaced" terminals in the chooser is planned but not built yet.

See [Labels](labels.md) for the full list.

## When programs exit

corgi follows tmux:

- A pane closes when its program exits.
- A window closes when its last pane closes, and a workspace when its last window closes.
- When the last pane of the last workspace closes, corgi prints `[exited]` and quits.

Closing a pane yourself (`C-b x`) ends its program. corgi asks first when an agent runs there, and always before closing a workspace. `ui.confirm_close` changes this. See [Configuration](configuration.md).

## Related

- [Agent status](agent-status.md)
- [Files and paths](files-and-paths.md)
- [Troubleshooting](troubleshooting.md)
