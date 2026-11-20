# Changelog

All notable changes to corgi. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
see [docs/dev/release.md](docs/dev/release.md) for how entries are written.

## [Unreleased]

### Changed

- A workspace in a linked git worktree is named after its repository too, as `corgi/feat-login` instead of `feat-login`. A name you give it still wins.

## [0.1.2] - 2026-11-20

### Added

- The sidebar tree and the tree chooser (`C-b w`) draw guide lines (`├─`, `└─`, `│`) showing which windows and panes belong to which workspace.

### Fixed

- The sidebar highlights the focused pane, not its workspace. When the pane's window or workspace is folded, the highlight goes on that row instead, and the rows above it stay bold.
- A workspace made just after corgi starts could get the same id as the first one, so the two shared a name in the layout and segments that follow the workspace (`on = "workspace"`) kept reading the first one's machine.

## [0.1.1] - 2026-11-18

### Added

- The sidebar's cursor reaches the agents list: `↓` past the tree's last row (or `a`) moves into it, `Enter` goes to the agent, `m` opens its menu.
- `new-workspace -w`: a workspace in a new git worktree of the focused pane's folder, on a branch named with random words, with no dialog. Bind it to `N` to skip the New Workspace dialog.

### Fixed

- Under the herdr preset, windows are numbered from 1, so `prefix 1`–`9` pick the window labelled with that number (tab bar, tree, menus, status, `#I`). `select-window -t` stays 0-based, so existing configs keep working.

## [0.1.0] - 2026-11-17

The first release: everything so far. corgi needs herdr-core 0.1.19 or newer. Builds for Apple Silicon macOS
and Linux (x86_64, arm64) are attached to the release.

### Core connection

- Talks to herdr-core through `hc connect` (which starts core when needed), or a socket with `--socket`; `--session`
  / `HC_SESSION` picks the core session.
- Refuses a core that speaks another protocol generation, is older than 0.1.19, or lacks a method corgi needs,
  and says why. A missing `hc` gives the install command.
- Survives core restarts and updates: corgi reconnects every second, resubscribes, re-reads the layout and
  reattaches its panes. A silent line (45 s without events or heartbeats) is treated as dropped.
- Output is credited back to core only once a pane's emulator has used it, so a flooding program can't grow
  corgi's memory or hold up the other panes.
- `corgi doctor` prints the core's version, features, terminal count and the layout document's state, and finds
  corgi running inside its own panes.

### Panes and terminals

- Each pane is a core terminal drawn by libghostty-vt, the same parser core uses: true colour, wide characters,
  synchronized output, backgrounds painted to the pane's border.
- Keys are encoded for the program's modes (application cursor keys, kitty keyboard protocol, Alt as ESC prefix);
  pastes are bracketed when the program asks.
- Terminals keep running when corgi quits or detaches (`C-b d`); reattaching restores their screens. A pane closes
  when its program exits; after the last one corgi prints `[exited]`.
- `corgi attach <addr>` shows any core terminal (`t7`, `vps/t7`).
- corgi refuses to start inside its own pane (`CORGI_NESTED=1` forces it) and warns on panes running corgi.

### Layout

- Workspaces › windows › panes, stored as the core document `corgi`, shared by every corgi on that core, with
  local backups of the last 20 versions. Labels on each terminal put the layout back after a lost write or a
  conflict.
- tmux keys for windows (`c n p 0-9 , &`), panes (`% " o ; z x { } ! Space`, arrows and `h j k l`), resizing with
  repeat keys, and the presets even-horizontal, even-vertical, main-horizontal, main-vertical and tiled
  (`M-1..5`, `select-layout`).
- New terminals start at their final size.
- A narrow layout for screens of 64 columns or fewer (`ui.narrow_width`): a two-row header and one pane at a time.

### Keys and commands

- A tmux-style command language for bindings, `[[keys.bind]]` and the `:` prompt (history, Tab completion,
  "did you mean"), with tmux aliases.
- Key tables `prefix`, `root`, `resize` and your own (`switch-client -T`); presets `tmux` (default), `herdr` and
  `none`; `keys.unbind`.
- A key-hints panel after the prefix, generated from your bindings (`keys.hints`, `hint_delay`,
  `hints_position`); `C-b ?` lists every binding and runs the one you pick; `C-b q` shows pane numbers.
- Agent navigation: `C-b a` / `C-b A`, `next-agent`, `previous-agent`, `select-agent N`.

### Screen and chrome

- A sidebar with the workspace tree (foldable, shared with the `C-b w` / `C-b s` chooser) and every agent below it;
  focusable with `C-b ←`, resizable by keys or by dragging its edge; `show`, `collapsed` or `hidden`.
