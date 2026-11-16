# Terminal emulation

How `corgi-vt` turns a pane's bytes into cells, encodes keys and the mouse for the program, and handles replies, titles, bells and scrollback.

## The boundary

Core relays each terminal's raw output. To draw several panes on one screen corgi keeps one emulator per pane:
`corgi_vt::Vt` (`crates/corgi-vt/src/lib.rs`), a safe wrapper over libghostty-vt, the parser core itself uses.
Why libghostty-vt: [ADR 0001](adr/0001-vt-emulator.md). How it's built: [building.md](building.md).

The crate is the only place with FFI to Ghostty. The generated bindings live in the private `sys` module
(`include!` of `$OUT_DIR/sys.rs`). Everything else in corgi sees plain Rust types. Keeping the boundary this narrow
is what lets the backend be swapped if the Zig build ever becomes a problem.

## API

| Call | What it does |
|---|---|
| `Vt::new(cols, rows)` | A fresh terminal, render state, key and mouse encoders, and the callbacks. |
| `write(&[u8])` | Feed program output exactly as relayed. Bumps `revision()`. |
| `frame() -> &Frame` | Capture the screen: `cols × rows` `Cell`s, the `Cursor` (if visible), default `fg`/`bg`. |
| `resize(cols, rows)` | Resize; queues a mode-2048 size report in the replies if the program asked for them. |
| `take_replies()` | Bytes the emulator wants to send the program (query answers, size reports). |
| `take_title()` | The window title, if it changed since the last call. |
| `take_bell()` | Whether a BEL arrived. |
| `take_notifications()` | OSC 9 / OSC 777 desktop notifications, oldest first (at most 16 kept). |
| `mode(n, ansi)` | Read a terminal mode (`?2004`, `?1049`, …). |
| `encode_key(&Key)` | Bytes for a key press, for the program's current modes. |
| `encode_paste(&str)` | A paste: bracketed when mode 2004 is on, unsafe control bytes stripped. |
| `mouse_tracking()`, `alternate_screen()`, `encode_mouse(…)` | Mouse reports (`mouse.rs`). |
| `scroll(delta)`, `scroll_to(row)`, `scroll_bottom()`, `scrollbar()` | Move the viewport through history. |
| `screen_lines()` | Every row's text, history first, one line per row. |
| `hold_deadline()` | When a synchronized-output hold runs out. |
| `revision()` | Rises with every write and resize: text read at one revision is still good while it stays. |
| `set_cell_pixels(w, h)` | The host's cell size, for pixel size reports. corgi doesn't call it yet. |

Each pane owns one `Vt` (`crates/corgi/src/pane.rs::Pane`). Every attach gets a fresh one (`Pane::attached`), at
the size it asked core for, because core's first paint (starting with CAN) redraws the whole screen and history.

## How the callbacks collect state

libghostty calls back during `ghostty_terminal_vt_write`. The callbacks (`on_write`, `on_bell`, `on_title`,
`on_notify`, `on_size`, `on_hold`) write into a boxed `Sink` whose address is the terminal's userdata. The `take_*`
methods drain it. Nothing calls back outside `write` and `resize`, so `&mut self` covers all of it.

## Cells to the ratatui buffer

```
bytes ─► Vt::write ─► libghostty terminal ─► render state ─► Vt::frame (dirty rows only)
                                                                   │
                     ui::draw_frame(buf, rect, frame) ◄────────────┘
                     draw.rs::extend_into_inset
```

`Vt::frame` updates the render state and re-reads only rows libghostty marks dirty, unless the whole screen is
dirty or the size changed. `read_cell` reads each cell's grapheme (UTF-8), its wide flag, its style when it has
one, and its background always: cells erased while a background colour was set (how vim paints its background)
carry only a colour.

`ui::draw_frame` (`crates/corgi/src/ui.rs`) copies a `Frame` into the buffer, clipped to the pane's rectangle:

- `None` colours become `Color::Reset`, so the host terminal's own default shows through. Themes never recolour
  panes.
- Bold, italic, faint, underline, inverse, strike and invisible map to ratatui modifiers.
- `Wide::Tail` cells are reset so ratatui's diff skips them, as its own wide-text widgets do. A wide glyph that
  would spill out of the last column becomes a space.
- The cursor position is returned when visible inside the area; `App::draw` shows it only for the focused, live
  pane and only when no overlay places its own cursor.

