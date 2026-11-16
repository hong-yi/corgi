# How corgi uses herdr-core

The client in `crates/hc-proto`, and what the app does with it: hello, framing, credit, attach, events, recovery, documents and machines.

The protocol itself is herdr-core's: read `schema/README.md` and `schema/hc.openrpc.json` in the
[herdr-core repository](https://github.com/herdrdev/herdr-core). This page covers corgi's side. The reasons for
writing our own client are in [ADR 0002](adr/0002-hc-proto.md).

## Connecting and hello

`Client::connect(&Transport)` (`client.rs`):

1. Opens the transport. `Transport::Spawn` runs `hc connect` with `HC_SESSION` set (`Transport::hc`); a missing
   `hc` gives `Error::Connect` with the install command. `Transport::Socket` opens a Unix socket.
2. Sends one line-mode JSON-RPC request: `hello {framed: true, generation: 1}`, and waits up to 15 s
   (`HELLO_TIMEOUT`) for the reply line. If `hc connect` exits first, its stderr is parsed by `describe`, which
   prefers core's own `{"error":{"message","hint"}}`.
3. Runs `hc_proto::check(&hello)` (`lib.rs`). It fails with `Error::Mismatch` when:
   - `generation` isn't `GENERATION` (1);
   - `version` is older than `MIN_CORE` (`"0.1.19"`), compared as `major.minor.patch`;
   - any method in `USES` is missing from `hello.methods`.
4. Requires `framed: true` in the reply, then starts the reader and writer tasks.

`USES` lists `hello`, `terminal.run|list|get|attach|kill|label`, `events.subscribe`, `doc.get`, `doc.set`. corgi
also calls `terminal.rm`, `terminal.put`, `templates.list` and `machine.list|exec|add|connect|rename|rm`; these
aren't gated, so a core without them fails at the call, and the app shows the error.

Features are kept in `HelloResult::features` (`has(name)`). `automatic_input` is the only one corgi acts on: an
attach uses AUTOMATIC_INPUT frames only when both the hello and the attach reply say so (`Client::attach`).

`corgi doctor` (`crates/corgi/src/doctor.rs`) connects the same way and prints the version, features, terminal
count and the layout document's state.

## Framing

After hello every byte is a frame (`wire.rs`):

```
[channel u32 LE][kind u8][len u32 LE][payload]
kinds: DATA 0  CREDIT 1  CLOSE 2  RESIZE 3  FOCUS 4  FACTS 5  AUTOMATIC_INPUT 6
```

Payloads over 16 MiB (`MAX_PAYLOAD`) and truncated frames are errors; a clean end of stream is allowed only between
frames (`read_frame`).

## Channels

Channel 0 carries JSON-RPC. `Client::call` takes an id, registers a oneshot in `State::pending`, and sends the
request; the reader task (`read_loop` → `deliver_reply`) completes it. Calls run concurrently. Errors become
`Error::Rpc { code, message, kind, hint }`; `Error::kind()` gives the stable name (`not_found`, `conflict`,
`unavailable`, …).

Other channels belong to an attach or the event stream. The reader routes their frames:

- to the owner's mpsc sender in `State::routes`;
- into `State::early` when the owner hasn't claimed the channel yet (the attach reply is still on its way to the
  caller); `Client::route` delivers those first;
- nowhere, when the channel is in `State::dropped` (its owner was dropped): frames are discarded until core's CLOSE.

The writer task (`write_loop`) batches whatever is queued before each flush.

When the line drops, `Inner::finish` fails every pending call with `Error::Closed`, ends every stream, and wakes
`Client::closed()`.

## Credit

Core sends at most the credit a stream has. Every stream starts with 256 KiB (`wire::INITIAL_CREDIT`).

- The event stream gets 1 MiB more at subscribe (`EVENTS_CREDIT`), and `Events::next` credits each DATA frame as it
  takes it.
- Attach output is credited only after the emulator has used it. The pane's task reads with
  `Attach::recv_uncredited` (`crates/corgi/src/pane.rs`). The app writes the bytes into the `Vt` (`Pane::output`)
  and counts them (`Pane::used`). After each loop pass, `Pane::credit` sends what is owed with `AttachTx::credit`:
  one CREDIT frame per pane per pass.

```
core ──DATA──► reader task ──mpsc──► pane task ──Msg::Stream──► app: vt.write, used(n)
  ▲                                                                     │ end of loop pass
  └──────────────────────────── CREDIT n ◄──── AttachTx::credit ◄──────┘
```

So output waiting anywhere between core and the emulator stays within one stream's credit, and a flooding program
waits in core instead of in corgi's memory. The ignored e2e test `a_flood_doesnt_drown_the_others` measures this.
`Attach::recv`, which credits on hand-over, remains for simpler users (the integration tests).

## Attach lifecycle

`Pane::attach` (`pane.rs`) detaches any old stream, bumps the pane's `epoch`, and spawns a task:

1. `Client::attach(addr, cols, rows, {history: 2000, display, resumed})`. `display` is the host's colours and cell
   size from `display::query`, so core can answer the program's colour and pixel queries as your terminal would.
   corgi sends these facts only in the attach call; `AttachTx::facts` (a FACTS frame) exists but isn't used yet.
2. `Msg::Attached` with the `AttachTx`, the record, and the size asked for.
3. Every frame as `Msg::Stream`, until CLOSE or the line ends.

Messages carry the epoch; `Pane::is_current` drops frames from an older attach.

On `Msg::Attached`, `Pane::attached` starts a fresh `Vt` at the size asked for, so core's first paint (which starts
with CAN) lands on a clean screen. If the pane was laid out again meanwhile, it resizes the `Vt` and sends RESIZE.
It sends FOCUS, then any keys typed while attaching (`pending`). An `unavailable` error means a kept terminal is
between programs: the pane shows "waiting for its new shell" (`State::Recovering`).

