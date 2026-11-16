# corgi: implementation plan

corgi is a Rust terminal UI (ratatui + crossterm) for [herdr-core](https://github.com/herdrdev/herdr-core) (`hc`).
It replaces the old `herdr` TUI, which ran its own PTY server and is being superseded by the herdr desktop app.

What corgi owns and what core owns:

- **herdr-core** runs the terminals, keeps them alive, relays raw bytes, answers terminal queries, stores status
  (OSC 7501), connects machines, and stores opaque JSON documents. It draws nothing.
- **corgi** owns everything a person sees and arranges: workspaces, windows, the pane layout, keybinds, modal
  dialogs, the tab bar, the sidebar, usage status, and themes.

Closing corgi, or detaching, leaves every terminal running in core.

---

## 1. Decisions to make first

Each decision gets an ADR in `docs/dev/adr/`. The "Recommendation" column is the proposed default.

| # | Decision | Options | Recommendation |
|---|---|---|---|
| D1 | VT emulator inside each pane | `ghostty-vt` (what core and app use; needs a Zig build), `alacritty_terminal`, `vt100`, `wezterm-term` | `ghostty-vt`: same parser as core, and it comes with key/mouse encoders. Spike it in M0. Fall back to `alacritty_terminal` if the Zig toolchain is a blocker for contributors. |
| D2 | Protocol client | Depend on the `herdr-core` crate, or write our own | Write our own `hc-proto` crate, about 500 lines. The core crate is `publish = false`, pulls in clap, schemars and ghostty, and most result types are Serialize-only. herdr-app does the same (`herdr-app/src/hc.rs`). |
| D3 | Transport | Unix socket directly, or spawn `hc connect` | Spawn `hc connect --session S` by default: it autostarts core, and `ssh host hc connect` works the same way. Allow a direct socket (`HC_SOCKET`) as an option. |
| D4 | Where layout lives | Core document (`doc.get`/`doc.set`), or a local file | One core document named `corgi` per machine, written with compare-and-swap. Private view state (active workspace, focus) goes in `~/.config/corgi/view.json`. |
| D5 | Interop with the herdr desktop app | Share the `herdr-app` doc and `herdr.space`/`herdr.tab` labels, or stay separate | Separate at first: `corgi.*` labels and our own doc. Share `herdr.seen=<turn>`, so "done vs seen" agrees across clients. Revisit two-way sync after v1. |
| D6 | Names | tmux (session/window/pane), or herdr (space/tab/pane) | **Workspace → Window → Pane.** The UI and keys follow tmux; "workspace" stands in for a tmux session, because a core session is a different thing. |
| D7 | Agent status source | OSC 7501 only, or re-add screen scraping | OSC 7501 only. This is core's hard rule. The agent's identity comes from the `foreground` event and the status `app` field. Panes that never report show as `unknown`. |
| D8 | Async model | tokio + one app loop, or threads | tokio. The transport task routes frames to channels, and one app task owns all state. Render on dirty, capped at about 60 fps. |
| D9 | Usage data sources | See §5 | Use a pluggable provider trait. Ship the agents, system, command, clock and Claude/Codex local-log providers. Leave the plan-limit percentage out until a supported source exists. |

---

## 2. Architecture

```
┌────────────────────────── corgi (one process) ─────────────────────────┐
│ crossterm input ─► Keymap/state machine ─► Action ─► App state (owner)   │
│                                                       │   ▲              │
│  Modal/forms ◄──────────── Action ◄───────────────────┘   │ AppEvent     │
│                                                           │              │
│ Renderer (ratatui): sidebar │ tab bar + usage │ panes │ overlays         │
│        ▲ reads              ▲ UsageHub (providers, timers)               │
│        │                                                                 │
│  PaneView[n] = VT emulator + attach channel + scrollback                 │
│        ▲ bytes                       │ keys/resize/focus/facts           │
│ hc-proto: framed codec · channel router · RPC correlator · credit ·      │
│           events reducer (seq/missed) · reconnect loop                   │
└───────────────▲──────────────────────┬──────────────────────────────────┘
                │ `hc connect` stdio / unix socket / named pipe
         herdr-core (local) ── machines ──► herdr-core (remote, vps/t7)
```

Crates:

```
crates/
  hc-proto/   wire framing, JSON-RPC types (Deserialize), client, events reducer. No UI deps.
  corgi-vt/  emulator wrapper: feed bytes, read cells/cursor/title, encode keys+mouse, search, replies.
  corgi/     binary: config, model, layout, input, ui, modals, usage, persistence.
```

Code rules, adopted from herdr-core:

- Keep deciding code (layout, keymap, model reducers, usage allocator) separate from IO, so it can be tested
  without processes.
- Keep files under about 32 KB.
- Tests run against a real core in an isolated `HC_SESSION` with scratch XDG directories. They never touch the
  user's core.

---

## 3. Milestones and tasks

Every milestone ends with a demo and passing CI.

### M0: Bootstrap ✅ (2026-11-09, see [ADR 0001](dev/adr/0001-vt-emulator.md))

- [x] Cargo workspace (`hc-proto`, `corgi-vt`, `corgi`), rustfmt (width 120), clippy with warnings denied, `deny.toml`, `justfile` (`bootstrap`, `fmt`, `lint`, `test`, `check`, `run`, `bench`).
- [x] Pinned toolchain in `scripts/pins.env`. `scripts/bootstrap.sh` fetches Zig 0.16.0 into `.tools/` and Ghostty `1e5cdb8` into `vendor/`; nothing is installed system-wide.
- [~] CI workflow for Linux, macOS and Windows (`.github/workflows/ci.yml`). Written but not yet run: the repo has no remote. Installing `hc` in CI moves to M1.
- [x] Logging with `tracing` to `$XDG_STATE_HOME/corgi/corgi.log`. Level from `CORGI_LOG`.
- [x] Terminal guard: raw mode, alternate screen, mouse, bracketed paste, focus events, kitty keyboard flags. Restored on quit, panic and SIGTERM/SIGHUP. Verified in a PTY.
- [x] D1 spike: `ghostty-vt` builds, renders correctly through ratatui, parses at about 360 MB/s, and captures a 200×50 frame in about 0.3 ms. Decision: keep it.
- [x] Skeleton CLI: `--session`, `--socket`, `--config`, `--default-config`, `--version`. `corgi` with no arguments opens the spike screen (`C-b o` switches pane, `C-b d` or `C-q` quits).
- [x] Added: the key-hints dialog widget (`keyhints.rs`) with layout tests. The spike shows it on `C-b`; M5 makes it config-driven.

### M1: `hc-proto`, the core client ✅ (2026-11-09, see [ADR 0002](dev/adr/0002-hc-proto.md))

- [x] Transport: spawn `hc connect` (`Transport::hc`), or a Unix socket (`--socket`). A missing `hc` gives an install hint. If `hc connect` exits, the error is core's own message and hint.
- [x] `hello {framed:true, generation:1}`. Gated on the generation, the minimum core version (`MIN_CORE = 0.1.19`), and the required `methods` (`USES`). Features are recorded; `automatic_input` is used only when both the hello and the attach reply confirm it.
- [x] Framed codec with a 16 MiB cap. Round-trip tests across arbitrary splits, plus oversized and truncated frames.
- [x] Channel router, with early frames buffered until the owner claims the channel. Channels we dropped have their frames discarded until CLOSE.
- [x] JSON-RPC correlator: concurrent calls, typed errors (`kind`, `hint`). Every pending call fails with `Closed` when the line drops. `$/cancelRequest` is not needed yet.
- [x] Credit: attach output is credited as each frame is handed over; the events channel gets an extra 1 MiB up front.
- [x] Typed API: `run`, `get`, `attach` (`input`, `automatic_input`, `resize`, `focus`, `facts`, `detach`), `subscribe`, `doc_get`, `doc_set`, plus a generic `call`/`call_as`. Records keep unknown fields in `extra`.
- [x] Events reducer (`Snapshot::apply`, pure): handles every documented event type, plus `missed` resync, stale-seq dropping, doc events (local and remote), and lines split across frames.
- [x] Reconnect: `Client::closed()` and `Client::reconnect(transport, every)`. Retries while the core is down; gives up on a mismatch or a missing `hc`. M2's supervisor uses it to resubscribe, re-read docs and reattach.
- [x] Integration tests against a real `hc` (`crates/hc-proto/tests/core.rs`), about 0.8 s in total:
  - hello, run, list, `not_found`
  - attach: first paint starts with CAN; input echoed by `cat`; resize changes the core's size; detach leaves the terminal running
  - exit closes the stream with `exited`/3
  - events follow a terminal to finished; doc compare-and-swap with `conflict` and a doc event
  - a kept terminal survives `server.restart`, and the client reconnects
  - a missing `hc` gives an install hint
  - Each test gets its own core session and state under `target/tmp/h/<id>`. Teardown stops the core and any holder left behind. The user's core is never touched.
- [x] `corgi doctor`: connects and prints the core version, features, terminal count, and the state of the layout doc.
- [~] CI installs `hc` on Linux and macOS. Windows runs skip the integration tests, because the install path there isn't confirmed. Nothing has run in CI yet: the repo still has no remote.
- [ ] Moved to M2:
  - heartbeat watchdog (treat 45 s of silence as a dead line)
  - the supervisor that resubscribes, re-reads docs and reattaches panes after `reconnect`

### M2: Single-pane viewer ✅ (2026-11-09)

`corgi` now opens a real core terminal. If its own shell (label `corgi.pane=main`) is still alive it views that;
otherwise it starts one with `keep:true`. `corgi attach <addr>` views any terminal. M3 replaces the label with
layouts.

- [x] `corgi-vt`:
  - a fresh emulator per attach; the CAN at the start of the first paint is handled by the parser
  - callbacks for replies (`take_replies`), bell and title
  - `mode()` queries, scrollback (`scroll`, `scroll_bottom`)
  - `set_cell_pixels`, and mode-2048 size reports queued on resize
- [x] Attach with `{cols, rows, history: 2000, display, resumed}`. If the pane was resized while the attach was in flight, corgi resizes on arrival rather than re-attaching, and the program redraws. Keys typed while attaching are queued, not lost.
- [x] Emulator cells drawn into the pane area; the cursor is shown only while the pane is live and no dialog is open.
- [x] Input: crossterm key → `corgi_vt::Key` → ghostty's key encoder for the program's current modes.
  - Covered: DECCKM, kitty keyboard flags, Alt as ESC prefix, F-keys, Ctrl combos (tests for each).
  - Paste goes through `encode_paste`: bracketed when the program set mode 2004, unsafe bytes stripped.
  - Limitation: physical keys are inferred from US layout, which affects only kitty-protocol programs on other layouts.
- [x] RESIZE on host resize, FOCUS on host focus change.
- [x] Display FACTS: the host's fg, bg and cursor colours plus cell size are asked once in raw mode, before crossterm reads, and sent with attach. Resending on theme change waits for themes (M4).
- [x] AUTOMATIC_INPUT for emulator replies, used only when both ends support it (`AttachTx::automatic_input`).
- [x] Stream CLOSE: a final close shows "exited (code) · Enter: new shell". A kept terminal that lost its program shows "waiting for its new shell" and re-attaches on `restarted`.
- [x] Supervisor:
  - `closed()` → banner → `Client::reconnect` every 1 s → resubscribe → re-attach with `resumed:true`
  - heartbeat watchdog: 45 s of silence → `Client::close()` → the same path
- [x] Bell forwarded to the host. Mouse wheel scrolls history; any key jumps back to the bottom.
- [x] `C-b` shows the key-hints dialog; `C-b d` detaches (exit 0, terminal keeps running); `C-b C-b` sends C-b.
- [x] End-to-end test (`crates/corgi/tests/e2e.rs`): corgi on a real PTY against an isolated core. It covers spawn and typing, key hints, detach, reattach with the old screen restored, pane size seen by the shell, survival across a core restart, and exit then respawn. About 2.3 s; passed 3 times in a row.
- [ ] Not yet:
  - win32-input (DEC 9001) key encoding, with Windows (M9)
  - forwarding mouse events to programs that track the mouse (M8)
  - host window title (M4)

### M3: Model, layout and persistence ✅ (2026-11-09, see [ADR 0003](dev/adr/0003-layout-document.md))

- [x] Model (`model.rs`):
  - `Doc { schema, next, workspaces }` › `Workspace { id, name?, folder, machine?, windows, active, last }` › `Window { id, name?, root, focus, last, zoomed, preset }`.
  - A workspace is named after its folder; a window after the focused pane's title, then its foreground program, then argv[0].
- [x] Layout tree (`layout.rs`, pure):
  - split by percentage (the new pane gets it)
  - remove, with the sibling taking the space
  - neighbour by direction (most overlap, then top-left), swap
  - resize, moving the deepest matching divider and clamped to at least one cell
  - tmux presets: even-horizontal, even-vertical, main-horizontal, main-vertical, tiled
- [x] Geometry: one-cell separators. Tiny areas give everything to one side and never panic. Zoom shows the focused pane full size.
- [x] Persistence:
  - one `corgi` document on the local core, compare-and-swap, one write in flight
  - on a conflict, take theirs; labels put terminals back
  - the last 20 versions backed up locally
  - an unreadable document is never overwritten
  - the last write is flushed on detach
- [x] Labels on every terminal corgi starts: `corgi.workspace`, `corgi.window`, plus `keep:true` and the TERM/COLORTERM/TERM_PROGRAM env. Panes moved by break-pane are relabelled.
- [x] Reconcile with core's terminals:
  - dead or finished panes are dropped (remote ones kept while their machine is down)
  - labelled terminals are put back
  - M2's terminal is adopted
  - unlabelled terminals are left alone (they'll be "Unplaced" in M6)
- [x] tmux exit semantics: a pane closes when its program exits; after the last one, `[exited]`.
- [x] View state: the active workspace per client in `view-<session>.json`.
- [x] Prefix table (`keys.rs`, the tmux preset):
  - windows: `c n p l 0-9 , &`
  - panes: `% " arrows o ; z x { } ! Space C-arrows M-arrows`
  - workspaces: `N $ ( )`
  - other: `d C-b`
  - Arrow and resize keys repeat (tmux `-r`, 500 ms). The key-hints dialog is generated from the same table.
- [x] Status-line prompts (tmux style) for rename-window and rename-workspace, and y/n confirmation for kill-pane and kill-window.
- [x] Drawing: separators with junctions, the active pane's edges green. The status line shows the workspace badge, the window list (`*` active, `-` last, `Z` zoomed, a status dot per window), and messages. Click focuses a pane; the wheel scrolls the pane under the pointer.
- [x] Startup keys (typed before any window exists) are queued and replayed.
- [x] Nesting guard (found in real use: corgi was started inside its own pane and drew itself forever):
  - corgi detects the core terminal it runs in: `HC_TERMINAL_ID`, confirmed by the session leader's pid.
  - It refuses to start inside a corgi pane; `CORGI_NESTED=1` forces it.
  - It never attaches its own terminal; that pane shows a placeholder.
  - Panes whose foreground program is corgi get a warning bar.
  - `corgi doctor` lists corgi processes in core terminals and exits 1 for a nested one.
  - Tests cover a refused launch, a forced launch, the doctor report, corgi run inside a core terminal, and a kitty-keyboard terminal.
- [x] End-to-end tests (2, about 2.2 s, stable over 4 runs):
  - shell, key hints, detach, reattach with the screen restored, size, core restart, `[exited]`
  - split right, focus left, new window, select window, zoom on and off, detach and relaunch with the layout restored, kill-pane with confirmation
- [ ] CI still has no remote to run on (local only, by choice for now).
- [ ] Moved later:
  - join-pane and moving a pane to another window (M6: needs the chooser)
  - pane-number overlay (`q`, M5)
  - per-client focus (see ADR 0003)

### M4: Chrome and theming ✅ (2026-11-09)

- [x] Screen: the sidebar on the left at full height, the tab bar at top or bottom, panes in the rest. The sidebar hides below 60 columns.
- [x] `ui.corners`: `sharp` (default: ┌ ┐ └ ┘, so edges meet cleanly where boxes sit side by side and above the tab bar) or `rounded`. Applies to the sidebar, panes, tab bar and dialogs (through the theme).
- [x] Pane box: `[panes] style = "box"` (default) puts a rounded border around the pane area. Split separators join it (┬ ┴ ├ ┤), and the edges beside the focused pane light up, unless the keyboard is in the sidebar or a dialog. `plain` turns it off.
- [x] Mode badge at the start of the tab bar (tmux/vim style):
  - NORMAL, PREFIX (also during the repeat window), SIDEBAR, CHOOSE, RENAME, CONFIRM; NEW is kept for the M6 dialogs
  - every label is set in `[mode]`, and `""` hides the badge in that state
  - colours `mode_normal`, `mode_prefix`, `mode_sidebar`, `mode_dialog` in `[theme.custom]`
  - it replaces the right-side PREFIX badge
  - the workspace badge is gone from the tab bar (the sidebar already shows the current workspace)
  - found by its e2e check: a lapsed repeat window kept showing PREFIX until the next key; corgi now wakes when the window ends
- [x] `[tab_bar]` section:
  - `position`: top or bottom (default bottom)
  - `style`: `plain` (default, one row) or `box` (a rounded border like the sidebar's, three rows); the box falls back to plain on short or narrow terminals
  - `span`: `window` (default: the full width, under the sidebar too) or `panes` (beside the sidebar)
  - `padding`: 0–5 rows between the bar and the panes
  - (moved from `ui.tab_bar`)
- [x] Status line:
  - workspace badge
  - window tabs (`*` active, `-` last, `Z` zoomed, a status dot), scrolled with `‹ ›` to keep the active tab in view
  - a `+` button (new window)
  - PREFIX badge, messages, core version
  - click a tab to select it
  - The free space on the right is ready for usage segments (M7).
- [x] Sidebar (`sidebar.rs`, pure render plus click targets):
  - Workspaces: rolled-up status, name, git branch (local workspaces, refreshed every 10 s).
  - Agents across all workspaces: grouped or by priority.
  - Rows come from templates (`{state} {workspace} {branch} {machine} {window} {pane} {agent} {title} {label:KEY}`).
  - Modes: `show`, `collapsed` (icons only) or `hidden`. `C-b b` toggles it.
  - Click a workspace or an agent to jump there.
- [x] Status (`status.rs`):
  - From OSC 7501 only: blocked or error, working, done, idle, unknown.
  - Done means a turn ended that you haven't seen, tracked with the shared `herdr.seen` label (set while you look at the pane).
  - Roll up from pane to window to workspace.
  - `ui.indicators`: dots or symbols.
- [x] Themes (`theme.rs`):
  - 7 built in: catppuccin-mocha and -latte, tokyo-night, gruvbox-dark, nord, dracula, terminal.
  - `auto` picks light or dark from the host's background (from the display query).
  - `[theme.custom]` per-colour overrides.
  - Chrome only: panes keep the program's and the terminal's own colours.
- [x] Config (`config.rs`):
  - TOML at `$XDG_CONFIG_HOME/corgi/config.toml` or `--config`.
  - Every key has a default; `--default-config` prints them, and a test checks they match.
  - Unknown keys are warnings; bad values are errors with the key and the reason.
  - At startup an error stops corgi before it takes the screen. `C-b R` reloads; a broken file keeps the current config.
  - Covers: `keys.prefix` (any tmux-notation key; pressing it twice sends it), `keys.hints` always/delay/off plus `hint_delay`, `ui.mouse`, `ui.window_title`.
- [x] Host window title: `corgi · workspace:window`.
- [x] Workspace tree (`tree.rs`, pure), shared by the sidebar and the tree chooser:
  - workspaces › windows › panes, with status dots rolled up at each level
  - workspaces start open, windows start closed; only windows with more than one pane fold
  - row templates `window_row` and `pane_row`
  - fold state saved per client in the view file
  - in the sidebar: click ▾/▸ to fold, click a row to go there
- [x] Tree chooser dialog (`chooser.rs`, pulled forward from M6):
  - `C-b w` starts at the current window, `C-b s` at the current workspace
  - ↑↓ (or C-n/C-p) move; → ← fold, and ← on a leaf goes to its parent; Tab toggles
  - typing filters (matches shown with their parents, unfolded); Enter goes there; Esc clears the filter, then closes
  - folding in the dialog folds the sidebar too
- [x] Sidebar focus and resizing:
  - `C-b ←` from the left-most pane gives the sidebar the keyboard: highlighted border, cursor row.
  - `↑↓`/`j k` move, `←→`/`h l` fold, Enter goes there, Esc or `C-b →` goes back to the panes.
  - While it's focused, the resize keys resize it: `C-b C-←/→` by 1, `M-←/→` by 5, with repeat.
  - Dragging its right edge with the mouse also resizes it.
  - The width is saved per client in the view file and overrides `sidebar.width`; it's kept between 12 columns and the screen width minus 20.
  - Covered by an e2e test (keys don't leak into the pane, key and drag resizing, kept across a relaunch).
- [x] Every dialog (key hints, tree chooser) is centred on the whole window on both axes, not on the pane area beside the sidebar. Placement goes through `ui::centered`.
- [x] `sidebar.padding` (default 1, 0–10): empty columns between the sidebar and the panes. Reloads live; the status line moves with the panes.
- [x] Sidebar style: `box` (the default: rounded border, "Workspaces" in the title, `├─ Agents ─┤` divider) or `line`.
- [x] `sidebar.header`: with the box, `row` (the default: each part's name on a row of its own at the top, a line under it joined to the box, like the panes' breadcrumbs) or `border` (the name in the top border, `├─ Agents ─┤`).
- [x] Background-colour-erase fix (reported from real use with nvim). Cells cleared while a background colour was set carry only a colour. corgi-vt read colours only for styled cells, so nvim's background stopped at its text. Fixed by always reading the resolved background, with a unit test and an e2e check (corners of the pane). `CORGI_RECORD=<dir>` saves each pane's received bytes for replaying with `dump`.
- [x] The status line gives window tabs priority: messages get the room that's left and are cut with `…`. Found by the e2e test, where a long "config reloaded" message pushed window 0 out of view.
- [x] Split `app.rs` into `app/{mod,actions,draw}.rs` to stay under the file-size cap.
- [x] Tests: config, theme, status, sidebar render, tab scrolling. An e2e test checks the default sidebar, the `C-b b` toggle, reloading to a bottom tab bar, a broken config on reload and at startup.
- [ ] Not yet:
  - dragging the agents/workspaces split, and reordering tabs by drag (M8, mouse)
  - the other old-herdr themes (11 more)
  - pane title borders

### M5: Input and tmux-style keybinds ✅ (2026-11-10)

- [x] Commands (`commands.rs`, pure): one tmux-style language for key bindings, `[[keys.bind]]` and the `:` prompt.
  - Parsing: shell-style words (quotes, escapes), tmux options (`-h`, `-p 30`, `-c dir`, `--`), aliases (`splitw`, `neww`, …).
  - Errors name the problem and suggest the nearest command (`splt-window` → "did you mean split-window?").
  - Commands:
    - windows and panes: `new-window [-n name] [-c dir] [cmd]`, `split-window [-h|-v] [-p|-l pct] [-c dir] [cmd]`, `select-window -t N`, `next-/previous-/last-window`, `rename-window [name]`, `kill-window`, `select-pane -L/-R/-U/-D/-l`, `next-/previous-/last-pane`, `resize-pane -L/-R/-U/-D [n]` and `-Z`, `swap-pane -U/-D` or a direction, `break-pane`, `kill-pane`, `next-layout`, `select-layout <name>`
    - workspaces and agents: `new-workspace [-c dir]`, `rename-workspace [name]`, `kill-workspace`, `next-/previous-workspace`, `switch-client -n/-p/-T table`, `choose-tree [-s]`, `next-agent -b/-d`
    - other: `send-keys …`, `send-prefix`, `detach-client`, `toggle-sidebar`, `source-file`, `command-prompt [-p label] [-I text] [template with %%]`, `confirm-before [-p prompt] cmd` (`#P #I #W #S` placeholders), `list-keys`, `display-panes`
- [x] Keymap (`keys.rs`): named tables `prefix`, `root` (no prefix: corgi sees the key before the pane), `resize`, and any custom one (entered with `switch-client -T name`; repeatable bindings keep it in use until Esc).
- [x] Presets:
  - `tmux` (default)
  - `herdr`: the old herdr TUI's keys (`v`/`-` splits, `hjkl`, `HJKL` swap, Tab, `r` resize mode, `1-9`, `T`, `X`, `N`, `W`, `D`, `w`, `g`, `q`)
  - `none`
- [x] Config: `keys.preset`, `keys.repeat_time`, `[[keys.bind]] key/command/table/repeat/note/group`, `keys.unbind` (`"%"` or `"root M-h"`). The keymap is built while the config is parsed, so a bad binding is an error that names it. Rebuilt on `C-b R`.
- [x] `:` command prompt: history (↑↓), Tab completion of command names, mistakes shown in the status line.
- [x] `C-b ?` (`list-keys`): every binding by table, with its note and command. Typing filters by key, command, description or table; Enter runs the selected command. A dialog like the others (`keylist.rs`).
- [x] `C-b q` (`display-panes`): each pane's number for 1 s; a digit focuses that pane.
- [x] `C-b a` / `C-b A`: next blocked agent / next agent that finished unseen, across workspaces.
- [x] tmux extras in the default preset: `'` window by index, `M-1..5` select layout. Key-hints dialog for any table (resize mode shows its keys).
- [x] Mode badge labels for the new states: COMMAND, KEYS, PANES, RESIZE (other tables show their name).
- [x] A split's new terminal starts at its final size, so the shell doesn't get a resize straight away.
- [x] Key hints at the bottom (`keys.hints_position = "bottom"`, the default; `"center"` for the dialog): the window's full width, just above a bottom tab bar, its columns together in the middle. Each column is as wide as its own entries. The centred dialog may be up to 160 columns wide. Columns are packed so one tall group (Panes) no longer forces a single column.
- [x] Sidebar in two (`sidebar.agents = "split"`, the default): the workspace tree in the top box, every agent in the bottom one (`sidebar.agents_height`, 50 %). The tree scrolls to keep the cursor or the current workspace in sight; agents that don't fit become "+N more"; with none, "no agents running". `"below"` keeps the agents straight after the tree. Sidebars under 10 rows don't split.
- [x] Split panes apart (`panes.separate`, default on): each pane in its own box, like the herdr TUI's `pane_gaps`, the focused one lit; `panes.padding` puts blank cells between them (0: the boxes touch). `separate = false` gives tmux-style shared one-line dividers. Without the box style, separate panes keep at least one blank cell between them.
- [x] Inset (`panes.inset`, 1 column each side by default; `panes.inset_rows`, 0): blank cells between each pane's border and its terminal, taking the background of the terminal's nearest cell (like Ghostty's `window-padding-color = extend`), so a program that paints its background fills the pane to its border. Titles stay by the border; breadcrumbs line up with the text.
- [x] Breadcrumbs (`panes.title_shape = "breadcrumbs"`, the default): a row of its own at the top of each pane, under its border: the folder's path (`~ › workspace › corgi`), or folder › agent › what it's doing, or the pane's name; the first crumbs give way to `…` when narrow; the last lit for the focused pane; a line under them joined to the pane's border (`├──┤`), so they're a box of their own. The indicator at the row's end: a spinner (⠋⠙⠹…) while something works (an agent in the working colour, a command muted), a solid ● for a state (waiting: red, finished unseen: green, idle), nothing at a prompt.
- [x] Pane title lines (`panes.title_shape = "line"`): the title as text on the line above each pane (lit for the focused pane), and a circle at the line's end for what the pane is doing: ● waiting for you (red), a spinning ◐◓◑◒ while working, ● finished unseen (green), ○ idle, ◌ a command running, nothing at a shell prompt. The pills remain as `rounded`/`angled`/`slant`/`plain`.
- [x] Pane tabs (`pane_title.rs`): a tab on the border line above each pane.
  - What it says (`panes.title`): `auto` (an agent's summary while one runs, else the directory's name), `cwd`, `agent`, `off`.
  - The summary is the agent's OSC 7501 `title`, else its `msg`, else the terminal title it set (spinner stripped), else its name.
  - Shape (`panes.title_shape`): powerline caps `rounded`, `angled` or `slant` (Nerd Font), or `plain`.
  - The focused pane's tab is lit. Clicking a tab focuses its pane. Without the panes' box, the top row of panes gives up a line for the tabs.
- [x] Tests:
  - unit: parser, keymap (presets, bind, unbind, errors), key list
  - e2e: unbind and bind from the config, a root binding, the command prompt (run, mistake, completion, history), list-keys filter, pane numbers; herdr preset with resize mode
- [ ] Not yet:
  - importing `~/.tmux.conf`
  - `C` (New Window dialog) and `S` (settings): M6
  - `[` copy mode, `]` paste buffer, `e` scrollback in `$EDITOR`: M8

### M6: Modal dialogs

- [x] Form framework (`form.rs`; every dialog draws over the whole window, placed with `ui::centered`):
  - Overlay with focus trap and dimmed background. Field types: text (emacs editing), path (completion), select, toggle, read-only hint.
  - Validation messages. `Tab`/`S-Tab` move between fields, `Enter` submits, `Esc` cancels.
  - Its own key table. Mouse support. Centered layout that adapts to the terminal size.
- [~] Path completion:
  - [x] Local: std::fs. Folders only; hidden ones when you type the dot; Tab completes to the one match or the common start and lists the rest under the field.
  - Remote: `machine.exec ls`, or `file.read` against the target machine (`on`).
- [x] **New Workspace** dialog (`C-b N`, `dialog new-workspace`). The folder is checked when it's on this machine. Fields:
  - Name: optional; auto-named from the folder.
  - Folder: path completion, defaulting to the focused pane's cwd or home.
  - Machine: select from `machine.list`, with state shown.
  - First program: shell, a template from `templates.list`, or a known agent.
  - [x] Worktree: a switch (last, off by default) that reveals Branch (default a new `worktree/<adjective>-<noun>-<hex>`; an existing branch is checked out) and Path (default `<worktrees.directory>/<repository>/<branch slug>`, as herdr lays them out). `git worktree add` runs through core's `machine.exec`, so on the workspace's machine; the dialog waits, then the workspace opens in the worktree; git's error shows in the dialog if it can't.
- [x] **New Window** dialog (`C-b C`, `dialog new-window`; `C-b c` still opens one at once). The extra panes of a layout are shells in the same folder; the program keeps the first. Fields:
  - Workspace: select, defaulting to the current one.
  - Name.
  - Cwd: defaults to the focused pane's cwd.
  - Program or template.
  - Initial layout: single, or a preset with N panes.
- [x] Rename dialog (workspace `C-b $`, window `C-b ,`, pane `rename-pane`; a pane's name is its terminal's `corgi.name` label, so every viewer sees it, and shows in its tab). Confirm-close dialog (workspace, window, pane) listing agents that would end, controlled by `ui.confirm_close`: `auto` (workspaces always, panes and windows when an agent runs there; the default), `always`, `never`.
- [~] Chooser/tree (`prefix w`). Done in M4: tree, folding, filter, go to. M6: about three quarters of the screen; a "+ New workspace" row at the bottom (also chosen when the filter matches nothing, which then names it) opens the New Workspace dialog. Once there are other machines, they're the top level (machine › workspace › window › pane), in the sidebar too: this machine first (⌂ and its hostname), then each one with its line's state; a down machine's rows are dimmed. Still to do: status filters `b/w/i/d/a`, an "Unplaced" section, and a preview of the selected pane via `terminal.read`.
- [x] Add Machine dialog (`dialog new-machine`, `machine add`, `n` in `C-b M`): reach it over ssh (a target) or with a command (`via`, needs a name); name, session, temporary. It runs `machine.add` and waits ("reaching …"), then shows the Machines dialog; core's error shows in the dialog. C-t runs `hc machine add …` in a popup instead, where ssh can ask for passwords, host keys, or to install hc there.
- [ ] Settings overlay: theme, indicators, toasts, sound, usage segments.
- [x] Action menus, keyboard first (`menu.rs`): `C-b m` / `display-menu` for the focused pane (Pane, Window and Workspace sections), `m` on a sidebar row for that row (a window's has no Pane section), right-click on a pane, tab or sidebar row as a shortcut. Each entry runs on its own key, the same as its prefix key (`%`, `"`, `z`, `x`, `,`, `&`, `$`, …), or by `j`/`k` and Enter; entries are commands. New `dialog new-worktree`: New Workspace with the worktree switch on, from the workspace's folder. MENU badge.

### M7: Usage status in the tab bar (see §5) ✅ first cut (2026-11-10)

- [x] Sources (`usage_sources.rs`) and a hub (`app/usage.rs`): one loop per segment reads off the UI task every `interval`; the latest reading is kept with its time; older than 3× the interval draws dimmed. A config reload starts them over.
- [x] Free-space allocator (`usage::place`, pure): by priority, each in the widest of full / short / icon that fits, skipping what can't fit without blocking smaller ones; tabs first, always.
- [x] Providers: `agents` (from status, no IO), `claude` (tokens today and the 5-hour block, ccusage-style, from `$CLAUDE_CONFIG_DIR`/`~/.claude/projects`, read incrementally; each message once), `codex` (tokens today and its rate-limit windows, from `$CODEX_HOME`/`~/.codex/sessions`), `command`, `clock`, `host`, `system` (load). Not yet: memory/CPU, cost (needs a price table).
- [x] Remote-aware: `command`, `host` and `system` segments with `on = "workspace"` (the default for host and system) read on the active workspace's machine through `machine.exec`, and read again at once when that changes; their details say which machine.
- [x] Config: `[usage] enabled`, `[[usage.segment]] provider/priority/interval/format/short/cmd/timeout` (fields like `{tokens_today}`); none given means agents, claude, codex. Live-reloadable. (`on` comes with remote-aware.)
- [x] Claude's real limits: Claude Code gives them only to its status line command (`rate_limits.five_hour` / `.seven_day`: `used_percentage`, `resets_at` on stdin), as claude-hud reads them. `corgi claude-statusline -- <command>` saves them (`claude-usage.json` in the state folder) and runs your status line with the same input. The claude segment uses them (the tightest window); without them, an estimate only if `limit` asks for one, else tokens with a pointer to the setup.
- [x] Percent left, by default (`show = "left"`; or `"used"`, `"tokens"`): codex from its own rate-limit windows (the tightest; a window past its reset is fresh); claude of `limit`, the 5-hour block's token budget, since Claude Code keeps no limit locally: `"max"` (the biggest finished block this week, ccusage's estimate; the default) or a number. Fields `{left}`, `{used}`, `{limit}`, `{window}` for `format`. `C-b u` draws a bar per limit (red under 20% left) and says where the limit comes from.
- [x] Each segment is a block in its own colours: `bg` / `fg` per segment (`bg = "none"`: plain text), else the provider's (agents: accent, claude: its coral, codex: badge, command: active border, others: border). The core version left the tab bar for `C-b u`.
- [x] Details, keyboard first: `C-b u` / `display-usage` shows every segment's full breakdown (USAGE badge); clicking the segments opens the same.

### M8: Remaining parity features

- [x] Copy mode (`copy.rs`, `app/copymode.rs`):
  - [x] `C-b [` (`copy-mode`), `C-b PageUp` (`copy-mode -u`). The pane is shown from copy mode's row by libghostty's own viewport, so it keeps its colours; corgi draws the cursor, selection, `[row/rows]` and the search over it. The rows' text comes from libghostty's formatter (history first, one line per row), read again on each key, so the pane stays live.
  - [x] vi keys: `h j k l`, `w b e`, `0 ^ $`, `gg G`, `H M L`, `C-u C-d C-b C-f`, PageUp/PageDown, counts (`5j`); `/ ?` search with smart case, wrapping round, `n N`; `v` / `V` select, `o` the other end; `y` / Enter copy (to corgi's buffer and the host's clipboard over OSC 52, through tmux too) and leave; Esc clears, then leaves; `q` leaves; the wheel scrolls. COPY badge.
  - [x] `C-b ]` (`paste-buffer`): the last copy, as a paste (bracketed when the program wants).
  - [x] `W B E` (WORDs), `{ }` (paragraphs), `f F t T` with `; ,`, `C-v` blocks (as wide as the column the cursor keeps to; `$` to every line's end).
  - [x] The mouse: press and drag in a pane selects (copy mode starts; past the top or bottom it scrolls), letting go copies; a double click copies the word, a triple click the line.
  - [ ] Still: an emacs table, keys from `[[keys.bind]] table = "copy-mode"`.
- [ ] Mouse:
  - Click to focus. Drag split borders. Drag the sidebar, and reorder tabs and workspaces.
  - [x] Programs that ask for mouse reports (DEC 9/1000/1002/1003; any format, by libghostty's encoder) get clicks, drags, motion and the wheel in their own coordinates; a button keeps its drag and release in the pane it was pressed in; Shift held keeps the mouse corgi's (select, the menu). Full-screen programs that didn't ask (less, man) get the wheel as arrow keys. Otherwise the wheel scrolls; drag selects and copies; double-click a word, triple-click a line (copy mode).
  - Ctrl+click opens links and core addresses (`t7`, `vps/t7`). `mouse_capture = false` turns all of this off.
- [x] Paste: bracketed paste. Images (`clipboard.rs`, `app/paste.rs`), as herdr does for remote panes: for a pane on another machine (`paste.images = "remote"`; `"always"`, `"off"`), an empty paste (the clipboard holds only an image) sends this machine's clipboard image, and a dropped image file (a paste of its path) sends that file, beside the terminal with `terminal.put`; the path it landed at is pasted instead. `C-b C-v` (`paste-image`) does it on purpose, for any pane. The clipboard: osascript on macOS, wl-paste / xclip on Linux, or `paste.image_command`; images are recognised by their bytes, up to 8 MB.
- [~] Notifications (`[notifications]`, pulled forward):
  - [x] Toasts over the panes (`position`: top-left/center/right, bottom-left/center/right; default top-right) for an agent that needs you or finished unseen (OSC 7501), and for a program's own OSC 9 / OSC 777. Nothing for the pane you're looking at; looking at it dismisses its toasts; click one to go there. A newer notice about the same pane replaces the older one.
  - [x] Passed on to the host terminal while its window isn't focused (or always, if it never reports focus): `host = auto | osc777 | osc9 | osc99 | bell | off`. `auto` goes by TERM_PROGRAM / TERM / LC_TERMINAL / KITTY_WINDOW_ID, else the bell. Wrapped for tmux passthrough inside tmux. One per pane within 2 s.
  - [ ] OSC 9/777 from panes in windows you can't see: they aren't attached, so only their agent status reaches corgi.
  - [ ] kitty's OSC 99 from panes (libghostty-vt only reports 9 and 777).
  - [ ] Sounds: bell or configured files, per agent. A message log to look back at past toasts.
- [x] Remote machines (`machines.rs`, `app/machines.rs`):
  - `machine.list` polled every 2.5 s (core sends no event when a line drops; remote terminals just vanish from its lists, and come back as a `missed` snapshot).
  - Machines head the sidebar and chooser with their line's state (● connected, ◌ connecting, ! needs login / update, ✕ unreachable); a toast when a machine with workspaces goes down or comes back.
  - A down machine's panes keep their last screen, greyed, with the state, how long, and core's reason; they aren't attached until the line is back, then reattach.
  - `C-b M` (`choose-machine`): the machines, their targets, sessions, versions, workspace counts, and what's wrong with the selected one. Enter connects now (`machine.connect`), `n` adds, `w` a new workspace on it (in its home), `r` renames (`machine.rename`; its panes' addresses `old/t3` → `new/t3` in the layout), `d` forgets it (`machine.rm`, asked first; its workspaces close here, its programs keep running there), `l` / `u` run `hc machine login` / `update` in a popup.
  - `machine connect|login|update|rename|remove|new-workspace [name]` commands (default: the current workspace's machine); a machine row's menu has them.
  - Git facts (branch, checkout name, worktree grouping) for workspaces on another machine: one `machine.exec` per machine (whose line is up) each tick, kept apart from this machine's by machine name.
  - e2e: a second core session reached with `--via` a script that drops its line on a flag file.
- [x] Worktrees (`worktrees.rs`, `app/worktrees.rs`):
  - `C-b W` (`choose-worktree`): the current workspace's repository's worktrees (`git worktree list --porcelain` through core's `machine.exec`, on the workspace's machine): branch, path, main, changes, locked; ● for those open as workspaces. Enter opens one as a workspace (or goes to its workspace); `n` makes one (New Workspace with the worktree switch on); `d` removes one, asked first, warning of uncommitted changes and that its workspace closes too (forced when it has changes); the main checkout can't be removed.
  - Names follow the workspace, as in herdr: without a name of its own, a workspace is named after the folder its first pane is in now (the git checkout's name when inside one: `~/src/corgi/crates` → `corgi`), and its branch and grouping follow that folder; a change of folder looks them up at once.
  - Grouping: each local workspace's repository (git's common dir) comes with its branch; a workspace in another's worktree is listed right under it in the sidebar and chooser, marked ↳.
  - Fixed on the way: the slow tick (branches) ran only when the main loop had a quiet moment, which a busy session might not give it.
  - Still: remote workspaces' grouping (the tick reads local git only).
- [x] Popups and custom commands, as commands you bind with `[[keys.bind]]` (no separate `[[keys.command]]`):
  - `display-popup [-E] [-w W] [-h H] [-d dir] [-T title] [cmd]` (`app/popup.rs`): its own core terminal (`terminal.run`, labelled `corgi.popup`, never in the layout), centred over everything, every key to it; `-E` closes it when the program ends, else it says so and a key closes it; `C-b x` closes it, `C-b C-b` sends the prefix; the terminal is removed after. POPUP badge.
  - `run-shell <line>`: in the background on the workspace's machine (`machine.exec`), its first line of output (or its error) in the status line.
  - `split-window <cmd>` / `new-window <cmd>` for a pane or window running something.
  - Shell commands keep their quoting (`sh -c 'a; b'`).
- [x] Narrow layout (`ui.narrow_width`, 64 columns or fewer; 0 never), as herdr's mobile layout (`app/narrow.rs`): drawn from a narrowed copy of the config (`Config::narrowed`: plain panes without boxes, insets or titles; no sidebar). A two-row header instead of the tab bar: the mode (when not NORMAL), the workspace with its state and machine, the window (`2/3`) and pane (`pane 1/2`); then the agents (waiting, working, done) or a message, and usage in what's left; a "switch" button on the right (`C-b w`, marked when an agent waits) opens the chooser over the whole screen. Only the focused pane shows (as if zoomed); resizing wide again brings the usual layout back. The e2e harness can resize its terminal (`Corgi::resize`, `spawn_sized`).
- [x] `C-b e` (`edit-scrollback`): the pane's scrollback (libghostty's rows, history first) in a temp file, opened in `$VISUAL` / `$EDITOR` (else `vi`) at its end, in a popup; the file goes with it.
- [ ] Peek: open a worker terminal (a child agent with no pane) as a popup, with an option to keep it as a window.

### M9: Hardening and release

- [x] Recovery tests (e2e): a core restart (`hc server restart`, the handover `hc update` does: the same shells, their state and screens kept); a pane's shell killed (`lost` → `restarted`: a new shell in the same terminal, typed into); the core stopped and started again (kept terminals parked, then new shells: the same windows and terminals); a remote line drop (`a_machine_reached_by_a_command`). `hc update` itself isn't run: it would replace the installed `hc` every session uses.
- [x] Performance (the review in `docs/dev/optimizations.md`, worked through on `m9-perf`):
  - Idle: no wakeups for key hints that won't open (prefix mode with hints off took 0.56 CPU seconds in 3; e2e `idle_doesnt_spin` keeps idle and that under 0.15); sleeping redraws only for what changed; messages expire in one place; unchanged git facts and machine polls don't redraw.
  - The 10 s tick actually ticks (its timestamp was reset every loop pass); one git scan at a time, folders deduplicated, git killed on timeout, other machines asked side by side.
  - `process.rs`: bounded program runs (usage commands, the clipboard reader): output read as it comes, one deadline for exit and output, the process group killed.
  - Usage segments that follow the workspace: an explicit target with a revision (obsolete remote reads dropped, a down machine not read here, the segment's own timeout).
  - Copy mode re-reads the scrollback only when it changed (`Vt::revision`); views and backups written off the UI; image encoding off the UI, one upload at a time; log polling rediscovers files once a minute, reads 4 MiB at a time, prunes what's gone.
  - Cheaper: base64 decoding in place; the tree's worktree grouping in one indexed pass (8 million folder lookups for 400 workspaces, now 400); the sidebar's agents straight from the model.
  - Backpressure: output is credited back to core once the emulator has it, batched per loop pass; queued messages handled for at most a frame before drawing. A flood test (`--ignored a_flood`: three panes of `yes`; seven measured too) shows about 0.1 s for another pane to echo and about 20 MB, before and after: here core relays slower than corgi parses, so this is a bound, not a speed-up.
  - Not done: caching the tree rows between draws, and the status line's clone of the active workspace (no measured cost yet).
- [ ] Windows: named pipe, 9001 win32-input encoding, ConPTY quirks.
- [x] Migration (`migrate.rs`): `corgi migrate [--from <herdr config>] [--print] [--force]` reads `~/.config/herdr/config.toml` and writes a corgi config (`--config`, else the usual place) on the herdr preset: the prefix and every key you set there as `[[keys.bind]]` (herdr notation to tmux's; `1..9` ranges spread out; herdr's 1-based tabs to corgi's 0-based windows), the theme where corgi has it, the sidebar (hidden / collapsed, width, agent sort, agent row tokens), tab bar position, pane borders and gaps, toasts, the narrow width, mouse, confirm-close, indicators, worktrees. What has no counterpart (plugin bindings, navigate-mode keys, sounds, unknown themes, herdr's own sections) is listed in the file and on stderr. It won't replace a corgi config without `--force`, which keeps the old one as `.bak`. New commands for it: `next-agent -a` / `-p`, `previous-agent`, `select-agent N` (herdr's agent keys: any agent, in the sidebar's order).
- [x] Packaging: release builds on a tag (Apple Silicon macOS, Linux x86_64 and arm64) and the Homebrew formula in hong-yi/homebrew-tap (switched on once the repo is public); see docs/dev/release.md.
- [ ] An install script that also installs `hc` when missing; Windows builds; third-party notices in the tarball.
- [x] Docs (see `docs/OUTLINE.md`): `docs/user/` (29 pages: getting started, concepts, how-to, reference, help), `docs/dev/` (architecture, protocol, emulation, layout, input, UI, usage, testing, building, release), README, CHANGELOG (Unreleased), CONTRIBUTING. Still: the plugins page (M10), ADRs 0004–0009 named in the outline, a screenshot for the README, and a versioning policy (release.md has it as a proposal).

### M10: Plugins (future)

Plugins are programs, not code loaded into corgi: a folder with a manifest, commands corgi runs with context in the environment, and a way back in. The same model as herdr's plugins (`herdr-plugin.toml`), so herdr plugins that only use actions and hooks can work unchanged. Keyboard first: every plugin action is a command, so it can be bound, run from `C-b :`, and found in `C-b m`. Not planned: in-process scripting (Lua, WASM) or dynamic libraries; they'd need sandboxing and a stable API for little gain over processes.

1. Remote control (useful on its own):
   - [ ] corgi listens on a socket per session (in the runtime folder); `corgi cmd "<command>"` runs any corgi command in the running corgi, like `tmux <command>`, and `corgi list --json` prints workspaces, windows, panes and agents.
   - [ ] Commands that act on something other than the focus take a target (`-t workspace:window.pane`).
2. Manifests and hooks:
   - [ ] `corgi-plugin.toml`, and herdr's `herdr-plugin.toml` read as it is: `id`, `name`, `version`, `min_*_version`, `platforms`, `[[build]]`, `[[startup]]`, `[[actions]]` (with `contexts`: workspace, window, pane), `[[events]]`.
   - [ ] `corgi plugin link <dir>`, `corgi plugin install <github owner/repo[/path]>` (a checkout, its build commands, a trust prompt showing the manifest), `list`, `remove`. Each plugin's config and state folders kept apart.
   - [ ] Actions as commands (`plugin <id>.<action>`), bindable with `[[keys.bind]]`, in `C-b m` under Plugins for their contexts, and in `C-b ?`.
   - [ ] Environment for every command: the corgi binary and socket, plugin id, root, config and state folders, and the context as JSON (workspace, window, focused pane, worktree, agent, selection); herdr's `HERDR_*` names too, for herdr plugins.
   - [ ] Events: agent waiting / finished, notification, workspace / window / pane created and closed, worktree created, focus changed, config reloaded; each runs its hooks with the event as JSON. A slow or failing hook is logged, never blocks the UI.
   - [ ] Usage segments from plugins (`[[usage]]` in the manifest, read like a `command` provider).
3. Panes and links (with M8's popup terminal and link detection):
   - [ ] `[[panes]]` with `placement`: popup (a modal terminal over everything), split, window, zoomed.
   - [ ] `[[link_handlers]]`: a pattern and an action, for links opened from copy mode or the keyboard.
   - [ ] For herdr plugins that call herdr's own CLI or socket (`herdr workspace list`, …): a shim mapping what has a corgi equivalent; the rest refused clearly.

---

## 4. Parity matrix (old herdr TUI → corgi)

| Feature | Old herdr | corgi | Milestone |
|---|---|---|---|
| Workspaces, tabs, panes, BSP splits, zoom, swap, resize | yes | Workspaces / windows / panes, plus tmux preset layouts | M3 |
| Detach / keep running | own server | core (`keep:true`) | M1 |
| Session restore after restart | `session.json` + 48 snapshots | core keeps terminals; layout in doc + local backups | M3 |
| Named sessions | `herdr --session` | `HC_SESSION` / `--session` | M0 |
| Prefix keybinds | `ctrl+b`, herdr keys | tmux preset by default, herdr preset available | M5 |
| Navigate / resize / copy modes | yes | prefix tables, repeatable keys, copy mode | M5/M8 |
| Key hints after the prefix | mode bar hints only | **New:** key-hints dialog listing the next keys, generated from the key table | M5 (widget in M0) |
| Sidebar: spaces + agents panel | yes | same, plus core labels as tokens | M4 |
| Tab bar right-side entries (`zoom`, `hostname`, `datetime`, `text`, `command`) | yes | Replaced by usage segments (a superset) | M7 |
| Agent detection | screen manifests + process detection + hooks | **Changed:** OSC 7501 only (core rule); identity from `foreground` | M4 |
| Overlays: rename, confirm, help, goto, settings, context and global menus | yes | yes, plus new workspace/window dialogs and a command prompt | M6 |
| Worktrees | built in | via `machine.exec git` | M8 |
| Remote (`--remote`, saved machines) | herdr remote | core machines (`vps/t7`) or `ssh host hc connect` | M8 |
| Mouse, copy-on-select, links | yes | yes | M8 |
| Toasts, sounds | yes | yes | M8 |
| Custom commands and popups | yes | yes | M8 |
| Mobile layout (64 columns or fewer) | yes | narrow layout | M8 |
| Themes (18 built-ins, auto-switch) | yes | port | M4 |
| Plugins (`herdr-plugin.toml`) | yes | **Deferred:** custom commands + `hc` CLI cover most uses | after v1 |
| Kitty graphics passthrough | yes | **Deferred** | after v1 |
| Integration installer, manifest updates | yes | **Dropped:** agents report status themselves | — |
| Onboarding, release notes, announcements | yes | Minimal "what's new" from CHANGELOG | M9 |
| Live handoff updates | experimental | not needed: core updates keep terminals | — |
| Pane metadata tokens (`report-metadata`) | yes | core labels (`hc label t7 k=v`) | M4 |

---

## 5. Usage status in the tab bar

**Goal.** Show useful status in whatever space the tab bar has left, without ever pushing out window tabs.

**Allocator.** This is a pure function: `(width, tabs_width, segments[]) -> placed[]`.

1. Lay out the window tabs first. When they overflow, they scroll, and no usage is shown.
2. `free = width - tabs_width - gap(1) - 2`.
3. Sort segments by `priority`, highest first. Each segment offers renderings from widest to narrowest: `full` → `short` → `icon`.
4. Place segments greedily from the right edge. For each segment, try `full`, then `short`, then `icon`. If none fits, skip it and try lower-priority ones that might be smaller.
5. Separators count toward the width. Stale values (older than 3× the interval) render dimmed.

**Providers** (`trait UsageProvider { id, interval, run(ctx) -> Result<Segment> }`):

| Provider | Shows | Source |
|---|---|---|
| `agents` | `●2 ●1 ✓3`: working / blocked / done counts | core status (no IO) |
| `system` | CPU %, memory %, load | `sysinfo` locally; `machine.exec` for a remote workspace |
| `claude` | Today's tokens and cost, current 5-hour block, burn rate | Claude Code local transcript logs (`~/.claude/projects/**/*.jsonl`, the same approach as ccusage). Plan-limit percentage is **open**: no supported local source; revisit. |
| `codex` | Today's tokens | `~/.codex/sessions` logs |
| `command` | Any shell output (first line) | `{cmd, interval, timeout}`; parity with the old tab-bar `command` entry |
| `clock`, `host` | Time and hostname | local |
| `core` | Live terminals, machines with problems | `server.status`, `machine.list` |

Example config:

```toml
[[usage.segment]]
provider = "claude"
priority = 90
interval = "30s"
format   = "{tokens_today} · ${cost_today:.2} · {block_remaining}"
short    = "${cost_today:.0}"

[[usage.segment]]
provider = "command"
cmd      = ["gh", "pr", "status", "--json", "..."]
interval = "2m"
priority = 20
```

---

## 6. Testing strategy

- **Unit tests (pure):**
  - layout operations and geometry
  - keymap state machine (prefix, repeat, timeout)
  - events reducer (`missed`, seq)
  - status rollup
  - usage allocator
  - form validation
  - doc merge on conflict
- **Render snapshots:** ratatui `TestBackend` + `insta` for the tab bar, sidebar, dialogs (including the key-hints dialog) and pane borders at several widths and in both themes.
- **Protocol tests:** codec round-trip, partial frames, early-frame buffering, credit.
- **End to end:** a real `hc` in an isolated `HC_SESSION` with scratch XDG directories under the target directory. Drive corgi in a PTY (`expectrl` or `portable-pty`) and assert on the screen through `corgi-vt`. Cover core restart in the middle of a session.
- Keep the suite fast, in seconds. No real-agent tests in CI.

---

## 7. Risks

| Risk | Mitigation |
|---|---|
| The Zig/Ghostty build makes contributing and CI harder | M0 spike. Cache the build artifacts. Keep `corgi-vt` as the boundary so the backend can be swapped. |
| Key re-encoding mistakes (kitty protocol, 9001, alt/meta) | Use the emulator's encoder for the inner program's modes. Table-driven tests per mode. |
| Agents that never emit OSC 7501 show `unknown` (a regression from the old detection) | Document it clearly. Show the foreground program name. Upstream agent integrations are the right fix. |
| Doc conflicts when two clients edit at once | Compare-and-swap with a three-way merge per window. Local backups. |
| Usage sources are unofficial or change format | Providers are isolated and fail soft (hidden or dimmed). Version-detect the log formats. |
| Core protocol is a preview (generation 1, 0.x) | Pin a minimum core version. Check `methods` and `features` at hello. The API only grows. |

---

## 8. Reference reading (upstream, for ideas only)

- herdr-core: `schema/README.md`, `schema/hc.openrpc.json`, `crates/herdr-core/src/{wire,rpc,server,viewers,relay,attach,status,terminals}.rs`, `docs/{commands,machines,templates,security}.md`
- herdr-app (reference client): `src/hc.rs` (client and reducer), `src/terminal.rs` (attach and credit), `src/vt.rs` (ghostty wrapper), `src/mirror*.rs` (doc sync), `src/model.rs` (status rollup), `docs/labels.md`
- old herdr TUI (parity spec): `docs/next/website/src/content/docs/*.mdx`, `src/config/keybinds.rs`, `src/config/model.rs`, `src/layout.rs`, `src/client/shell/{render,sidebar,tabs,overlays,mouse,input}.rs`
