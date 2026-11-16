# corgi documentation outline

The docs have two halves:

- `docs/user/`: for people using corgi.
- `docs/dev/`: for people working on corgi.

Each user page is one of four kinds:

- tutorial: learn by doing
- how-to: reach a goal
- reference: look something up
- explanation: understand why

---

## README.md (repo root)

1. What corgi is, in one paragraph: a tmux-style TUI for herdr-core. Core keeps the terminals; corgi arranges them.
2. Screenshot or GIF: sidebar, tab bar with usage segments, split panes, and the New Window dialog.
3. Install: one line, plus how to get `hc` if it is missing.
4. Quick start: `corgi`, then `C-b c`, `C-b %`, `C-b d`.
5. Coming from tmux / coming from herdr (links).
6. Links to the docs, the changelog, and the license.

---

## User guide (`docs/user/`)

### Getting started (tutorial)
- `install.md`
  - Platforms: Linux, macOS, Windows.
  - Installer script, Homebrew, cargo.
  - Installing and updating `hc`, and the minimum core version.
  - Verifying the install with `corgi --version`, which shows the core version too.
- `first-session.md`
  - Starting corgi and how it connects to (or starts) core.
  - Your first workspace and windows.
  - Splitting, moving between panes, zoom.
  - Pressing `C-b` and reading the key-hints dialog: you don't need to memorise keys first.
  - Detach, reattach, and proof that the programs kept running.
  - Running an agent and watching its status dot.

### Concepts (explanation)
- `concepts.md`
  - Core vs corgi: who owns what, and why closing corgi leaves terminals alive.
  - What a pane shows while attaching, after the program exits, while a kept terminal recovers, and while reconnecting.
  - Workspace › Window › Pane, compared with tmux session/window/pane and herdr space/tab/pane.
  - Terminal addresses (`t7`, `vps/t7`), and how panes point at core terminals.
  - Core sessions (`HC_SESSION`) vs corgi workspaces.
  - Where layout lives: the core document `corgi` vs local view state. Using several corgi clients at once (shared focus, as in tmux).
  - How labels make the layout recoverable, and what "unplaced" terminals are.
  - Panes close when their program exits; `[exited]` after the last one.
- `agent-status.md`
  - OSC 7501 and the states working / blocked / done / idle / unknown.
  - "Done" vs "seen", and the `herdr.seen` label.
  - Rollup from pane to window to workspace.
  - Why there is no screen scraping, and what to do about agents that never report.

### Using corgi (how-to)
- `workspaces-and-windows.md`
  - Create, rename, reorder and close workspaces and windows.
  - The New Workspace / New Window dialogs, field by field.
  - Quick-create vs dialog.
- `panes-and-layouts.md`
  - Splits, focus, swap, resize (including repeat keys), zoom.
  - Preset layouts; break and join a pane.
  - What happens when a pane's program exits.
- `navigation.md`
  - The workspace tree, in the sidebar and in the `C-b w` / `C-b s` chooser: folding (shared), filtering, jumping.
  - Focusing the sidebar (`C-b ←` from the left-most pane), its keys, and resizing it by keys or by dragging its edge.
  - Tree chooser, workspace chooser, go-to-agent.
  - Status filters, the "Unplaced" terminals list, peek.
- `copy-mode.md`
  - vi and emacs keys, search, selection types.
  - Paste buffer, OSC 52 clipboard.
  - Editing scrollback in `$EDITOR`.
- `mouse.md`
  - Focus, dragging borders, tab and workspace reorder.
  - Selection, links and addresses.
  - Turning mouse capture off.
- `remote-machines.md`
  - Adding a machine (dialog, or `hc machine add`).
  - Machine states, and workspaces on another machine.
  - The `ssh host hc connect` alternative.
  - What you see while disconnected.
- `worktrees.md`: create, open and remove git worktree workspaces; how they group.
- `notifications.md`: toasts, sounds, delivery modes, and suppressing them for the visible pane.
- `custom-commands.md`: shell, pane and popup commands, and binding them to keys.
- `plugins.md` (M10): what a plugin is (a folder, a manifest, programs), installing and trusting one, writing one (actions, events, usage segments, panes), the environment and context JSON, remote control (`corgi cmd`, `corgi list --json`), and running herdr plugins.
- `usage-status.md`
  - What each built-in usage segment shows and where its data comes from.
  - Adding a `command` segment.
  - Priority and how segments shrink or hide when space is short.
  - Privacy: which local files are read.
