# Testing

What the tests cover, how the end-to-end harness drives a real corgi against a real, isolated core, and the rules that keep the suite reliable and away from your own core.

## Running

| Command | Runs |
|---|---|
| `just test` | `cargo test --workspace`: everything below except ignored tests |
| `just check` | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, then the tests |
| `cargo test -p corgi-tui --test e2e` | the end-to-end tests only |
| `cargo test -p corgi-tui --test e2e -- some_test_name` | one e2e test |
| `cargo test -p corgi-tui --test e2e -- --ignored a_flood` | the flood test, on its own |

The integration and e2e tests need `hc` on `PATH` (or `HC_BIN`); without it each one prints "skipping: hc not
installed" and passes. The e2e tests are Unix only (`#![cfg(unix)]`).

Before committing anything that touches the app, run the whole suite three times and read each result. Commit in a
separate step after that, never in the same command as the test run.

## Unit tests

Every pure module tests itself in a `#[cfg(test)] mod tests` in the same file. The main ones:

| Area | File |
|---|---|
| framing, hello gate, events reducer, line splitting, error text | `hc-proto/src/wire.rs`, `lib.rs`, `events.rs`, `client.rs` |
| key and mouse encoding, frames, scrollback, holds | `corgi-vt/src/keys.rs`, `mouse.rs`, `lib.rs` |
| split tree, geometry, neighbours, resize, presets, serialisation | `corgi/src/layout.rs` |
| model operations and reconciling | `corgi/src/model.rs` |
| command parsing and errors | `corgi/src/commands.rs` |
| keymap: notation, presets, bind and unbind | `corgi/src/keys.rs` |
| key-hints layout and rendering | `corgi/src/keyhints.rs` |
| forms, menus, copy-mode motions, chooser, tree, sidebar, toasts, status | the module of the same name |
| usage allocator and formatting; sources against scratch logs | `corgi/src/usage.rs`, `usage_sources.rs` |
| config defaults (`default-config.toml` matches `Config::default`), theme resolution | `config.rs`, `theme.rs` |
| bounded program runs | `corgi/src/process.rs` |

Rendering is tested by drawing into a ratatui `Buffer` and asserting on the rows as text. There are no snapshot
files: the repo doesn't use `insta`.

## Protocol integration tests

`crates/hc-proto/tests/core.rs` runs the client against a real `hc`: hello and errors; attach (first paint, input,
resize, detach); a program's exit closing the stream with its reason; events and document compare-and-swap; a
terminal surviving `server.restart` with the client reconnecting; a missing `hc` giving an install hint.

Its `Core::start(id)` asserts the longest socket path core will make stays under 104 bytes (macOS's limit for Unix
socket paths), so keep ids to one or two letters. Don't set a relative `HC_SOCKET` in tests: on hc 0.1.19 the
holder isn't adopted across `server.restart` with one and outlives `server stop` (see
[ADR 0002](adr/0002-hc-proto.md)).

## The end-to-end harness

`crates/corgi/tests/e2e.rs` runs the real `corgi` binary on a PTY and reads its screen back through
`corgi-vt`, the emulator the panes use.

### `Core`

- `Core::start(id)` makes `target/tmp/h/<id>` (`CARGO_TARGET_TMPDIR`), emptied first, or returns `None` when `hc`
  is missing. Use `let Some(core) = Core::start("xy") else { return };`.
- `Core::env()` is what every process in the test gets: `HC_SESSION=e<id>`, `XDG_STATE_HOME`, `XDG_CONFIG_HOME` and
  `XDG_RUNTIME_DIR` inside the folder, `GIT_CEILING_DIRECTORIES` set to the parent of the test folders, and
  `CLAUDE_CONFIG_DIR` / `CODEX_HOME` pointing at the test's own folders.
- `Core::config(toml)` writes corgi's config for this test.
- `Core::hc(args)` runs `hc` in this session and returns its JSON output.
- The panes' shell is `Core::shell()`: a wrapper that runs `/bin/sh` with a plain `$ ` prompt, not as a login shell
  (a login shell takes the system's prompt, with the host name in it: long on CI machines, where it wrapped in
  narrow panes and broke tests). Commands (`-c …`) pass through. Both corgi and `hc` get it as `SHELL`.
