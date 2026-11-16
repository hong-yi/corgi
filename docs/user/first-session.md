# Your first session

A walk through one session: start corgi, split panes, open windows, detach and come back, and watch an agent's status.

You need corgi and `hc` installed. See [Install](install.md). The keys below are the default `tmux` preset with the prefix `C-b`. If you use the herdr preset, see [Keys in the herdr preset](#keys-in-the-herdr-preset) at the end.

## Start corgi

Go to a project folder and start corgi:

```sh
cd ~/src/myproject
corgi
```

corgi runs `hc connect` to reach herdr-core. If core isn't running, `hc connect` starts it. If corgi can't reach core, it prints the reason and exits before it takes the screen.

The first time, there is no layout yet. corgi makes a workspace for the current folder and starts your shell in it, in one pane. The screen has three parts:

- **The sidebar**, on the left. The top box, "Workspaces", lists your workspaces with their git branch. The bottom box, "Agents", lists every agent corgi knows about. It says "no agents running" for now.
- **The pane**, in its own box. The row at its top (the breadcrumbs) shows the folder, for example `~ › src › myproject`.
- **The tab bar**, at the bottom. It starts with the mode badge, `NORMAL`, then the window list (`0:zsh*`, where `*` marks the current window), then a `+` button. Usage segments, such as agent counts, appear on the right when they have something to show.

Type into the pane as you would in any terminal. corgi passes every key to the pane, except the prefix.

## Press the prefix and read the hints

Press `C-b`. The badge changes to `PREFIX`, and a key-hints panel opens at the bottom of the screen. It lists every key you can press next, in groups: Windows, Panes, Workspaces, Agents, Other.

You don't need to learn the keys first. Press `C-b`, read, then press the key you want. To close the panel without doing anything, press `Esc`.

To see every binding with its command, press `C-b ?`. Type to filter the list, press `Enter` to run the selected command, or `Esc` to close it.

To send a real `C-b` to the program in the pane, press it twice: `C-b C-b`.

## Split the window into panes

```
C-b %     split right
C-b "     split down
```

Each new pane starts your shell in the same folder as the pane you split. Each pane has its own box, and the focused one is lit.

Move between panes:

```
C-b ←↑↓→      the pane in that direction
C-b h j k l   the same, vi style
C-b o         the next pane
C-b ;         the pane you were in before
C-b q         show pane numbers; press a digit to go to that pane
```

From the left-most pane, `C-b ←` moves the keyboard into the sidebar. `Esc` brings it back to the panes.

The arrow keys repeat: after `C-b ←`, more arrow presses within half a second move again without the prefix. `C-b C-←↑↓→` resizes the pane by one cell, and `C-b M-←↑↓→` by five. Those repeat too.

Make one pane fill the window, then put it back:

```
C-b z     zoom in or out
```

The window's tab shows `Z` while it's zoomed.

`C-b Space` cycles through preset layouts. See [Panes and layouts](panes-and-layouts.md) for more.

## Open more windows

A window is a full screen of panes, like a tab.

```
C-b c       new window (your shell, in the focused pane's folder)
C-b n       next window
C-b p       previous window
C-b 0..9    window by number
C-b Tab     the window you were in before
C-b ,       rename the window
```

Click a tab in the tab bar to select it. `C-b C` opens the New Window dialog instead, where you can pick a name, a folder, a program and a layout.

To start another workspace, press `C-b N`. It opens the New Workspace dialog: a name (optional), a folder, a machine and a first program. `C-b (` and `C-b )` move between workspaces. `C-b s` opens a chooser with every workspace, and `C-b w` one with every window and pane. See [Workspaces and windows](workspaces-and-windows.md) and [Navigation](navigation.md).

## Detach and come back

Start something that keeps running, so you can see it survive. In one pane:

```sh
while true; do date; sleep 1; done
```

Now detach:

```
C-b d
```

corgi exits and you are back at the shell you started it from. The terminals did not stop. They live in herdr-core, not in corgi. Check with `hc`:

```sh
hc ls
```

Every pane you had is still listed as a live terminal.

Start corgi again:

```sh
corgi
```

You get the same workspaces, windows and panes, and the loop is still printing the time. If you close the terminal window corgi runs in, or lose the ssh connection to it, your terminals keep running in the same way. See [Concepts](concepts.md) for why.

## Run an agent and watch its status

A coding agent can report what it is doing to herdr-core with an escape sequence, OSC 7501. Whether yours does depends on the agent and its herdr-core integration. corgi shows that status in several places:

- in the Agents box of the sidebar, with the agent's name and where it runs
- as a dot on its workspace's row in the sidebar, and on its window's tab
- at the end of its pane's breadcrumbs row: a spinner while it works, a red `●` while it waits for you, a green `●` when it finished a turn you haven't seen yet
- as a toast in the top-right corner, when it needs you or finishes while you look elsewhere

Start your agent in a pane, give it a task, then switch to another window with `C-b n`. When it stops to ask you something, its tab and workspace turn red and a toast appears. Press `C-b a` to jump to an agent that's waiting for you, or `C-b A` to one that finished unseen.

You can try this without an agent. A program reports its status by printing an OSC 7501 sequence, so `printf` works:

```sh
printf '\033]7501;state=working:app=demo\033\\'; sleep 5
printf '\033]7501;state=blocked:app=demo\033\\'; sleep 60
```

The first line shows `demo` as working for five seconds. The second shows it as waiting for you. Press `C-c` to stop it: once the shell is back in front, `demo` leaves the Agents box.

See [Agent status](agent-status.md) for what each state means.

## Close panes and quit

A pane closes when its program exits. Type `exit` in a shell, or press `C-b x` to close the focused pane. `C-b &` closes the window. When the program in a pane is an agent, corgi asks before it closes.

When the last pane of the last workspace closes, corgi prints `[exited]` and quits, like tmux.

To leave corgi and keep everything running, use `C-b d` instead.

## Keys in the herdr preset

With `keys.preset = "herdr"` in your config, the prefix is still `C-b`, but some keys differ:

| What | tmux preset | herdr preset |
|---|---|---|
| Split right / down | `%` / `"` | `v` / `-` |
| Focus a pane | arrows, `h j k l` | `h j k l` |
| Next pane | `o` | `Tab` |
| Window by number | `0..9` (from 0) | `1..9` (from 1) |
| Rename window | `,` | `T` |
| Close pane / window | `x` / `&` | `x` / `X` |
| Resize | `C-←↑↓→`, `M-←↑↓→` | `r`, then `h j k l` (`H J K L` by 5), `Esc` to leave |
| Workspace chooser / tree | `s` / `w` | `w` / `g` |
| Detach | `d` | `q` |

The herdr preset has no keys for jumping to agents. Run `next-agent` at the `C-b :` prompt, or bind it yourself. Press `C-b` in either preset to see its keys. See [Key bindings](keybindings.md) and [Migrating from herdr](migrating-from-herdr.md).

## Next

- [Concepts](concepts.md): what lives in core and what lives in corgi
- [Agent status](agent-status.md)
- [Coming from tmux](coming-from-tmux.md)
