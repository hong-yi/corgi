# Architecture

How corgi is split into processes and crates, how data moves between them, and how the one app loop owns all state.

## Who owns what

herdr-core (`hc`) runs the terminals, keeps them alive across restarts, relays their raw output, stores agent
status (OSC 7501), connects other machines, and stores opaque JSON documents. It draws nothing.

corgi is a viewer and layout manager. It owns workspaces, windows and the pane layout, keys, dialogs, the tab
bar, the sidebar, usage segments and themes. Closing corgi, or detaching with `C-b d`, leaves every terminal
running in core. The layout is the core document `corgi` (see [layout-engine.md](layout-engine.md)), so it
survives corgi and is shared by every corgi on that core.

## Processes

```
 your terminal (Ghostty, kitty, iTerm, tmux, ...)
        │ stdin / stdout (raw mode, alternate screen)
┌───────▼──────────── corgi ─────────────────────┐
│ input thread ─► app task (tokio, current thread) │
│                 ▲   │                            │
│     hc-proto reader/writer tasks                 │
└─────────────────┼───┼────────────────────────────┘
                  │   │ framed JSON-RPC + streams over stdio
            ┌─────┴───▼─────┐
            │  hc connect   │  child process (or a Unix socket with --socket)
            └───────┬───────┘
                    │
            ┌───────▼───────┐   machines   ┌──────────────┐
            │  herdr-core   │ ───────────► │ remote core  │  (addresses like vps/t7)
            │  terminals    │              └──────────────┘
            └───────────────┘
```

- `hc connect` is spawned by `hc_proto::Transport::hc` (`crates/hc-proto/src/client.rs`). It starts the core if
  needed and relays bytes unchanged. `--socket` (`Transport::Socket`) talks to a core socket directly.
- Remote terminals are reached through core's machines. corgi never opens ssh itself; `hc machine …` runs in a
  popup when it must ask for a password (`crates/corgi/src/app/machines.rs`).

## Crates

| Crate | Path | Role |
|---|---|---|
| `hc-proto` | `crates/hc-proto` | The core client: framing (`wire.rs`), connection, calls, attach and event streams (`client.rs`), the pure events reducer (`events.rs`), record types (`types.rs`), the hello gate (`lib.rs::check`). No UI dependencies. See [protocol.md](protocol.md). |
| `corgi-vt` | `crates/corgi-vt` | One terminal emulator per pane: libghostty-vt built with Zig and bound with bindgen. Bytes in, `Frame` of cells out; key, mouse and paste encoders. See [terminal-emulation.md](terminal-emulation.md). |
| `corgi` | `crates/corgi` | The binary: config, model, layout, keys, commands, drawing, dialogs, usage, persistence. |

Inside `crates/corgi/src`:

- `main.rs`: the CLI (`clap`): `doctor`, `attach <addr>`, `migrate`, `claude-statusline`, `--session`,
  `--socket`, `--config`, `--default-config`.
- `app/`: the `App` and its loop. `mod.rs` holds state, `Msg`, the loop, connection handling, spawning and pane
  geometry; `actions.rs` runs an `Action`; `keyboard.rs` routes keys by `Mode`; `draw.rs` draws; the other files
  are one feature each (`dialogs`, `menus`, `copymode`, `popup`, `worktrees`, `machines`, `usage`, `notify`,
  `paste`, `mouse`, `narrow`).
- `pane.rs`: one pane, its `Vt` and its attach stream.
- Pure modules beside `app/`: `layout.rs`, `model.rs`, `keys.rs`, `commands.rs`, `form.rs`, `menu.rs`, `copy.rs`,
  `chooser.rs`, `tree.rs`, `sidebar.rs`, `pane_title.rs`, `status.rs`, `toast.rs`, `keyhints.rs`, `keylist.rs`,
  `usage.rs`, `machines.rs`, `worktrees.rs`, `migrate.rs`.
- IO helpers: `term.rs` (the host terminal guard), `display.rs` (host colour query), `persist.rs` (local files),
  `process.rs` (bounded program runs), `usage_sources.rs`, `clipboard.rs`, `host_notify.rs`, `paths.rs`.

## Data flow

```
 crossterm::event::read ──► Msg::Input ─┐
 Pane attach task ───────► Msg::Stream ─┤      ┌──────────────────────────┐
 events task ────────────► Msg::Event ──┼────► │ App::handle(msg)          │
 doc_get / doc_set tasks ► Msg::DocLoaded, ───┤ │  updates Doc, terminals,  │
                           Msg::Saved          │  panes, mode, toasts      │
 terminal.run tasks ─────► Msg::Spawned ──┤    └────────────┬─────────────┘
 machine.list poll ──────► Msg::Machines ─┤                 │ dirty
 usage loops ────────────► Msg::Usage ────┤                 ▼
 git scans ──────────────► Msg::Branches ─┘    term::draw ─► App::draw(frame)
                                                    │
 calls out: Client::call (terminal.run, terminal.label, doc.set, machine.exec, ...)
            AttachTx::input / resize / focus / credit / automatic_input
```