`extend_into_inset` (`app/draw.rs`) fills the blank inset around a terminal (`panes.inset`, `panes.inset_rows`)
with the background of the nearest cell, so a program that paints its background fills the pane to its border.

## Synchronized output

When a program sets mode 2026, `on_hold` captures the finished frame and records when the hold began. `frame()`
keeps showing that capture until the hold ends or runs past `HOLD_LIMIT` (1 s), when `release_hold` turns the mode
off. Scrolling sets `live`, which forces a fresh capture. The app loop wakes at the earliest `hold_deadline()` so a
held pane is drawn when its time runs out.

## Keys

The host key path: crossterm `KeyEvent` → `input::to_vt` (`crates/corgi/src/input.rs`) → `corgi_vt::Key`
(`Char(char, Mods)` or `Named(Named, Mods)`) → `Vt::encode_key` → `AttachTx::input`.

`keys.rs::encode` sets libghostty's key encoder from the terminal (`setopt_from_terminal`), so DECCKM, the keypad
modes and kitty keyboard flags the program set all apply. Alt sends an ESC prefix, and Option counts as Alt
(the host terminal already turned it into Alt or text). Physical keys are inferred from a US layout (`physical`),
which matters only to kitty-protocol programs on other layouts. Tests: `text_control_alt_and_named_keys`,
`follows_the_programs_modes`.

corgi pushes kitty keyboard flags (disambiguate, report alternates) on the host when it supports them
(`term::Guard::enter`), so corgi can tell Esc, Alt and `C-i` from Tab.

## Mouse

`mouse.rs` reports what the program asked for:

- `mouse_tracking()`: DEC modes 9, 1000, 1002 or 1003 are on.
- `alternate_screen()`: modes 1049, 1047 or 47.
- `encode_mouse(action, button, (col, row), mods, pressed)`: libghostty's encoder in the program's format (X10,
  UTF-8, SGR, urxvt, SGR pixels). Without a cell size it assumes 1×1-pixel cells, so pixel formats carry cell
  positions.

`app/mouse.rs::mouse_to_program` decides when the program gets the mouse: when it asked and Shift isn't held. A
pressed button keeps its drag and release in the pane it was pressed in (`App::grab`). A full-screen program that
didn't ask gets the wheel as three arrow keys. Otherwise the wheel scrolls corgi's view and a drag selects.

## Replies, title and bell

- Replies: `Pane::output` writes the bytes, then `flush_replies` sends `take_replies()` as AUTOMATIC_INPUT. Resizes
  flush too (mode-2048 reports). See [protocol.md](protocol.md#attach-lifecycle).
- Title: `take_title()` updates `Pane::title`, which names the pane (`Pane::name`) and feeds the agent summary in
  its tab (`pane_title::agent_summary`).
- Bell: after each draw the loop checks every pane's `take_bell()` and rings the host once (`term::bell`).
- Notifications: `Msg::Stream` handling calls `take_notifications()` and passes them to
  `App::pane_notifications` (toasts, and the host terminal per `notifications.host`). Only attached panes, the
  visible ones, are heard.

## Scrollback and search

The attach asks core for 2000 rows of history (`Pane::attach`), so the first paint fills the emulator's scrollback.

- The wheel calls `scroll(±3)`; any input calls `scroll_bottom()` (`Pane::input`).
- Copy mode (`crate::copy`, `app/copymode.rs`) works on `screen_lines()`: plain text from libghostty's formatter,
  history first, one line per row (soft wraps kept apart) so line `i` is row `i` in `scroll_to`'s row space. It
  re-reads only when `revision()` changed. It shows the pane from its own row with `scroll_to(top)`, so the pane
  keeps its colours, and draws the cursor and selection over it.
- Search (`/`, `?`, `n`, `N`, smart case) is in `copy.rs`, on those lines. `corgi-vt` has no search of its own.
- `edit-scrollback` (`C-b e`, `app/popup.rs`) writes `screen_lines()` to a temp file for `$EDITOR`.

## Debugging a pane

- `CORGI_RECORD=<dir>` makes `pane.rs::record` append each pane's received bytes to
  `<dir>/<addr>-<attach>.bin`.
- `cargo run -p corgi-vt --example dump -- <file> <cols> <rows>` replays such a file and prints the screen.
- `just bench` (`--example bench`) prints parse throughput and frame-capture time.
