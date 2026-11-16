# Pane tabs

What each pane's title shows, where an agent's summary comes from, and the shapes it can be drawn in.

Two keys in `[panes]` control it (see [configuration](configuration.md#panes)):

| Key | Default | Meaning |
|---|---|---|
| `title` | `"auto"` | What it says. |
| `title_shape` | `"breadcrumbs"` | How it's drawn. |

## What it says (`panes.title`)

| Value | Shows |
|---|---|
| `"auto"` | An agent's summary while an agent runs in the pane; otherwise the folder. |
| `"cwd"` | The folder. |
| `"agent"` | Only panes with an agent get a title: its summary. |
| `"off"` | No titles. |

A pane you named (`rename-pane`, or "rename pane…" in its menu) is called that, whatever runs in it, unless `title = "off"`. The name is the terminal's `corgi.name` label, so every corgi showing the pane sees it (see [labels](labels.md)).

The folder is the foreground program's working directory, else where the terminal started; your home folder shows as `~`.

### An agent's summary

An agent is a program that reports its status (OSC 7501; see [agent status](agent-status.md)). Its summary is the first of these that is set:

1. The `title` it reports in OSC 7501.
2. The `msg` it reports in OSC 7501.
3. The terminal title it set (OSC 0/2), with a leading spinner or mark stripped (`✳ Fix login` becomes `Fix login`).
4. The terminal title core reports.
5. Its name: the `app` it reports, else the program's name.

A program that never reports status has no summary, so with `"auto"` its pane shows the folder, and with `"agent"` it has no title.

## How it's drawn (`panes.title_shape`)

| Value | Where | Looks like | Font |
|---|---|---|---|
| `"breadcrumbs"` | A row of its own at the top of the pane, with a line under it joined to the pane's border. | `~ › src › corgi` or `corgi › claude › Fixing the bug`, an indicator at the end. | Any. |
| `"line"` | The border line above the pane. | ` corgi ` as text, an indicator at the end. | Any. |
| `"rounded"` | The border line above the pane. | A pill with round ends (U+E0B4/E0B6). | A Nerd Font (powerline glyphs). |
| `"angled"` | The border line above the pane. | A pill with arrow ends (U+E0B0/E0B2). | A Nerd Font. |
| `"slant"` | The border line above the pane. | A pill with slanted ends (U+E0BA/E0BC). | A Nerd Font. |
| `"plain"` | The border line above the pane. | Text on a coloured block. | Any. |

### Breadcrumbs

What the crumbs are:

| The pane | Crumbs |
|---|---|
| Named | The name. |
| Running an agent (not with `title = "cwd"`) | The folder's name, the agent's name, and its summary when it differs from the name. |
| Otherwise | The folder's path: `~ › src › corgi`, or `/ › etc` outside your home. With `title = "agent"`: none. |

The crumbs are separated by ` › `. The last one stands out, lit in the active border colour for the focused pane. When they don't fit, the first ones give way to `…`.

Breadcrumbs take the pane's first two rows (the crumbs and the line under them); a pane 3 or 4 rows tall gives one row, a pane of 2 rows or fewer none.

### Line and pills

The title goes on the border line above the pane. Without the panes' box (`panes.style = "plain"`), panes on the top row give up their first row for it. A title that doesn't fit is cut with `…`.

The focused pane's title is lit: bold in the active border colour for `line`, a solid pill for the pill shapes. Other panes get muted text, or a hollow outlined pill. Clicking a pane's title focuses that pane.

Pills show the agent's status glyph (per `ui.indicators`) before the text.

### The indicator

`breadcrumbs` and `line` end with an indicator of what the pane is doing:

| The pane | Indicator |
|---|---|
| An agent working | A spinner (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`) in the working colour. |
| A command running in the shell, or a program started on its own | The spinner, muted. |
| An agent waiting for you | `●` in the blocked colour. |
| An agent finished, unseen | `●` in the done colour. |
| An agent idle | `●` in the idle colour. |
| An agent in an unknown state | `●` in the unknown colour. |
| A shell at its prompt | Nothing. |

The spinner uses Braille characters, which most terminal fonts have.

## The narrow layout

In the narrow layout (`ui.narrow_width`) panes have no titles.

```toml
[panes]
title = "agent"
title_shape = "rounded"
```