Everything that does IO runs in a spawned task or thread and reports back with a `Msg`
(`crates/corgi/src/app/mod.rs::Msg`). The app task never awaits IO itself. It sends with `Client::call` from a
`tokio::spawn`, or through the non-blocking `AttachTx` methods, which only queue a frame for the writer task.

## The app loop

`app::run` builds a current-thread tokio runtime and runs `main`. `main` connects first (so errors print plainly
before the screen is taken), checks the nesting guard (`crate::nesting`), enters the terminal guard
(`term::Guard::enter`), starts the input thread (`spawn_input`) and the usage loops, and calls `App::connected`.

Each pass of the loop in `main`:

1. If something changed (`dirty`) and a frame interval (`FRAME`, 8 ms) has passed, draw once with `term::draw`,
   inside a synchronized update. Then ring the host bell if a pane rang, and update the host window title.
2. Work out how long to sleep: until the next frame when dirty, else until the next `TICK` (10 s). Shorten it for
   timers that change the screen: the key-hints delay, the status message expiry (`NOTE_FOR`), a pane's
   synchronized-output hold, toasts, the spinner, the repeat window, the Machines dialog's clock.
3. `tokio::select!` on the message channel or the sleep.
   - A message: `App::handle` it, then drain more for at most one `FRAME`, so a flooding pane can't hold the
     screen or the keys up. Then credit every pane's used output back to core (`Pane::credit`) and save the
     layout if it changed (`App::save`).
   - The sleep: mark dirty only for what the timer changes (toasts expiring, hints opening, a hold ending).
4. Expire the status message, and run `App::tick` (git facts) when `TICK` has passed, whatever woke the loop.

`App::handle` returns whether the screen can have changed, so unchanged machine polls or git scans don't redraw.

On quit the loop waits up to 1 s for a last `Msg::Saved` and calls `persist::flush`, so the final layout and view
reach core and disk.

### One state owner

`App` (in `app/mod.rs`) holds everything: the `Client`, the `Doc` and its version, core's terminal records, the
attached `Pane`s, the `Mode`, the config and theme, toasts, usage readings, machines. Only the app task touches it.
There are no locks around app state; background work gets clones of what it needs (`Client` is a cheap clone) and
answers with a `Msg`.

### Actions

Every key binding, `[[keys.bind]]` entry, `:` prompt line, menu entry and list-keys row is a command string.
`commands::parse` turns it into an `Action`, and `App::run` (`app/actions.rs`) carries it out. Mouse clicks and
dialog submits call the same `App` methods. See [input-and-keymaps.md](input-and-keymaps.md).

### Render on dirty

Drawing is a function of state. `App::draw` (`app/draw.rs`) computes the screen areas (`App::areas`), draws the
sidebar, the tab bar, the panes from each `Vt::frame()`, and the overlay for the current `Mode`. It records click
targets in `App::hits` for the mouse. ratatui diffs the buffer, so only changed cells reach the host.

## Panes attach only while visible

`App::sync` (`app/mod.rs`) runs after anything that changes what's on screen. It keeps a `Pane` only for the
panes of the visible window (and the popup), resizes each to its rectangle, sets focus, and attaches panes that are
`Offline` when the line is up and their machine isn't down. Panes in other windows are dropped, which detaches them;
they attach again when shown, and core's first paint restores their screen.

## Deciding code stays separate from IO

Code that decides (layout, model reconciling, keymap, command parsing, usage placement, forms, menus, copy-mode
motions, the tree, status rollup, key-hints layout) is pure: no processes, files or core. It takes values and
returns values, and it has unit tests in the same file. The app calls it and does the IO.

When adding a feature, put the decision in a pure module with tests and keep the `App` side thin: gather inputs,
call the function, act on the result. Examples:

- `model::Doc::reconcile(&terminals)` returns a `Reconciled` (removed, adopted, relabel); `App::reconcile` makes
  the calls to `terminal.rm` and `terminal.label`.
- `usage::place(free, &segments)` decides what fits; `App::usage_placed` supplies readings and draws.
- `chooser::Chooser::key` returns an `Outcome`; `app/keyboard.rs` acts on it.

## Threads besides the app task

- The input thread blocks on `crossterm::event::read` and sends `Msg::Input`.
- `persist.rs` writes view files and backups on its own thread (`corgi-persist`), coalescing views.
- Usage sources that read files or run programs run on tokio's blocking pool (`spawn_blocking`).
- `process.rs` uses two short-lived threads per run (output reader, waiter).

## Further reading

- [protocol.md](protocol.md): how corgi talks to core.
- [optimizations.md](optimizations.md): performance work and its measurements.
- [adr/](adr/): the decisions behind the emulator, the client and the layout document.