- A tab bar (top or bottom, plain or boxed) with a mode badge, window tabs with status dots, and a `+` button.
- Pane boxes, shared dividers or gaps; pane titles as tabs, lines with an activity circle, or breadcrumbs on their
  own row; a configurable inset.
- Seven built-in themes, `auto` light/dark from your terminal's background, and `[theme.custom]` overrides. Themes
  colour corgi's chrome, never your panes.
- A TOML config with a default for every key (`corgi --default-config`), warnings for unknown keys, and live
  reload with `C-b R`.

### Dialogs and menus

- New Workspace (`C-b N`) and New Window (`C-b C`) dialogs: folder completion, machine, first program (shell,
  template or agent), initial layout, and an optional new git worktree.
- Rename dialogs for workspaces, windows and panes; close confirmations that list agents that would end
  (`ui.confirm_close`).
- Action menus from the keyboard (`C-b m`, `m` in the sidebar) or a right-click.

### Agents and notifications

- Agent status from OSC 7501 only: working, waiting, done (unseen, shared through `herdr.seen`), idle, unknown;
  rolled up to windows and workspaces.
- Toasts for agents that need you or finished, and for programs' OSC 9 / OSC 777 notices; passed on to your
  terminal while its window isn't focused (`notifications.host`).

### Usage in the tab bar

- Segments `agents`, `claude`, `codex`, `command`, `clock`, `host` and `system`, placed by priority in the space the
  tabs leave, in full, short or icon form; stale values dimmed.
- Claude Code's own rate limits through `corgi claude-statusline -- <command>`; Codex's from its logs; percent left
  by default (`show`).
- `command`, `host` and `system` can read on the active workspace's machine (`on = "workspace"`).
- `C-b u` shows every segment's details.

### Copy, paste and the mouse

- Copy mode (`C-b [`) with vi motions, search, character, line and block selection; copies go to the paste buffer
  (`C-b ]`) and your clipboard over OSC 52.
- Mouse selection, double and triple click; programs that ask for the mouse get it (Shift keeps it corgi's).
- Images pasted into panes on other machines (`terminal.put`), or on purpose with `C-b C-v`.
- `C-b e` opens a pane's scrollback in `$EDITOR`.

### Popups and custom commands

- `display-popup` terminals over everything, `run-shell` in the background, and commands in new panes or windows,
  all bindable with `[[keys.bind]]`.

### Worktrees and remote machines

- `C-b W` lists the repository's worktrees to open, make or remove; worktree workspaces group under their
  repository, and workspace names follow their first pane's checkout.
- Other core machines: their state in the sidebar and chooser, `C-b M` to connect, add, rename, log in, update or
  forget one, dimmed panes while a line is down, and git facts read on each machine.

### Migration

- `corgi migrate` writes a corgi config from the herdr TUI's: keys on the herdr preset, theme, sidebar, tab bar,
  panes, toasts and more; what has no counterpart is listed.

### Performance

- No wakeups while idle; redraws only for what changed; bounded program runs; incremental log reading; files
  written off the UI thread.

### Fixed

- `C-b a` (`next-agent -b`) goes forwards again: `-b` is the agents waiting for you; only `-p` goes back.
- The herdr preset bound `W` twice; it renames the workspace, as in herdr.
- "Machine down / back" toasts obey `notifications.toasts = false`.
- `source-file <path>` reads that file (and keeps using it); it used to ignore the path.
- `send-keys -l` types its words as text; it used to be ignored.
- `ui.mouse` applies on reload, not only at the next start.
- A worktree made from inside another worktree is filed under the repository's name.
- A popup whose program exits at once says so; a pane whose program exits before it's placed closes.
- Core heartbeats no longer redraw the screen.

### Development

- Cargo workspace (`hc-proto`, `corgi-vt`, `corgi`), pinned Zig 0.16.0 and Ghostty `1e5cdb8`
  (`scripts/pins.env`, `scripts/bootstrap.sh`), `just` recipes, cargo-deny, and CI on Linux and macOS.
- Release builds on a `v*` tag (Apple Silicon macOS, Linux x86_64 and arm64) attached to a GitHub release, and a
  Homebrew formula for hong-yi/homebrew-tap written from them (pushed once the repository is public).
- Integration tests against a real, isolated core, and end-to-end tests driving corgi on a PTY, including core
  restarts, a killed shell, a core stop and start, and a machine whose line drops.

### Licence

- corgi is MIT-licensed; `crates/corgi-vt/build.rs`, derived from herdr-core, stays Apache-2.0.

[Unreleased]: https://github.com/hong-yi/corgi/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/hong-yi/corgi/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/hong-yi/corgi/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/hong-yi/corgi/releases/tag/v0.1.0
