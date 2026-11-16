# ADR 0002: our own herdr-core client (`hc-proto`)

- Status: accepted (M1, 2026-11-09)
- Context: [PLAN.md](../../PLAN.md) §1 D2 and D3

## Decision

Write the client ourselves instead of depending on the `herdr-core` crate. herdr-core is `publish = false`, pulls in
clap, schemars and Ghostty, and most of its result types are Serialize-only. The protocol is small and documented
(`schema/README.md`), so a client takes about 600 lines. herdr-app made the same call.

The default transport is `hc connect`. It starts the core when needed, the core handles socket discovery, and
`ssh host hc connect` works the same way. A direct Unix socket is available through `--socket`.

## Shape

- `wire`: the frame codec.
- `client`: one connection.
  - A reader task routes frames: replies to their callers, stream frames to their owners. Frames for channels not
    claimed yet are buffered.
  - A writer task batches outgoing frames.
  - `Attach` and `Events` own their channels and send credit as frames are consumed. Dropping either sends CLOSE.
- `events`: the pure snapshot reducer, plus the line splitter.
- `types`: the records corgi reads. Each keeps unknown fields in `extra`, because the API only grows.
- `check`: generation, minimum version and required methods. A core that fails it is reported, not used.

The library uses tokio. The app adopts tokio in M2 (D8). `corgi doctor` already runs on it.

## Findings from the integration tests (hc 0.1.19, macOS)

1. **Socket paths.** macOS caps them at 104 bytes. Core puts its socket and its holder socket in the state dir, so
   test state dirs have to be short. The test setup checks the longest path up front and fails with a clear message
   if it's too long.
2. **Relative `HC_SOCKET` breaks restart adoption.** With `HC_SOCKET=s.sock`:
   - after `server.restart`, the new core starts a new holder instead of adopting the old one;
   - the old holder outlives `server stop`, and later starts a core of its own.

   With core's default absolute socket, the holder is adopted (same pid) and `stop` leaves nothing. Our tests
   therefore never set `HC_SOCKET`. This looks like an upstream bug. It's worth reporting to herdr-core with the
   repro steps above; we haven't filed it.
3. Everything else behaved as documented:
   - the first paint begins with CAN;
   - RESIZE from the active viewer changes the core's size;
   - detach leaves the program running;
   - exit sends CLOSE with `{reason:"exited", exit_code}`;
   - `doc.set` with a stale version fails with kind `conflict`;
   - doc and terminal events arrive in seq order;
   - a kept terminal survives `server.restart`.

## Consequences

- When core's protocol grows, corgi updates `hc-proto` by hand. Records tolerate new fields, and `check` catches
  breaking generations.
- Integration tests need `hc` on PATH, and skip with a note when it's missing. CI installs it on Linux and macOS.
