# Input and keymaps

How a key travels from the host terminal to an action or a pane, how key tables and presets are built, and how to add an action and keep the key hints right.

## Where keys go

```
crossterm KeyEvent (input thread) ─► Msg::Input ─► App::input
   no window yet ─► App::early (replayed by App::sync once there is one)
   else ─► App::key(k)  (app/keyboard.rs), by Mode:
      Prompt / Dialog / Copy / Popup / Menu / Chooser / Keys / Worktrees / Machines ─► that mode's handler
      Prefix      ─► keymap.lookup("prefix", k) ─► App::run(action)
      Repeat      ─► a repeatable prefix binding, else handled as Normal
      Table(name) ─► keymap.lookup(name, k)
      Normal      ─► the prefix key? ─► Mode::Prefix
                     a "root" binding? ─► App::run
                     sidebar focused? ─► App::sidebar_key
                     else ─► input::to_vt ─► Vt::encode_key ─► the focused pane
```

`App::key` takes the mode out with `std::mem::replace(&mut self.mode, Mode::Normal)` and each handler puts back
the mode it wants. So a handler that does nothing returns to Normal; that's how unknown prefix keys are dropped.

Every new UI state is a `Mode` variant (`app/mod.rs`). Route its keys in `app/keyboard.rs::App::key`, draw it in
`app/draw.rs::App::draw`, give it a badge in `App::mode_badge`, with a label in `config::ModeLabels` and
`default-config.toml`.

## Key notation

`keys::Key::parse` reads tmux notation:

- modifiers `C-` and `M-`, in any order and repeated (`C-M-x`);
- names: `Up Down Left Right Enter Tab BTab S-Tab Space Escape Esc BSpace Home End`, `PPage PageUp PgUp`,
  `NPage PageDown PgDn`, `DC Delete`, `IC Insert`, `F1`…;
- any single character (`%`, `"`, `N`).

`Key::matches` ignores Shift for characters (they carry it: `%`, `N`) and compares letters case-folded under Ctrl,
because some terminals report `C-b` as `C-B` with Shift. `Key::event` turns a key back into a `KeyEvent`, for
`send-keys` and `send-prefix`.

## Tables, prefix and repeat

`keys::Keymap` has `tables: BTreeMap<String, Vec<Binding>>`, the `prefix` key and `repeat_time`.

- `prefix`: after the prefix key.
- `root`: checked before the pane gets a key. Empty in every preset; filled from the config.
- `resize`: entered with `switch-client -T resize`; `h j k l` / arrows by 1, `H J K L` by 5.
- any other name a `[[keys.bind]]` entry uses, entered the same way.

Timing (`app/keyboard.rs`):

- The prefix has no timeout: `Mode::Prefix(since)` waits for the next key. `since` is only for the key-hints
  delay. Esc cancels; the prefix pressed twice runs `send-prefix` (added by `Keymap::build`).
- A binding with `repeat` (tmux `bind -r`) in the prefix table enters `Mode::Repeat(now + repeat_time)`; default
  500 ms (`keys.repeat_time`). Within it, repeatable prefix keys run without the prefix and extend the window; any
  other key is handled as a normal key. The loop wakes when the window ends so the PREFIX badge goes.
- In `Mode::Table(name)`, a repeatable binding keeps the table; another runs once and returns to Normal. Esc or
  Enter leaves; the prefix key switches to prefix mode.

Copy-mode keys are built into `copy.rs` (vi style); they aren't a key table yet.

## Building the keymap

`Keymap::build(&config::Keys)`:

1. The preset's rows: `tmux_rows()`, `herdr_rows()` or `none_rows()`, each followed by `resize_rows()` (except
   `none`). A row is `(table, key, command, repeat, hint)`.
2. `send-prefix` on the prefix key.
3. `keys.unbind`: `"%"` (prefix table) or `"root M-h"`.
4. `keys.bind`: each `[[keys.bind]] key/command/table/repeat/note/group`.

`Keymap::add` parses every command with `commands::parse`, so a bad binding fails the build with a message naming
it. A later binding for the same key replaces the earlier one. The config is parsed and the keymap built before
corgi takes the screen; `C-b R` (`source-file`, `App::reload_config`) rebuilds it and keeps the old one on error.

## Commands and actions

`commands.rs` is the one command language. `commands::words` splits a line like a shell (quotes, backslashes);
`commands::parse` maps the first word (or its tmux alias: `splitw`, `neww`, …) to an `Action`, with options parsed
by `Args::parse(cmd, words, takes_value, flags)`. Unknown commands suggest the nearest name in `NAMES` by edit
distance. `commands::complete` gives the `:` prompt's Tab completion from `NAMES`.

`App::run(Action)` (`app/actions.rs`) carries an action out. Key bindings, the `:` prompt
(`App::run_command`), menu entries, list-keys' Enter and `confirm-before` all end up here.

### Adding an action

1. Add a variant to `commands::Action`, with a doc comment.
2. Parse it in `commands::parse` (name, aliases, options), and add the name to `NAMES`.
3. Add a test in `commands.rs`'s `tests`: the parse, its options, and its mistakes.
4. Handle it in `App::run`. Put any decision in a pure module and keep this arm thin.
5. Bind it in the presets in `keys.rs` (`tmux_rows`, and `herdr_rows` if herdr had it) with a hint, or `None` when
   another entry's hint covers it (as `h j k l` covers four bindings).
6. If it acts on a pane, window, workspace or machine, add it to the matching section in `app/menus.rs`, keyed like
   its prefix key. `j` and `k` are taken (`menu::MOVES`).
7. The `:` prompt and `C-b ?` need nothing more: they parse and list commands.

## The key-hints dialog

The hints come from the same keymap that runs the keys, so they can't disagree.

`Keymap::hints(table)` builds `Vec<keyhints::Group>`:

- Each binding's `hint` is `(group, key as shown, label)`. Bindings are grouped by group title, in the order the
  preset first uses each group (tmux: Windows, Panes, Workspaces, Agents, Other).
- Bindings from the config get group `group`, else `"Custom"`, and label `note`, else the command itself. The
  `Custom` group moves to the end.
- `Esc` ("cancel" in the prefix table, "leave" elsewhere) is appended to `Other`.
- `keys.unbind` removes a binding and its hint with it.

`App::draw` shows the widget in `Mode::Prefix` when `keys.hints` says so (`always`, `delay` after `hint_delay`
(400 ms by default), or `off`), and in `Mode::Table` unless hints are off. Layout and drawing are in
[ui-components.md](ui-components.md#the-key-hints-widget).

Keep hints in sync when you change bindings:

- Give every new prefix binding a hint, or make sure another hint names it.
- Keep labels short; the widest entry sets the column width.
- `keyhints.rs::one_tall_group_still_gets_columns_beside_it` builds the default prefix table and checks every group
  shows, in at least two columns, at 100×30, 160×45 and 200×50. `keys.rs::tmux_preset` checks the group titles.
  Run them after touching the presets.

## Host-side details

- `term::Guard::enter` enables raw mode, the alternate screen, bracketed paste, focus events, mouse capture (per
  `ui.mouse`), and the kitty keyboard flags when the host supports them.
- Key releases are ignored (`App::input`).
- Pastes go to the prompt, the popup, or the pane (`App::paste_into_pane`, bracketed by `Vt::encode_paste`).
- Esc followed by more bytes in one read is Alt+key to crossterm. The e2e tests pause after Esc for that reason.
