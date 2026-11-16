# Mouse

What clicks, the wheel and drags do in corgi, how programs that use the mouse get it, and how to
turn mouse capture off.

corgi is keyboard first: everything here has a key too. The mouse is a shortcut.

## Clicks

| Where | Left click | Right click |
|---|---|---|
| A pane, or its title above it | Focus that pane. | The pane's action menu (Pane, Window, Workspace). |
| A window tab | Select that window. | The window's action menu. |
| The `+` after the tabs | A new window, as `C-b c`. | |
| Usage segments in the tab bar | The usage details, as `C-b u`. | |
| A sidebar row | Go to that workspace, window or pane. | That row's action menu. |
| `▾` / `▸` in the sidebar | Fold or unfold the row. | |
| An agent in the sidebar | Go to its pane. | |
| A toast | Go to the pane it is about. | |
| "switch" in the narrow layout's header | The tree chooser, as `C-b w`. | |

See [navigation](navigation.md#action-menus) for what the menus hold. Over a pane whose program
uses the mouse, clicks go to the program; hold `Shift` to get the table above (see
[below](#programs-that-use-the-mouse)).

While a dialog is open, a click on one of its fields focuses that field, and other clicks are
ignored. While a menu is open, any click closes it; pick an entry with its key or `Enter`. While a
popup is open, left clicks and the wheel do nothing, and the program in the popup doesn't get the
mouse.

## The wheel

Over a pane, the wheel depends on what runs there:

| The program | The wheel |
|---|---|
| Asked for mouse reports (vim with `mouse=a`, lazygit, htop) | Goes to the program. |
| Is full screen but didn't ask (less, man) | Sent as `↑` / `↓` keys, three per notch. |
| Anything else (a shell) | Scrolls the pane's history, three lines per notch. Any key you type jumps back to the bottom. |

In copy mode the wheel moves copy mode's view instead (see [copy mode](copy-mode.md)).

## Selecting and copying

In a pane whose program doesn't use the mouse:

| Do | Gets |
|---|---|
| Press and drag | A selection: copy mode starts. Letting go copies it. Dragging past the pane's top or bottom scrolls. |
| Double-click | The word under the pointer, copied. |
| Triple-click | The line, copied. |

Clicks count as double or triple when they land on the same cell within 400 ms. A copy goes to
corgi's paste buffer (`C-b ]` pastes it) and to your terminal's clipboard over OSC 52. In copy
mode, a click moves the cursor there. See [copy mode](copy-mode.md) for the details.

## Programs that use the mouse

When a pane's program asks for mouse reports, corgi passes it the mouse: clicks, drags, motion
and the wheel, in the program's own coordinates and in the format it asked for. Ctrl and Alt held
with a click are passed on too. Pressing a button in a pane also focuses that pane.

A button pressed in a pane keeps its drag and release in that pane, even if the pointer leaves
it; positions outside it are given as the pane's nearest edge.

Hold `Shift` to keep the mouse for corgi: Shift-drag selects and copies, Shift-click focuses,
Shift-right-click opens the action menu, as most terminals do with Shift.

corgi passes the mouse on only in normal mode, with the keyboard in the panes. While the sidebar
has the keyboard, or copy mode, a dialog or a menu is open, corgi keeps the mouse.

## Resizing the sidebar

Drag the sidebar's right edge to make it wider or narrower. The width is saved for this client
when you let go. The keyboard way is in [navigation](navigation.md#resizing-the-sidebar).

## Turning mouse capture off

```toml
[ui]
mouse = false
```

corgi then leaves the mouse to your terminal: its own selection and scrolling work as they would
without corgi, and none of the above happens. `C-b R` applies it at once, either way.

## Not built yet

- Dragging the dividers between panes to resize them. Use `C-b C-←` and the other resize keys
  ([panes and layouts](panes-and-layouts.md#resizing)).
- Dragging tabs or workspaces to reorder them.
- Dragging the line between the sidebar's workspaces and agents.
- Ctrl+click to open links and core addresses (`t7`, `vps/t7`).

## Related

- [Copy mode](copy-mode.md)
- [Navigation](navigation.md)
- [Configuration](configuration.md): `ui.mouse`
