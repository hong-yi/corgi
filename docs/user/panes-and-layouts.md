# Panes and layouts

Split a window into panes, move between them, swap, resize and zoom them, and arrange them with
preset layouts.

Keys below are for the default `tmux` preset with the prefix `C-b`. The `herdr` column is for
`keys.preset = "herdr"`. Each action is also a command for the `C-b :` prompt and for
`[[keys.bind]]` (see [custom commands](custom-commands.md)).

## Splitting

| What | tmux preset | herdr preset | Command |
|---|---|---|---|
| Split right (side by side) | `C-b %` | `C-b v` | `split-window -h` |
| Split down (stacked) | `C-b "` | `C-b -` | `split-window -v` |

The new pane gets half the space, starts in the focused pane's folder, and takes the focus.
`split-window` takes these options:

| Option | Meaning |
|---|---|
| `-h` | Side by side. Without it, the split is stacked (`-v`). |
| `-p N`, `-l N` | The new pane's share, in percent (1 to 99; `30` or `30%`). Unlike tmux, `-l` is a percentage too, not a number of cells. |
| `-c dir` | The folder to start in. |
| `cmd …` | A command to run instead of a shell: `split-window -h -p 30 htop`. |

A pane that runs a command closes when the command ends (see [below](#when-a-program-exits)).

## Moving between panes

| What | tmux preset | herdr preset | Command |
|---|---|---|---|
| Pane to the left / below / above / right | `C-b ←` `↓` `↑` `→`, or `C-b h` `j` `k` `l` | `C-b h` `j` `k` `l` | `select-pane -L` / `-D` / `-U` / `-R` |
| Next pane | `C-b o` | `C-b Tab` | `next-pane` |
| Previous pane | none | `C-b S-Tab` | `previous-pane` |
| Last pane you were in | `C-b ;` | none | `last-pane` (or `select-pane -l`) |
| Show pane numbers, then pick one | `C-b q` | none | `display-panes` |

The arrow keys repeat: after `C-b ←`, more arrow presses within `keys.repeat_time` (500 ms by
default) move again without the prefix. `h j k l` don't repeat, so text you type straight after
isn't taken as a move.

`C-b q` shows each pane's number for one second. Press a digit in that time to focus that pane.

`C-b ←` (or `C-b h`) from the left-most pane moves the keyboard into the sidebar. See
[navigation](navigation.md#the-sidebar).

A click on a pane, or on its title, focuses it (see [mouse](mouse.md)).

## Swapping

| What | tmux preset | herdr preset | Command |
|---|---|---|---|
| Swap with the previous / next pane | `C-b {` / `C-b }` | none | `swap-pane -U` / `-D` |
| Swap with the pane to the left / below / above / right | none | `C-b H` `J` `K` `L` | `swap-pane left` / `down` / `up` / `right` |

As in tmux, `swap-pane -U` and `-D` go by the panes' order, not by direction. `swap-pane -L` and
`-R` swap with the neighbour on that side, the same as `swap-pane left` and `right`. The focus
stays with the pane you were in.

## Resizing

| What | tmux preset | herdr preset | Command |
|---|---|---|---|
| Resize by 1 cell | `C-b C-←` `C-↓` `C-↑` `C-→` | `C-b r`, then `h` `j` `k` `l` or the arrows | `resize-pane -L` / `-D` / `-U` / `-R [n]` |
| Resize by 5 cells | `C-b M-←` `M-↓` `M-↑` `M-→` | `C-b r`, then `H` `J` `K` `L` | `resize-pane -L 5`, … |

`resize-pane` moves the nearest divider beside the focused pane in that direction, by `n` cells
(default 1).

In the tmux preset the resize keys repeat: after `C-b C-←`, press `C-←` again within the repeat
time to keep going.

The herdr preset uses a resize mode instead: `C-b r` switches to the `resize` key table (the badge
says RESIZE), where `h j k l` and the arrows resize by 1 and `H J K L` by 5 until you press `Esc`
or `Enter`. The `resize` table exists in the tmux preset too, with no key to reach it; bind one
with `command = "switch-client -T resize"` if you want it.

## Zoom

| What | tmux preset | herdr preset | Command |
|---|---|---|---|
| Zoom the focused pane, or unzoom | `C-b z` | `C-b z` | `resize-pane -Z` |

A zoomed pane fills the window, and its tab shows `Z`. Zoom needs more than one pane. Moving to
another pane by direction or with next / previous pane, splitting, or choosing a layout ends the
zoom. `C-b ;` (last pane) keeps it: the other pane is shown zoomed instead.

## Preset layouts

| What | tmux preset | herdr preset | Command |
|---|---|---|---|
| Next layout | `C-b Space` | none | `next-layout` |
| `even-horizontal` | `C-b M-1` | none | `select-layout even-horizontal` |
| `even-vertical` | `C-b M-2` | none | `select-layout even-vertical` |
| `main-horizontal` | `C-b M-3` | none | `select-layout main-horizontal` |
| `main-vertical` | `C-b M-4` | none | `select-layout main-vertical` |
| `tiled` | `C-b M-5` | none | `select-layout tiled` |

The layouts are tmux's:

| Layout | Arrangement |
|---|---|
| `even-horizontal` | All panes side by side, equal widths. |
| `even-vertical` | All panes stacked, equal heights. |
| `main-horizontal` | One large pane on top, the rest side by side below it. |
| `main-vertical` | One large pane on the left, the rest stacked beside it. |
| `tiled` | A grid, as even as the number of panes allows. |

`next-layout` goes through them in the order above. The status line names the layout you get.
The window action menu (`C-b m`) has "next layout" on `l` too.

The New Window dialog can start a window with a layout already in place (see
[workspaces and windows](workspaces-and-windows.md#the-new-window-dialog)).

## Break a pane into its own window

| What | tmux preset | herdr preset | Command |
|---|---|---|---|
| Move the focused pane to a new window | `C-b !` | none | `break-pane` |

The new window goes at the end of the window list and becomes the current one. The program in the
pane keeps running. A window with only one pane can't be broken.

Joining a pane into another window (tmux's `join-pane`) is not built yet.

## Naming a pane

| What | tmux preset | herdr preset | Command |
|---|---|---|---|
| Rename the focused pane | none (menu: `C-b m`, then `r`) | none | `rename-pane [name]` |

Without a name, the command opens a Rename dialog. A pane's name is stored on its core terminal,
so every corgi viewing it sees the same name. An empty name goes back to the automatic one: the
pane's title, else the program in front.

## Closing a pane

| What | tmux preset | herdr preset | Command |
|---|---|---|---|
| Close the focused pane | `C-b x` | `C-b x` | `kill-pane` |

This ends the pane's program. Whether corgi asks first is set by `ui.confirm_close`; with the
default `"auto"` it asks only when an agent runs in the pane. See
[workspaces and windows](workspaces-and-windows.md#closing-and-when-corgi-asks-first).

## When a program exits

corgi follows tmux: a pane closes when its program exits. The focus goes to the pane you were in
before, and the space goes to its neighbour. The last pane closing closes the window, and so on up.
When nothing is left, corgi exits and prints `[exited]`.

While a pane isn't showing its program, it says why:

| Message | Meaning |
|---|---|
| `attaching t3…` | corgi is connecting the pane to its terminal. Keys you type now are kept and sent once it's attached. |
| `t3 lost its program (…); waiting for its new shell…` | The terminal is kept by core and is starting a new shell. |
| `herdr-core connection lost · reconnecting…` | corgi lost its connection to core and is retrying. |

See [concepts](concepts.md) for what each of these means and why closing corgi leaves your
programs running.

## Related

- [Workspaces and windows](workspaces-and-windows.md)
- [Navigation](navigation.md): the sidebar, the chooser and action menus
- [Copy mode](copy-mode.md): scroll back through a pane and copy from it
- [Key bindings](keybindings.md): every default key, table by table
