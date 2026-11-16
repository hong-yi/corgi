# UI components

The pieces corgi draws around the panes (tab bar, sidebar, overlays, dialogs, the key-hints widget), the theme tokens they use, and how to write a new dialog.

All of it is drawn by `App::draw` (`crates/corgi/src/app/draw.rs`) each frame, from state. The widgets live in
pure modules that render into a ratatui `Buffer` and return click targets; the app stores those in `App::hits`.

## Screen areas

`App::areas` returns `Areas { sidebar, bar, bar_row, body, panes }`:

```
span = "window" (default)              span = "panes"
┌──────────┬──────────────────┐        ┌──────────┬──────────────────┐
│ sidebar  │  body (panes)    │        │ sidebar  │  body (panes)    │
│          │                  │        │          ├──────────────────┤
├──────────┴──────────────────┤        │          │  bar             │
│ bar (tab bar, bottom)       │        └──────────┴──────────────────┘
└─────────────────────────────┘
```

`tab_bar.position` puts the bar on top or bottom, `tab_bar.padding` adds rows between it and the panes,
`sidebar.padding` adds columns between the sidebar and the panes. The narrow layout (`ui.narrow_width`) replaces the
bar with a two-row header (`app/narrow.rs::draw_header`) and drops the sidebar.

## Tab bar

`App::draw_status_line` (`app/draw.rs`), left to right:

- the mode badge (`App::mode_badge`: label from `[mode]`, colour from `mode_*` theme tokens; `""` hides it);
- window tabs with `*` active, `-` last, `Z` zoomed and a status dot, scrolled with `‹ ›` to keep the active tab
  visible (`sidebar::visible_tabs`);
- the `+` button;
- usage segments, right-aligned in what the tabs leave (`App::usage_placed`, see
  [usage-providers.md](usage-providers.md));
- the status message (`App::note`), cut with `…` when short of room.

Tabs come first: messages and usage get only the space that's left. Click targets go in `hits.tabs` as `TabHit`.

## Sidebar

- `tree.rs` builds rows (`tree::rows(&Inputs, &Fold, filter)`): machines › workspaces › windows › panes, status
  rolled up at each level, worktrees grouped under their repository. `App::tree_rows` and `App::tree_agents`
  (`app/actions.rs`) fill `tree::Inputs` from app state.
- `sidebar.rs::View::render` draws the tree on top and agents below (`sidebar.agents = "split"`) or straight after
  it (`"below"`), from row templates (`sidebar::tree_line`). It returns `(Rect, Hit)` pairs for clicks.
- `chooser.rs::Chooser` is the same rows in a dialog (`C-b w`, `C-b s`), and the keyboard cursor when the sidebar
  has focus (`App::sidebar_focus`). Folding is one `tree::Fold`, shared by both and saved in the view file.

## Pane chrome

- Borders: each pane in its own box when `panes.separate` (lit with `border_active` for the pane your keys go to),
  or shared dividers with junctions (`draw_separators`, `draw_box`).
- Titles: `App::draw_pane_titles` with `pane_title.rs`: a tab on the border (`Tab::render`, shapes `rounded`,
  `angled`, `slant`, `plain`), a line (`render_line`) or breadcrumbs on their own row (`render_crumbs`). The
  activity circle or spinner comes from `pane_title::circle` and `status::activity`.
- Messages over a pane: "attaching…", "exited", "waiting for its new shell", "reconnecting…" (`pane_message`), a
  greyed last screen while its machine is down, and a warning bar when the pane runs corgi itself.

## Overlays

Drawn after the panes, in this order: toasts, the popup, then the overlay for the current `Mode`.

| Overlay | Module | Mode |
|---|---|---|
| Toasts | `toast.rs::Toasts` (push, render, expire, dismiss_pane) | none: always over the panes |
| Popup terminal | `app/popup.rs::draw_popup` | `Mode::Popup` has the keys |
| Key hints | `keyhints.rs::KeyHints` | `Mode::Prefix`, `Mode::Table` |
| All keys (`C-b ?`) | `keylist.rs::KeyList` | `Mode::Keys` |
| Tree chooser | `chooser.rs::Chooser` | `Mode::Chooser` |
| Action menu | `menu.rs::Menu` | `Mode::Menu` |
| Worktrees, Machines | `worktrees.rs::View`, `machines.rs::View` | `Mode::Worktrees`, `Mode::Machines` |
| Forms and confirms | `form.rs::Form` | `Mode::Dialog` |
| Copy mode | `app/copymode.rs::draw_copy` | `Mode::Copy` |
| Pane numbers | inline in `App::draw` | `Mode::Panes` |

## The dialog rule

Every dialog is drawn over the whole window and placed with `ui::centered(area, w, h)`, passing `self.screen` as
the area. Never centre on the pane area or the body: a dialog must open in the same place whatever the sidebar, tab
bar and panes are doing. `ui::centered` shrinks the box to fit a smaller area; `ui.rs::centered_boxes` tests it.
The key hints across the bottom are the one exception: they span the window's width above a bottom tab bar.

## The form framework

`form.rs` is pure apart from listing folders for completion.

