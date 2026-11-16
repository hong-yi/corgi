# FAQ

Short answers to common questions, with links to the pages that explain more.

## Does closing corgi stop my programs?

No. Your programs run in herdr-core's terminals, not in corgi. `C-b d` (or closing the terminal window) leaves them running; `corgi` again shows them where they were. A pane closes only when its program exits; after the last one corgi prints `[exited]`. See [concepts](concepts.md).

## Can I run corgi in two terminals at once?

Yes. Every corgi on the same core session shares one layout (the core document `corgi`), so both show the same workspaces, windows and panes, and the same focus, as tmux clients do. Which workspace each one is looking at, and its folds and sidebar width, are kept per client in `view-<session>.json`. See [files and paths](files-and-paths.md).

## How do I keep separate sets of workspaces?

Use another core session: `corgi --session work` (or `HC_SESSION=work corgi`). Each session has its own terminals and its own layout.

## Where is my config?

`~/.config/corgi/config.toml` on Linux and macOS (or `$XDG_CONFIG_HOME/corgi/`), `%APPDATA%\corgi\config.toml` on Windows, or wherever `--config` / `CORGI_CONFIG` points. It doesn't need to exist. `corgi --default-config` prints every key with its default. See [configuration](configuration.md).

## I changed the config; do I need to restart?

No: `C-b R` reloads it. Only `ui.mouse` needs a restart. A broken file keeps the config you had, and the status line says why.

## I don't remember the keys.

Press `C-b` and wait: the key-hints dialog lists what the next key does. `C-b ?` lists every binding and lets you filter and run one; `C-b :` runs any [command](commands.md) by name, with `Tab` completion. `C-b m` opens a menu for the focused pane.

## Can I use `C-a` as the prefix?

```toml
[keys]
prefix = "C-a"
```

`C-a C-a` then sends `C-a` to the pane.

## Can I have the old herdr TUI's keys?

Yes: `keys.preset = "herdr"`, or `corgi migrate` to carry your herdr config over. See [keybindings](keybindings.md#the-herdr-preset) and [migrating from herdr](migrating-from-herdr.md).

## Does corgi read my `.tmux.conf`?

Not yet. The commands are tmux's where corgi has them, so most `bind` lines translate to `[[keys.bind]]` directly. See [coming from tmux](coming-from-tmux.md).

## Why doesn't my agent show a status?

corgi shows status only from what a program reports itself (OSC 7501); it doesn't read the screen to guess. A program that never reports has no status dot and no summary in its pane's title. See [agent status](agent-status.md).

## What does "done" mean, and why did it turn into "idle"?

"Done" is an agent that finished a turn you haven't looked at. Looking at its pane marks it seen (the shared `herdr.seen` label), and it becomes idle. See [labels](labels.md).

## Why does corgi refuse to start?

Most often it's being started inside one of its own panes, where it would draw itself forever; the message says so, and `CORGI_NESTED=1` forces it. Otherwise it can't reach core (no `hc`, or a core that's too old), or the config is broken. The message says which, and `corgi doctor` checks the connection. See [troubleshooting](troubleshooting.md).

## Can corgi show my Claude Code limits?

Yes, if you put `corgi claude-statusline -- <your command>` in front of Claude Code's status line: Claude Code gives its limits only to that command. Without it the `claude` segment shows tokens, or an estimate if you set `limit`. See [usage providers](usage-providers.md#claude).

## Is there an emacs copy mode, or can I rebind copy-mode keys?

Not yet. Copy mode has vi keys only, and they are fixed. See [keybindings](keybindings.md#copy-mode).

## Can other programs control corgi (`corgi cmd`, plugins)?

Not yet. Remote control and plugins are planned; today, commands run from keys, the `:` prompt, `C-b ?` and menus.

## Does corgi work on Windows?

The paths are set for Windows (`%APPDATA%`, `%LOCALAPPDATA%`), but Windows support is not finished yet: `--socket` is Unix only, and the Windows console's key encoding isn't done.
