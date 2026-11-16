# Remote machines

Run workspaces on other machines that your herdr-core reaches over ssh, and see what happens when a line drops.

corgi doesn't connect to other machines itself. herdr-core does: you add a machine to core, core keeps a line to the `hc` there, and corgi shows that machine's terminals beside your local ones. A terminal on another machine has an address with the machine's name in front: `vps/t7` instead of `t7`.

## Add a machine

You can add a machine from inside corgi or from a shell. Both end up in the same place: core's list of machines.

### From corgi

Open the Add Machine dialog with any of these:

| How | What |
|---|---|
| `C-b M`, then `n` (or `a`) | From the Machines dialog |
| `C-b :` `machine add` | The command |
| `C-b :` `dialog new-machine` | The same dialog by name |
| `m` on a machine row in the sidebar, then `a` | From the row's menu |

The fields:

| Field | What to put |
|---|---|
| Reach it | `over ssh` (the default) or `with a command` |
| Target | With ssh: `user@host`, `host:2222`, or a `Host` from `~/.ssh/config` |
| Command | With a command: what reaches the machine, e.g. `fly ssh console -a app --pty=false -C`. It replaces Target. |
| Name | What to call it here. Optional over ssh (default: the host's name). Needed with a command. |
| Session | The `hc` session on that machine. Empty: its default. |
| Temporary | Mark it short-lived (a sandbox for one task). |

Press Enter to add it. The dialog says `reaching …` while core dials. When it's done, the Machines dialog opens with the new machine selected. If core can't reach it, its error shows in the dialog.

ssh may need to ask you something: a password, a host key, or whether to install `hc` there. The dialog can't answer those. Press `C-t` instead of Enter: corgi runs the same `hc machine add …` line in a popup terminal, where ssh can ask.

### From a shell

```sh
hc machine add vps
hc machine add --name box me@10.0.0.7:2222
hc machine add --via 'fly ssh console -a app --pty=false -C' --name fly
```

Run it against the same core session corgi uses (`HC_SESSION`, `default` unless you started corgi with `--session`). corgi asks core for its machines every 2.5 seconds, so the new one shows up without a restart. `hc machine add --help` lists every option.

## The Machines dialog

`C-b M` (`choose-machine`) lists every machine core knows: its line's state, its target, and tags such as `session scratch`, `temporary`, `hc 0.1.19` and `2 workspaces`. Under the list, it says what's wrong with the selected machine when its line isn't up.

| Key | What it does |
|---|---|
| `j` `k`, `↑` `↓`, `C-n` `C-p` | Move |
| Enter or `c` | Connect now, instead of waiting for core's next retry |
| `n` or `a` | Add a machine |
| `w` | New Workspace dialog on this machine, starting in its home folder |
| `r` | Rename it here |
| `d` or Delete | Forget it (asked first) |
| `l` | Run `hc machine login` in a popup |
| `u` | Run `hc machine update` in a popup |
| Esc, `q`, `C-c`, `C-g` | Close |

Renaming changes only what this core calls the machine. Its panes' addresses change with it (`old/t3` becomes `new/t3`). Names can't contain spaces or slashes.

Forgetting a machine closes its workspaces in corgi. Its programs keep running on that machine.

### Machine commands

Every action in the dialog is also a command, so you can bind it or run it at the `C-b :` prompt. Without a name, they act on the current workspace's machine.

| Command | What it does |
|---|---|
| `machine add` | Open the Add Machine dialog |
| `machine connect [name]` | Dial it now |
| `machine login [name]` | `hc machine login` in a popup |
| `machine update [name]` | `hc machine update` in a popup |
| `machine rename [name]` | Ask for a new name |
| `machine remove [name]` (or `rm`) | Forget it, after asking |
| `machine new-workspace [name]` | New Workspace dialog on it |
| `choose-machine` (alias `machines`) | The Machines dialog |

The menu on a machine's sidebar row (`m` on the row, or a right-click) has the same entries.

## Machine states

| Glyph | State | What to do |
|---|---|---|
| `●` | connected | Nothing |
| `◌` | connecting | Wait, or Enter in `C-b M` to dial now |
| `!` | needs login | `l` in `C-b M`: ssh wants a password or code again |
| `!` | needs update | `u` in `C-b M`: `hc` there is missing or too old |
| `✕` | unreachable | Check the network or the target; core keeps retrying |

## Workspaces on another machine

Pick the machine in the New Workspace dialog's Machine field (`C-b N`). Choosing another machine resets Folder to `~`, its home there. `w` in `C-b M` does the same in one step.

Once any other machine exists, the sidebar and the tree chooser (`C-b w`) group workspaces by machine. This machine comes first, marked `⌂` with its hostname. Each other machine follows with its state glyph. Fold a machine like any other row.

Everything you start in a remote workspace starts on that machine:

- new windows and splits
- popups (`display-popup`) and `run-shell`
- git worktrees (see [worktrees.md](worktrees.md))

Git facts come from that machine too. Every 10 seconds, corgi runs one git query per machine whose line is up, through core. That gives remote workspaces their branch, their checkout name, and their worktree grouping in the sidebar.

## While a machine is down

core sends no event when a line drops. corgi notices on its next 2.5-second poll.

- Panes on that machine keep their last screen, greyed out. Their bottom line says the state, for how long, and core's reason: `vps unreachable for 3m · its last screen · cannot reach: timed out`.
- They aren't attached while the line is down, and corgi doesn't drop them from the layout.
- When the line comes back, they reattach on their own.
- When a machine that has workspaces goes down, a toast names it and its state (`vps unreachable`), with core's reason. When it comes back, a toast says `vps is back`.
- Usage segments that follow the workspace keep their last reading and dim once it's old (see below).

## Usage segments on the workspace's machine

The `command`, `host` and `system` usage segments can read on the active workspace's machine instead of this one. Set `on`:

```toml
[[usage.segment]]
provider = "command"
cmd = "git status --porcelain | wc -l"
on = "workspace"   # "here" (this machine) or "workspace"
```

`host` and `system` default to `on = "workspace"`; `command` defaults to `here`. When you switch to a workspace on another machine, these segments read again at once. Their details in `C-b u` start with `on vps`. The `claude`, `codex` and `clock` segments always read on this machine. See [usage-status.md](usage-status.md).

## Pasting images into remote panes

A program on another machine can't see this machine's clipboard or files. corgi bridges that for images:

- **Clipboard image.** When your clipboard holds only an image, most terminals send an empty paste. corgi reads this machine's clipboard image instead.
- **Dropped file.** Dropping an image file on the terminal pastes its path. corgi reads the file instead.

Either way, corgi sends the image to core, which puts it beside the pane's terminal on its machine. corgi then pastes the path where it landed. The status line says `pasted an image: <path>`.

`C-b C-v` (`paste-image`) does this on purpose, for any pane, local or not.

| `[paste]` key | Default | What it does |
|---|---|---|
| `images` | `"remote"` | When an empty paste or a dropped image file is bridged: `"remote"` (panes on another machine), `"always"` (every pane) or `"off"`. `C-b C-v` works in every case. |
| `image_command` | unset | A shell line that prints the clipboard's image, e.g. `"pngpaste -"` |

Without `image_command`, corgi reads the clipboard with `osascript` on macOS, and with `wl-paste` (Wayland) or `xclip` on Linux. On other systems, set `image_command`.

Limits: PNG, JPEG, GIF, WebP and BMP, recognised by their bytes; up to 8 MB; one image at a time. A dropped path must be an absolute path to an existing file with one of those extensions.

## Instead: run corgi on the other machine

corgi always talks to the core on the machine it runs on (it starts `hc connect`, or uses `--socket`). It has no option to reach a core through `ssh host hc connect`. If you'd rather use a machine's core directly, ssh there and run corgi there:

```sh
ssh -t vps corgi
```

That corgi shows the machine's own terminals and layout. At 64 columns or fewer (a phone, say), it switches to the narrow layout.

## Related

- [worktrees.md](worktrees.md): worktrees on another machine
- [usage-status.md](usage-status.md): the `on` setting
- [concepts.md](concepts.md): terminal addresses and core sessions
- [configuration.md](configuration.md): `[paste]` and every other key