- `Field` constructors: `Field::text`, `Field::path` (Tab completes folders on this machine when `local`),
  `Field::select` (`(value, label)` options; ← → or Space), `Field::toggle`. A field has a `key`, a `label`, a
  `placeholder`, an optional `note` under it, and `hidden` (not shown, skipped by Tab).
- `Form::new(title, action, fields)`; `Form::confirm(title, action, message)` for a question with no fields
  (Enter or `y` goes ahead, Esc or `n` doesn't). `Form::message` holds lines above the fields; `true` marks a
  warning.
- `Form::key` returns `Outcome::Stay | Cancel | Submit`. Tab / S-Tab (or ↓ ↑) move, text edits emacs style.
- `Form::show(key, shown)` hides or reveals a field; `Form::value(key)` reads one; `Form::error` shows a validation
  message.
- `Form::render(area, buf, theme)` dims everything behind it, centres the box with `ui::centered`, and returns the
  cursor position. `Form::click` focuses the field under the mouse (`Form::hits`).

## Writing a new dialog

A dialog is `Mode::Dialog(Dialog { does: Does, form: Form, busy })` (`app/dialogs.rs`).

1. Add a `Does` variant describing what submitting does, and its badge in `Does::badge` (`New`, `Rename`,
   `Confirm` or `Info`).
2. Build the form and set the mode: `self.mode = Mode::Dialog(Dialog { does, form, busy: false })`. If a command
   opens it, add a `commands::DialogKind` and handle it in `App::open_dialog`.
3. Handle the variant in `App::submit`. Return `Ok(true)` when done, `Ok(false)` to keep the dialog open while
   something it started finishes (set `busy`), or `Err(message)` to show a validation error.
4. Fields that depend on others (shown or hidden, defaults that change) are updated after each key in
   `App::dialog_key`.
5. Lists from core (machines, templates) arrive as `Msg::Lists` and are applied in `App::lists_arrived`.
6. Nothing more for drawing or the mouse: `App::draw` renders any `Mode::Dialog` over `self.screen`, and clicks go
   to `App::dialog_click`.

A dialog that isn't a form (a list with its own keys) gets its own `Mode` variant instead: route keys in
`app/keyboard.rs`, draw it in `App::draw` over `self.screen` with `ui::centered`, add a badge in `App::mode_badge`
and a label in `ModeLabels` and `default-config.toml`.

## Theme tokens

`theme.rs::Theme` colours corgi's chrome only; panes always keep the program's colours over the host's own
background. Tokens:

| Token | Used for |
|---|---|
| `fg`, `muted`, `surface` | text, dimmed text, the tab bar's and sidebar's background |
| `accent`, `accent_fg` | the active tab, selections, text on accent |
| `badge`, `badge_fg` | badges |
| `prefix` | key-hint keys and the prefix colour |
| `border`, `border_active` | borders, and the focused pane's |
| `working`, `blocked`, `done`, `idle`, `unknown` | agent states |
| `mode_normal`, `mode_prefix`, `mode_sidebar`, `mode_dialog` | the mode badge |
| `rounded` | corners from `ui.corners` (`border_type()`, `corners()`) |

`Theme::resolve(&ThemeConfig, host_bg)` picks a built-in (`theme::NAMES`: 7 themes); `auto` chooses
`light`/`dark` from the host background the display query found; `[theme.custom]` overrides any token by name.
`theme.rs` has `parse_color` for `#rrggbb`, `#rgb` and names. Use tokens; don't hard-code colours in widgets (the
usage blocks' provider defaults in `App::segment_style` are the exception, e.g. Claude's coral).

## The key-hints widget

`keyhints.rs` draws `Vec<Group>` from `Keymap::hints` (see [input-and-keymaps.md](input-and-keymaps.md)).

`keyhints::plan(groups, max_w, max_h) -> Plan { col_w, columns, body_h, hidden }`:

- Column width is the widest entry (key plus label) or group title.
- Groups are kept whole. With 8 or fewer groups, `balanced` tries every way of putting them, in order, into up to
  the columns that fit, and keeps the shortest; ties go to fewer columns. Otherwise, or when that doesn't fit,
  `pack` fills columns in order, growing the target height until every group fits.
- When nothing fits, it fills what does and counts the rest in `hidden`.

`KeyHints::render` then sizes each column to its own entries (keys right-aligned to that column's widest key),
draws the box with the prefix in the title, and a footer: `Esc cancel · ? all keys`, plus `· N more groups` when
some are hidden. With `bottom` (`keys.hints_position = "bottom"`, the default) it spans the area's width with the
columns centred; otherwise it's centred with `ui::centered`, up to 160 columns wide. Below 28×8 it falls back to a
one-line strip at the bottom (`KeyHints::strip`).

### Render tests

There are no snapshot files (no `insta`). Widget tests render into `Buffer::empty(rect)`, turn rows into strings,
and assert on them: `keyhints.rs` (`dialog_shows_title_keys_and_footer`, `at_the_bottom_it_spans_the_width`,
`dialog_is_centred_both_ways`, `tiny_terminal_falls_back_to_a_strip`), `ui.rs`, `draw.rs` (borders and
junctions), `toast.rs`, `sidebar.rs` and others. Do the same for a new widget: render at a few sizes and check
placement and the text that matters.