- `themes.md`: the 7 built-in themes, `auto` (light/dark from your terminal's background), `[theme.custom]` colour names, and status indicator styles. Themes colour corgi's chrome, never your panes.
- `migrating-from-herdr.md`
  - The concept map.
  - `corgi migrate`; the herdr keybind preset.
  - What changed: status detection, plugins.
  - What was dropped.
- `coming-from-tmux.md`
  - Keys that match tmux, and keys that differ.
  - Importing parts of `.tmux.conf`.
  - Commands at the `:` prompt.

### Reference
- `pane-tabs.md`: what each pane's tab shows (`panes.title`), where an agent's summary comes from (OSC 7501 `title`/`msg`, the terminal title), shapes and the fonts they need.
- `keybindings.md`: tables (`prefix`, `root`, `resize`, your own), `[[keys.bind]]`/`unbind`, repeat keys, presets; every default key, table by table (`prefix`, `root`, `copy-mode-vi`, `copy-mode-emacs`, `resize`, dialogs), for the tmux preset and the herdr preset.
  - **Key-hints dialog.** What it shows, and that it is generated from your bindings. The next key runs as normal; `Esc` cancels; `?` opens all keys. Which tables show it. The narrow-terminal strip. Turning it off or delaying it.
- `actions.md`: every action name, its arguments, and where it is reachable (key, mouse, prompt, menu).
- `commands.md`: command syntax (quoting, options, `%%` templates, `#P #I #W #S`) and every command with its flags (`new-window`, `split-window`, `select-layout`, `confirm-before`, `switch-client -T`, …), with their tmux aliases.
- `configuration.md`
  - File locations; reload behaviour.
  - Every section and key, with type, default and example: `[keys]` (including `hints` and `hint_delay`), `[tab_bar]` (position, style, span, padding), `[panes]` (style), `[mode]` (badge labels), `[ui]`, `[ui.sidebar]`, `[ui.tab_bar]`, `[theme]`, `[usage]`, `[[usage.segment]]`, `[notifications]`, `[core]`.
- `sidebar-tokens.md`: row templates (`workspace_row`, `agent_row`), the tokens `{state} {workspace} {branch} {machine} {window} {pane} {agent} {title} {label:KEY}`, and agent sorting.
- `usage-providers.md`: per provider, the fields available in `format`, the options, and the default interval.
- `cli.md`: `corgi` flags and subcommands (`attach <addr>`, `doctor`, `migrate`, `claude-statusline -- <cmd>`, `cmd`, `list`, `plugin …`, `--default-config`, `--session`, `--socket`), environment variables, and exit codes.
- `files-and-paths.md`: config, view state, logs, backups, and the core document name, per OS.
- `labels.md`: the `corgi.*` and `herdr.*` labels corgi reads and writes on core terminals.

### Help
- `troubleshooting.md`
  - Start with `corgi doctor`: whether corgi reaches core, the core's version and features, and the layout doc.
  - `hc` not found, or core too old.
  - Repeated status lines and keys going nowhere: corgi running inside its own pane. Why corgi refuses, `CORGI_NESTED`, and finding a stray one with `corgi doctor`.
  - Wrong keys inside an app (kitty protocol, Alt/Meta, tmux-in-corgi).
  - Colours and true color.
  - Panes stuck "recovering".
  - Layout conflicts, and restoring from backups.
  - Where the logs are and how to collect them. `CORGI_RECORD=<dir>` records what each pane receives, and `corgi-vt`'s `dump` example replays it.
- `faq.md`

---

## Developer guide (`docs/dev/`)

- [optimizations.md](dev/optimizations.md): prioritized performance proposals, source references, measurements, and regression checks.
- `architecture.md`
  - The process and crate map (`hc-proto`, `corgi-vt`, `corgi`).
  - The data flow diagram.
  - The app loop: one state owner, actions, and render on dirty.
  - The rule that deciding code stays separate from IO.
- `protocol.md`: how corgi uses herdr-core.
  - Connecting and hello: version, method and feature gates.
  - Framing, channels and credit.
  - Attach lifecycle: first paint, resize ownership, FOCUS, FACTS, AUTOMATIC_INPUT, 9001.
  - Events reducer: seq, `missed`, heartbeat.
  - Recovery after a core restart or update: `Client::closed`, `Client::reconnect`, and what the supervisor redoes.
  - Documents and the compare-and-swap merge.
  - Links to the upstream `schema/README.md`.
- `terminal-emulation.md`
  - The `corgi-vt` boundary and API.
  - Cells to ratatui buffer.
  - Key and mouse encoding for the inner program's modes.
  - Replies, title and bell handling; scrollback and search.
- `layout-engine.md`
  - The BSP tree, operations and invariants.
  - The geometry algorithm and minimum sizes.
  - Preset layouts.
  - The persisted schema and how its versions migrate.
- `input-and-keymaps.md`
  - Key notation; tables; prefix and repeat timing.
  - Adding an action, and wiring it to keys, the prompt and menus.
  - How the key-hints dialog is built from a key table: each action's category and label, custom command descriptions, and keeping hints in sync when bindings change.
- `ui-components.md`
  - Tab bar, sidebar, overlays, the form framework, theming tokens.
  - Writing a new dialog.
  - Dialogs: drawn over the whole window and placed with `ui::centered` (the rule for every new dialog).
  - The key-hints widget: the layout plan (groups kept whole across columns), the overflow footer, the strip fallback, and snapshot tests.
- `usage-providers.md`
  - The provider trait and hub scheduling.
  - The allocator algorithm.
  - Writing a provider, including the remote-aware context; failure and staleness rules.
- `testing.md`
  - Unit, snapshot (`insta`), protocol and end-to-end tests.
  - Isolated `HC_SESSION` and scratch directories: short paths for the 104-byte socket limit, no relative `HC_SOCKET`, and teardown that stops leftover holders.
  - Never touch the real core.
  - Speed budget.
- `building.md`: toolchain, the Zig/Ghostty build, `just` recipes, cross-compiling, Windows notes.
- `release.md`: versioning, CHANGELOG format, the minimum core version bump, packaging, the Homebrew tap.
- `adr/`, one file per decision in `PLAN.md` §1:
  - `0001-vt-emulator.md`
  - `0002-own-protocol-client.md`
  - `0003-transport.md`
  - `0004-layout-in-core-doc.md`
  - `0005-herdr-app-interop.md`
  - `0006-naming.md`
  - `0007-status-osc7501-only.md`
  - `0008-async-model.md`
  - `0009-usage-sources.md`
- `CONTRIBUTING.md` (root): branches, commit style, `just check` before a PR, and the file-size guideline.
