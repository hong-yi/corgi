# Troubleshooting

How to find out what's wrong when corgi won't start, keys go astray, or the layout isn't what you expect.

## Start with `corgi doctor`

```sh
corgi doctor
```

It connects the way corgi does (the same `--session` / `--socket`, which go before `doctor`) and prints:

| Line | Tells you |
|---|---|
| `corgi <version>` | Which corgi you ran. |
| `core ✓ herdr-core <version> on "<name>" (session <s>, protocol generation <n>)` | corgi reaches core, and which session. |
| `features: …` | What the core supports. |
| `<n> live terminal(s), <m> other machine(s)` | What core is holding. |
| `layout doc "corgi": version <n>` or `not written yet` | Whether a layout exists in this session. |
| `corgi in core terminals: …` | Every corgi running inside a core terminal; `✗` for one inside a corgi pane. |
| `this doctor runs in core terminal <t>` | When you ran it from inside one. |

`core ✗ <reason>` means corgi can't connect; the reason is the same one corgi prints. `corgi doctor` exits 1 when it can't connect or finds a nested corgi. See [CLI](cli.md#corgi-doctor).

## `hc` not found, or core too old

| Message | Do |
|---|---|
| `` `hc` not found; install herdr-core: curl -fsSL https://releases.herdr.dev/core/install.sh \| sh `` | Install herdr-core with that line (or see [install](install.md)), and make sure `hc` is on your `PATH`. |
| ``core <v> is too old; corgi needs 0.1.19 or newer (run `hc update`)`` | Run `hc update`. |
| `core speaks protocol generation <n>, corgi speaks 1` | corgi and core don't match: update the older one. |
| `core <v> lacks <methods>` | The core is missing methods corgi uses: `hc update`. |
| `socket path too long (use a shorter HC_SESSION)` | Pick a shorter session name. |

corgi connects before it takes the screen, so these print plainly and corgi exits with status 1.

## corgi running inside its own pane

The symptoms: the status line repeated down the screen, a pane that shows corgi showing corgi, keys that go nowhere. A corgi started in one of its own panes shows that pane, which shows corgi, and so on.

corgi refuses to do this. Started in a terminal that corgi shows as a pane, it stops with:

```
refusing to run inside t7, which corgi itself shows as a pane: it would draw itself forever.
Leave this corgi (C-b d in the outer one), or run corgi from a terminal it doesn't manage.
Set CORGI_NESTED=1 to force (the pane t7 then shows a placeholder).
```

It knows which terminal it runs in from `HC_TERMINAL_ID` (set by core), confirmed by its session leader's pid. It also refuses `corgi attach` to its own terminal.

- To run corgi nested on purpose, set `CORGI_NESTED=1`. The pane it runs in then shows "corgi runs in this terminal, so it isn't shown here" instead of itself.
- A pane whose foreground program is corgi gets a warning bar.
- To find a stray one (from an older corgi, or a forced one), run `corgi doctor`: it lists each corgi in a core terminal and marks the bad ones `✗`, with its pid. Leave it with `C-b C-b d` from the corgi showing that pane (the first `C-b C-b` sends the prefix to the inner corgi), or `kill <pid>`.

## Keys going wrong inside a program

- **Esc then a key reads as Alt+key.** Terminals send Alt+x as Esc then x. If you press Esc and then a key quickly, the program may see Alt. Where your terminal supports the kitty keyboard protocol, corgi turns it on, which makes Esc, Alt and `C-i` vs `Tab` unambiguous; `corgi.log` says `kitty_keyboard = true` at startup when it did.
- **Alt doesn't work.** Your terminal must send Alt as Meta (on macOS: "Option as Meta" in Terminal, `macos-option-as-alt` in Ghostty, "Esc+" in iTerm2). corgi's `M-` bindings need the same.
- **A key does something in corgi instead of the program.** Check the `root` table (`C-b ?`, filter `root`): root bindings take the key before the pane. In copy mode and dialogs corgi has the keyboard; the mode badge says which state you are in.
- **tmux inside corgi.** Both use `C-b`. Press `C-b C-b` to send `C-b` to the inner tmux, or give one of them another prefix (`keys.prefix = "C-a"` in corgi).
- **corgi inside tmux.** tmux takes `C-b` first; the same applies the other way round. corgi's notifications and OSC 52 copies are wrapped for tmux, which needs `set -g allow-passthrough on`.

## Colours

- Programs in corgi's panes get `TERM=xterm-256color` and `COLORTERM=truecolor`. If colours look wrong in a pane, the outer terminal may not do true colour; check it outside corgi first.
- Themes colour only corgi's chrome (borders, tab bar, sidebar, dialogs), never your panes. `theme.name = "terminal"` uses your terminal's own palette. With `auto`, corgi picks `light` or `dark` by your terminal's background; if your terminal doesn't answer the query, it uses `dark`.
- Powerline title shapes (`rounded`, `angled`, `slant`) need a Nerd Font; without one, use `breadcrumbs`, `line` or `plain` (see [pane tabs](pane-tabs.md)).

## A pane stuck on a message

| The pane says | Means |
|---|---|
| `attaching t7…` | corgi is opening the terminal's stream. Keys you type are kept and sent once it's up. |
| `t7 lost its program (<why>); waiting for its new shell…` | The terminal is kept (`keep`) and its program was lost (killed, core restarted). Core starts a new shell in it, and corgi reattaches. |
| `herdr-core connection lost · reconnecting…` | corgi lost core and retries every second. |
| `t7 exited (<code>)` | Its program ended. |
| `not attached` | corgi isn't showing it now. |
| A greyed screen with the machine's state | The pane is on another machine whose line is down; it reattaches when the line is back. See [remote machines](remote-machines.md). |

If a pane stays "waiting for its new shell", check the terminal with `hc get t7` and the core with `hc server status`. If reconnecting fails for good, corgi says `can't reconnect: <reason>` (for example, `hc` was removed or the core became too old).

## The layout looks wrong

- **Another corgi changed it.** All corgis on the same core share one layout document (`corgi`), and its focus. When two write at once, corgi takes the other's version; panes it had come back through their [labels](labels.md).
- **"layout document unreadable (…); not saving over it".** The document isn't one corgi can read (written by a newer corgi, or edited by hand). corgi leaves it alone and doesn't save; fix the cause (update corgi) rather than deleting it.
- **A terminal is missing.** Terminals without corgi's labels (started with `hc run`, for example) are never pulled into the layout. View one with `corgi attach <t>`.
- **Backups.** Every document corgi saves is copied to `backups/` in the state folder (the last 20, named `<ms>-v<version>.json`). Not yet: a command to restore one; they are there for recovering a layout by hand.

## Logs

| What | Where |
|---|---|
| The log | `corgi.log` in the state folder: `~/.local/state/corgi/corgi.log` (or `$XDG_STATE_HOME/corgi/`), `%LOCALAPPDATA%\corgi\corgi.log` on Windows. corgi prints its path when it fails to start. |
| More detail | `CORGI_LOG=debug corgi` (any `EnvFilter`, such as `corgi=debug,hc_proto=info`). Key presses are logged at `debug`. |
| What a pane received | `CORGI_RECORD=<dir> corgi` appends every byte each pane receives to `<dir>/<address>-<attach>.bin` (the folder must exist). |

To replay a recording and see the screen it makes (from a corgi checkout):

```sh
cargo run -p corgi-vt --example dump -- <file.bin> <cols> <rows>
```

It prints each row and the cursor. `DUMP_BG="x,y x,y"` also prints those cells' background colours.

When reporting a problem, include `corgi --version`, `corgi doctor`'s output, the relevant part of `corgi.log` at `debug`, and a recording if the problem is in what a pane shows.

## Related

- [FAQ](faq.md)
- [Files and paths](files-and-paths.md)
- [Configuration](configuration.md#errors-and-warnings) for config errors.