- `Drop` asks `hc server status`, runs `hc server stop`, and kills any holder process it listed.

### `Corgi`

- `Corgi::spawn(&core)`; `spawn_env(&core, &[(k, v)])` adds variables; `spawn_at(&core, dir, env)` starts in
  `dir` (its first workspace's folder); `spawn_sized(&core, dir, env, (cols, rows))` on another terminal size. The
  default is 100×30.
- The child gets its own session with the PTY as controlling terminal, `SHELL=/bin/sh` and `CORGI_LOG=debug`.
- A reader thread collects output and answers the host queries a real terminal would: OSC 10/11 colours and cell
  size, the kitty keyboard query, DA1.
- `h.typ(s)` writes bytes as if typed (`"\x02"` is `C-b`).
- `h.screen()` replays all output since the last resize into a fresh `Vt` and returns the rows as text;
  `h.bg(x, y)` gives a cell's background.
- `h.wait_for(text, secs)` polls the screen every 50 ms until a row contains `text`, else fails showing the screen.
- `h.resize(cols, rows)` sets the PTY size (corgi gets SIGWINCH); `h.wait_exit(secs)` returns the exit code.
- Helpers: `live(&core)` lists live terminal addresses, `pids(&core)` their program pids, `tab_row(&screen)` finds
  the tab bar row.

### Rules for e2e tests

- Never touch the real core or your own folders. Everything goes through `Core::env()`.
- The test folders sit inside corgi's own git checkout. `GIT_CEILING_DIRECTORIES` stops git from finding it.
  Anything git-related must use a repository the test makes itself (`git init` in the test's folder, then
  `spawn_at` into it). Never let a test run git on corgi's checkout: a worktree test once made a real worktree and
  branch there.
- Wait for something only the new state shows, not text that's already on screen.
- After Esc, pause about 200 ms before more keys: Esc followed by more bytes in one read is Alt+key.
- Tests that check exact layout pin the chrome: `[sidebar] mode = "hidden"`, `[tab_bar] style = "plain"
  position = "top"`, `[panes] style = "plain" title = "off" separate = false inset = 0`.
- Use a new, short id per test.

### Another machine: the loopback `--via`

`a_machine_reached_by_a_command` adds a second core session as a machine with
`hc machine add --via "sh <test>/via.sh" --name loop --session emcx`. The script runs core's line command and drops
the line while a `down` file exists, so the test can take the machine down and bring it back. A `Forget` guard
runs `hc machine rm loop` when the test ends, and must drop before the second `Core`, or the test's core redials
and restarts the other session after it was stopped. `Core::hc` pins `SHELL=/bin/sh` so the cores' terminals run
the shell the test expects.

### Performance checks

- `idle_doesnt_spin` measures corgi's CPU time with `ps` over 3 s, idle and in prefix mode with hints off: under
  0.15 s each.
- `a_flood_doesnt_drown_the_others` is `#[ignore]`d: three panes run `yes`, and a fourth must still echo within
  3 s while corgi stays under 300 MB. It takes whole cores and starves the tests beside it, so run it alone. The
  floods stop themselves after 12 s, and a `Floods` guard kills the core's terminals even if the test fails.

## After an e2e run

Check for leftovers, and remove only the test's:

- `hc hold`, `hc-launchd` or `target/debug/corgi` processes whose working directory is under `target/tmp/`. Kill
  only those, never your own sessions.
- `git worktree list` must show only the main checkout.

## Speed

Keep the suite in seconds. Each e2e test waits only for what it needs (`wait_for` with a deadline), not fixed
sleeps; short pauses are for Esc and for settling after a key the screen can't show. No test runs a real agent.

## The README's demos

`crates/corgi/tests/demos.rs` records the demos in the README: each test drives corgi in a PTY with a script
of keys and pauses, on a core session of its own with HOME a scratch folder (projects `web`, `api`, `docs`)
and a plain `$ ` prompt, and saves what corgi drew as an asciinema cast in `docs/assets/demos/`. They're
`#[ignore]`d. `just demos` (`scripts/demos.sh`) records them all and makes the GIFs with agg
(`brew install agg`); `just demos demo_keys` records one. Re-record them when the look changes. A recording
starts with a resize, so corgi draws the whole screen afresh (`Rec::start`).
