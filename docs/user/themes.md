# Themes

Pick the colours of corgi's chrome: the tab bar, sidebar, borders, dialogs, toasts and status indicators.

Themes colour corgi's own chrome only. Your panes always draw with the program's colours over your terminal's own background, and no theme repaints them.

## Pick a theme

```toml
[theme]
name = "auto"
light = "catppuccin-latte"
dark = "catppuccin-mocha"
```

`name` is one of the built-in themes, or `auto`.

| Theme | Kind |
|---|---|
| `catppuccin-mocha` | Dark (the default dark) |
| `catppuccin-latte` | Light (the default light) |
| `tokyo-night` | Dark |
| `gruvbox-dark` | Dark |
| `nord` | Dark |
| `dracula` | Dark |
| `terminal` | Your terminal's own palette: ANSI colours and its default background |

`terminal` follows whatever colour scheme your terminal uses, and works in terminals without true colour.

An unknown name is an error that lists the built-in ones. Reload with `C-b R` to try a theme; a broken config keeps the current one.

### auto

With `name = "auto"` (the default), corgi asks your terminal for its background colour when it starts. A light background (relative luminance above one half) picks the `light` theme; a dark one picks `dark`. If the terminal doesn't answer, corgi picks `dark`.

Set `light` and `dark` to any built-in themes:

```toml
[theme]
name = "auto"
light = "catppuccin-latte"
dark = "tokyo-night"
```

## Change single colours

`[theme.custom]` overrides colours of whichever theme is in use:

```toml
[theme]
name = "nord"

[theme.custom]
accent = "#88c0d0"
blocked = "bright-red"
surface = "reset"
```

A value is `#rrggbb`, `#rgb`, or a colour name: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white` (or `gray`), `bright-black` (or `dark-gray`), `bright-red`, `bright-green`, `bright-yellow`, `bright-blue`, `bright-magenta`, `bright-cyan`, `bright-white`, and `reset` (or `default`): the terminal's own colour. `_` works in place of `-`.

| Name | What it colours |
|---|---|
| `fg` | Text |
| `muted` | Secondary text: hints, dimmed rows, inactive tabs |
| `surface` | The background of the tab bar, sidebar, dialogs and toasts |
| `accent` | The active window tab and selections; the `agents` usage segment |
| `accent_fg` | Text on the accent colour |
| `badge` | The `codex` usage segment |
| `badge_fg` | Text on the badge colour |
| `prefix` | Key names in the key hints; dialog borders and titles |
| `border` | Borders; the `clock`, `host` and `system` usage segments |
| `border_active` | The focused pane's edges; the `command` usage segment |
| `working` | An agent at work |
| `blocked` | An agent waiting for you |
| `done` | An agent that finished and you haven't looked yet |
| `idle` | An idle agent |
| `unknown` | An agent whose state isn't known |
| `mode_normal` | The mode badge in NORMAL |
| `mode_prefix` | The mode badge after the prefix (PREFIX) |
| `mode_sidebar` | The mode badge while the sidebar has the keyboard (SIDEBAR) |
| `mode_dialog` | The mode badge in dialogs, prompts, menus and choosers |

An unknown name, or a value that isn't a colour, is an error naming the key.

Usage segments can also set their own `bg` and `fg`; see [usage-status.md](usage-status.md).

## Status indicators

How agent states look in the sidebar and on the window tabs:

```toml
[ui]
indicators = "dots"
```

| State | `"dots"` (default) | `"symbols"` |
|---|---|---|
| Waiting for you (blocked) | `●` | `×` |
| Done, unseen | `●` | `✓` |
| Working | `●` | `◐` |
| Idle | `○` | `○` |
| Unknown | `·` | `·` |

With dots, colour tells the states apart (`blocked`, `done` and `working` above). Symbols also work without colour.

## Corners

```toml
[ui]
corners = "sharp"
```

| Value | Looks like |
|---|---|
| `"sharp"` (default) | `┌ ┐ └ ┘`: edges meet cleanly where boxes sit side by side |
| `"rounded"` | `╭ ╮ ╰ ╯` |

This applies to every box corgi draws: the sidebar, panes, the tab bar, dialogs and toasts.

## Not built yet

The old herdr TUI had 18 built-in themes; corgi has the 7 above. If `corgi migrate` meets a herdr theme corgi doesn't have, it says so and leaves the default.

## Related

- [configuration.md](configuration.md): `[theme]` and `[ui]`
- [agent-status.md](agent-status.md): what the states mean
- [migrating-from-herdr.md](migrating-from-herdr.md): herdr's theme names