While live:

- Keys and pastes go as DATA (`AttachTx::input`).
- Replies the emulator generates (size reports and other answers from `Vt::take_replies`) go as AUTOMATIC_INPUT
  (`AttachTx::automatic_input`), falling back to DATA when either end lacks the feature. They don't take size
  ownership in core; typed input does.
- A layout change sends RESIZE (`Pane::resize`), a focus change FOCUS (`Pane::focus`).
- A reattach after a dropped line passes `resumed: true` (`App::sync` sets it when the pane already has a record),
  so it doesn't take the size before you act.

CLOSE ends the stream. `Pane::closed` maps the reason: `exited`, `killed` or `finished` (`CloseReason::is_final`)
→ `State::Exited`; another reason → `State::Recovering`; an empty CLOSE or the line ending → `State::Offline`.
Dropping an `Attach` sends CLOSE; `Attach::detach` waits up to 2 s for core's.

DEC 9001 (win32-input) isn't encoded; it comes with Windows support.

## Events

`App::connected` (`app/mod.rs`) subscribes with an empty filter (all terminals), then reads the layout document.
Subscribing first is core's rule: no change slips in between. The task sends `Msg::Snapshot` with the subscribe
result's terminals, then `Msg::DocLoaded`.

`Events::next` splits DATA into lines (`events::Lines`, safe across any frame split), parses each with
`Notification::parse`, and applies it to `Events::snapshot` with the pure `Snapshot::apply`:

| Notification | `Applied` | App |
|---|---|---|
| `event` with `seq` ≤ the snapshot's | `Stale` | ignored |
| `created`, `labels`, `status`, `title`, `size`, `foreground`, `turn`, `keep`, `lost`, `finished` | `Changed(addr)` | record updated; a finished terminal in the layout closes its pane (tmux behaviour); agent toasts |
| `restarted` | `Restarted(addr)` | the pane reattaches |
| `removed` | `Removed(addr)` | pane removed from the layout |
| `doc` | `Doc { name, version, machine }` | the local `corgi` doc changed: re-read it unless saving or already at that version |
| `missed` | `Resynced` | the snapshot is replaced; `Msg::Snapshot`, and the doc is re-read |
| `heartbeat` | `Heartbeat` | nothing, except that it arrived |

The task wraps `ev.next()` in a 45 s timeout (`SILENCE`: three missed 15 s heartbeats). On timeout it sends
`Msg::Silent`, and the app calls `Client::close()`, which leads into recovery below.

## Recovery

`App::connected` also spawns a task that awaits `Client::closed()` and sends `Msg::Disconnected`. Then:

1. `conn` becomes `Reconnecting`, `saving` is cleared, every pane detaches and goes `Offline` (its last screen
   stays, with "reconnecting…").
2. A task runs `Client::reconnect(transport, 1s)`: it retries `connect` every second and gives up only on
   `Error::Mismatch` or a missing `hc`.
3. `Msg::Connected(Ok(client))` replaces the client and calls `App::connected` again: a new event subscription,
   machine polling, and a fresh read of the document.
4. `Msg::Snapshot` and `Msg::DocLoaded` reconcile the layout with core's terminals, and `App::sync` reattaches the
   visible panes with `resumed: true`.

Terminals corgi starts carry `keep: true`, so a core restart or `hc update` keeps them. A kept terminal whose
program died reports `lost`, then `restarted` with a new shell, and the pane reattaches. The e2e tests
`a_core_restart_keeps_the_panes`, `a_killed_shell_comes_back` and `a_core_stop_and_start_keeps_the_layout` cover this.

## Documents and compare-and-swap

The layout is the document `model::DOC` (`"corgi"`) on the core corgi connects to (`on: None`).
`Client::doc_get` returns `{value, version}`; `Client::doc_set(name, value, version, on)` succeeds only if
`version` is current, and returns the new one; a stale version fails with kind `conflict`.

`App::save` runs after each loop pass. It writes only when the doc is loaded, changed (`dirty`), no write is in
flight (`saving`) and the line is up. Changes made during a write go in the next one.

- `Msg::Saved(Ok(v))`: `doc_version = v`; `persist::backup` keeps the last 20 versions locally.
- `Msg::Saved(Err(conflict))`: another client wrote first. corgi takes theirs (`fetch_doc`); its terminals come
  back through their labels when the new doc is reconciled. At worst a local tweak (a ratio, a rename) is lost.
- Any other error sets `dirty` again, to retry.

`App::doc_loaded` never saves over a document it can't parse: it says so and leaves it. See
[ADR 0003](adr/0003-layout-document.md) and [layout-engine.md](layout-engine.md).

## Machines are polled

Core sends no event when a machine's line drops: its terminals just vanish from lists, and come back in a `missed`
snapshot. So `App::poll_machines` (`app/machines.rs`) calls `machine.list` every 2.5 s while the client is open and
sends `Msg::Machines`. `App::machines_arrived`:

- for a machine whose line came up, resets its panes (addresses `name/…`) to `Offline` so `App::sync` reattaches
  them, and reruns the git tick;
- toasts when a machine with workspaces goes down or comes back;
- repoints usage segments that follow the workspace (`App::usage_target`).

While a machine is down, `App::down_machine` keeps its panes from attaching, the draw greys their last screen, and
`Doc::reconcile` keeps panes whose address contains `/`. Work on another machine goes through `machine.exec` with
`on` (`app/worktrees.rs::exec_for`), which decodes the base64 stdout and stderr.
