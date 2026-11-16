# Handoff

For whoever picks corgi up next (a new session included). Read this, then `docs/PLAN.md`
(milestones, what's done, what's open) and `docs/OUTLINE.md` (the docs to write). Updated
2026-11-11, at `m8-remote` = `1abc2fa` (`main` = `65cb887`).

## What corgi is

A Rust TUI for [herdr-core](https://github.com/herdrdev/herdr-core) (`hc`), replacing the old herdr
TUI (now a desktop app). tmux-like keys, keyboard first; parity with the herdr TUI; modal dialogs;
usage in the tab bar. Terminals live in core (they survive corgi); corgi is a viewer and layout
manager. The layout (workspaces › windows › panes) is a JSON document in core (`doc` named
`corgi`), so it survives restarts and is shared by every corgi on that core.

Crates:
- `crates/hc-proto`: the core client (JSON-RPC over `hc connect` stdio or a socket; framed streams
  for attach; events). ADR 0002.
- `crates/corgi-vt`: libghostty-vt (Zig) via bindgen: the terminal emulator for each pane.
  `frame()`, keys, paste, `screen_lines()` (every row's text, history first), `scroll_to(row)`,
  `scrollbar()`, notifications (OSC 9/777). ADR 0001.
- `crates/corgi`: the app. `src/app/` is the App (one tokio task, an mpsc `Msg` loop):
  `mod.rs` (state, main loop, events, spawning, layout geometry `tiling/frames/inside/titled`),
  `actions.rs` (`run(Action)`), `keyboard.rs` (keys by `Mode`), `draw.rs`, `dialogs.rs`,
  `menus.rs`, `copymode.rs`, `popup.rs`, `worktrees.rs`, `notify.rs`, `usage.rs`.
  Pure modules beside it: `commands.rs` (the tmux-style command language: every key binding,
  `[[keys.bind]]` and the `:` prompt run these), `keys.rs` (keymap, presets tmux/herdr/none),
  `config.rs` (+ `default-config.toml`), `form.rs` (dialogs' forms), `menu.rs`, `copy.rs`,
  `chooser.rs`, `tree.rs`, `sidebar.rs`, `pane_title.rs` (breadcrumbs), `usage.rs` +
  `usage_sources.rs`, `worktree.rs` + `worktrees.rs`, `toast.rs`, `host_notify.rs`,
  `claude_tap.rs` (`corgi claude-statusline`), `keyhints.rs`, `keylist.rs`, `layout.rs`, `model.rs`.

## Build, test, commit

- First time: `just bootstrap` (pinned Zig in `.tools/zig`, Ghostty in `vendor/ghostty`;
  `scripts/pins.env`). `hc` must be installed for the integration and e2e tests (they skip without).
- `just check` = fmt check + `cargo clippy --workspace --all-targets -- -D warnings` + tests.
- **Before every commit: run the whole suite (three times for anything touching the app), read the
  result, and only then commit, in a separate step.** Twice a commit went in red because the test
  run and the commit were one command (`8ab6f07`, `f74ac77`; fixed after). Don't do that.
- After e2e runs, check for leftovers: `hc hold`/`hc-launchd`/`target/debug/corgi` processes whose
  cwd is under `target/tmp/` (kill only those, never the user's own), and `git worktree list` must
  show only the main checkout.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Branch per
  piece of work (`m8-remote`, …); merge into `main` by fast-forward when the user says so.
- rustfmt `max_width = 120`. Code comments are plain prose: say what and why, no filler.
- Edits with exact-match replace break when rustfmt has rewrapped the text; read the current text
  first (grep/sed) when a match fails.

### The e2e harness (`crates/corgi/tests/e2e.rs`)

Drives the real binary in a PTY (`libc::openpty`), reads the screen back through corgi-vt.
`Core::start(id)` gives each test its own core session and folders under `target/tmp/h/<id>`;
`Corgi::spawn(core)`, `spawn_env(core, env)`, `spawn_at(core, dir, env)`; `h.typ`, `h.wait_for`,
`h.screen()`, `h.bg(x, y)`, `live(&core)` (live terminals).
- The test folders are inside corgi's own git checkout: `Core::env` sets
  `GIT_CEILING_DIRECTORIES` so git can't see it. Anything git-related must work in a repo the test
  makes itself (`spawn_at` into it). Never let a test run git on corgi's checkout (a worktree
  test once made a real worktree and branch there).
- `CLAUDE_CONFIG_DIR` / `CODEX_HOME` point at the test's folder (usage segments read those logs).
- Esc then more keys in one write reads as Alt+key: pause (~200 ms) after Esc.
- Layout-exact tests pin `[sidebar] mode="hidden"`, `[tab_bar] style="plain" position="top"`,
  `[panes] style="plain" title="off" separate=false inset=0`.
- Wait for something only the new state shows (not text that's already on screen).

## How things work (the parts the next work touches)

- Spawning: `App::spawn(purpose, cwd, machine, run)` → `terminal.run` with labels
  `corgi.workspace` / `corgi.window`; `Msg::Spawned` → `place()`. `machine` is core's `on`.
  Workspaces carry `machine: Option<String>`; a remote terminal's address is `machine/t7`.
- Reconcile (`model.rs`): drops panes whose terminals are gone, *except* remote ones (address
  contains `/`) whose machine may just be down; adopts terminals labelled for a workspace.
  Terminals without those labels (popups: `corgi.popup`) are never adopted.
- Running things on a workspace's machine: `machine.exec` with `on` (worktrees, run-shell). See
  `app/worktrees.rs::exec` (decodes base64 stdout/stderr, returns git's last error line).
- Background reads: `App::tick()` every 10 s (and on folder changes, new workspaces): git branch,
  repository (common dir), checkout top-level, for *local* workspaces only.
- Workspace names: `App::ws_label(s)` (your name, else the live folder's checkout name, else the
  folder) and `App::live_folder(s)` (the first window's first-made pane's cwd). Don't use
  `Workspace::label()` in the app.
- Dialogs: `Mode::Dialog(Dialog { does: Does, form: Form, busy })`; `form.rs` has text / path /
  select / toggle fields, hidden fields, message-only confirms. New dialogs: add a `Does` variant
  and handle it in `dialogs.rs::submit`. Lists from core arrive by `Msg::Lists`.
- Every new UI state is a `Mode` variant: route its keys in `keyboard.rs`, draw it in `draw.rs`,
  give it a badge in `mode_badge` (+ a label in `ModeLabels` and `default-config.toml`).
- New commands: `Action` + `parse` + `NAMES` in `commands.rs` (with a test), handled in
  `actions.rs::run`; bind in `keys.rs` presets with a hint (watch the key-hints test: the panel
  must fit a 100×30 screen; whole groups are packed into the shortest columns).

## The user's preferences (follow these)

- Keyboard first, like tmux/nvim: everything doable without the mouse; the mouse is a shortcut.
- Parity with the herdr TUI where it has the feature (read its source: `herdr` repo, e.g.
  `src/workspace.rs`, `docs/next/website/src/content/docs/*.mdx`; herdr-core's
  `schema/hc.openrpc.json` and `schema/README.md` for the API).
- Look they chose: sharp corners, tab bar at the bottom spanning the window (no box), sidebar
  boxed with agents in its bottom half, panes each in their own box (`separate`), breadcrumbs row
  with a divider and a status indicator (spinner while working, solid ● for states), inset 1,
  key hints docked at the bottom, toasts top-right, usage segments as coloured blocks.
- Plugins later (M10, process plugins in herdr's manifest format). No in-process scripting.
- Answers short and concrete; show the result (ASCII sketches help); offer the next step.
- Don't edit the user's dotfiles without asking (shell rc edits were refused once). Their
  `~/.claude/settings.json` status line ran `$HOME/.cargo/bin/herman claude-statusline -- …`
  (backup `settings.json.bak-herman`); since the rename (herman → corgi, 2026-11-13) it should
  say `corgi`: the user changes it themselves. Re-run `cargo install --path crates/corgi --locked` when
  they want the installed copy updated.

## Remote machines (done on `m8-remote`)

- `machines.rs` (pure: `Machine`, `Link` states, the Machines dialog) and `app/machines.rs`
  (polling `machine.list` every 2.5 s, reattach on reconnect, Add/Rename/Remove dialogs,
  `hc machine login|update|add` in popups). Core sends no event when a line drops: remote
  terminals just vanish from lists, and return in a `missed` snapshot.
- The tree's top level is machines once any exist (`Target::Machine("")` is this one).
- Git facts are keyed by `git_key(machine, folder)` (`app/mod.rs`); remote ones come from
  `worktree::facts_script` through `machine.exec`.
- Usage segments with `on = "workspace"` read through `usage::State.target`.
- e2e `a_machine_reached_by_a_command`: a second core session reached with `--via` a script
  that drops its line while a `down` file exists. `Forget` must drop first (else the test core
  redials and restarts the other session after it's stopped). `Core::hc` pins `SHELL=/bin/sh`.

## Next

M8 has: image paste (`terminal.put`), the narrow layout (≤ 64 columns), copy-mode extras
(`C-v` blocks, `W B E { }`, mouse selection). M6 still has the settings dialog and the
chooser's status filters and pane preview. M9: hardening and release.
