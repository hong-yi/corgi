# Copy mode

Scroll back through a pane, search it, select text and copy it, with vi keys; paste it back, or
open the whole scrollback in your editor.

Keys below are for the default `tmux` preset with the prefix `C-b`.

## Entering and leaving

| What | tmux preset | herdr preset | Command |
|---|---|---|---|
| Copy mode in the focused pane | `C-b [` | `C-b [` | `copy-mode` |
| Copy mode, a screen up at once | `C-b PageUp` | none | `copy-mode -u` |
| Paste what you copied last | `C-b ]` | `C-b ]` | `paste-buffer` |
| The scrollback in your editor | `C-b e` | `C-b e` | `edit-scrollback` |

In copy mode the badge says COPY, and `[row/rows]` at the pane's top right says where the cursor
is. The cursor starts where the pane's own cursor is. The pane keeps its colours, and new output
keeps arriving: corgi reads the rows again when they change and keeps your cursor where it was.

To leave:

| Key | Does |
|---|---|
| `q`, `C-c` | Leave. |
| `Esc` | Clear the selection; with none, leave. |
| `Enter` | Copy the selection and leave; with none, just leave. |
| `y` | Copy the selection and leave; with none, copy the cursor's line. |
| The prefix (`C-b`) | Leave copy mode and start a prefix key. |

Leaving puts the pane back at the bottom.

Dragging with the mouse in a pane also starts copy mode (see [selecting with the mouse](#selecting-with-the-mouse)).

## Moving

| Key | Moves to |
|---|---|
| `h` `j` `k` `l`, arrows | Left, down, up, right. |
| `w` / `b` / `e` | Next word's start / this or the previous word's start / this or the next word's end. A word is letters, digits and `_`; other punctuation forms words of its own. |
| `W` / `B` / `E` | The same for WORDs: anything that isn't a space. |
| `0`, `Home` | The start of the line. |
| `^` | The first character that isn't a space. |
| `$`, `End` | The end of the line. |
| `f` `x` / `F` `x` | The next / previous `x` on this line. |
| `t` `x` / `T` `x` | Just before the next / just after the previous `x` on this line. |
| `;` / `,` | The last `f F t T` again, the same way / the other way. |
| `}` / `{` | The blank line after / before this paragraph (or the last / first line). |
| `gg` / `G` | The first / last line. |
| `H` / `M` / `L` | The top / middle / bottom of the screen. |
| `C-d` / `C-u` | Half a screen down / up. |
| `PageDown` / `PageUp`, `C-f` / `C-b` | A screen down / up. |

A count before a motion repeats it: `5j`, `3w`, `2f,`. `0` starts a count only after another
digit (`10j`); on its own it goes to the start of the line.

With the default prefix `C-b`, pressing `C-b` in copy mode leaves it and starts a prefix key, so
`C-b` never scrolls up a screen. Use `PageUp` instead, or pick another prefix.

The mouse wheel scrolls the view three lines at a time.

## Searching

| Key | Does |
|---|---|
| `/` | Search down. Type the text, then `Enter`. |
| `?` | Search up. |
| `n` / `N` | The last search again, the same way / the other way. |

While you type a search, it shows at the bottom of the pane. `Backspace` deletes, `C-u` clears,
`Esc` cancels.

Search is smart about case: all lowercase matches any case; any capital letter makes it match
case exactly. It wraps round the ends and says "from the top" or "from the bottom" when it does.
If nothing matches, it says "not found".

## Selecting and copying

| Key | Does |
|---|---|
| `v` | Start (or end) a selection of characters. |
| `V` | Start (or end) a selection of whole lines. |
| `C-v` | Start (or end) a block: the same columns on each line. |
| `o` | Move the cursor to the selection's other end. |
| `y`, `Enter` | Copy the selection and leave. |
| `Esc` | Clear the selection. |

Pressing the same key again ends the selection; pressing another of `v V C-v` changes its shape
and keeps its start. A block runs from the column it started in to the column the cursor keeps
to; after `$` it reaches every line's end. Spaces at the end of each line of a block are dropped.

A copy goes to two places, and the status line says how much ("copied 3 lines"):

- corgi's paste buffer, for `C-b ]`. It holds the last copy only, and it lasts while this corgi
  runs.
- Your terminal's clipboard, through OSC 52. Your terminal has to support and allow OSC 52; some
  need a setting for it. Inside tmux, corgi wraps it for tmux's passthrough, which needs
  `set -g allow-passthrough on`.

## Pasting

`C-b ]` pastes the last copy into the focused pane as a paste: bracketed when the program asked
for bracketed paste, so a shell or editor doesn't run it line by line. With nothing copied yet,
the status line says so.

## Selecting with the mouse

In a pane whose program doesn't use the mouse:

- Press and drag to select. Copy mode starts, and letting go copies the selection. Dragging past
  the top or bottom of the pane scrolls.
- Double-click to copy a word; triple-click to copy a line.

A click in a pane that is already in copy mode moves the cursor there. See [mouse](mouse.md) for
programs that use the mouse themselves.

## Editing the scrollback in your editor

`C-b e` (`edit-scrollback`) writes the focused pane's whole scrollback to a temporary file and
opens it in `$VISUAL`, else `$EDITOR`, else `vi`. corgi starts the editor as `editor +N file`,
so editors that read `+N` (vi, vim, nvim, nano, emacs) open at the last line. The editor runs in a popup
over corgi, 90% of the screen, titled "scrollback of" and the pane's name. Search, copy and save
parts of it as you would any file.

The popup closes when the editor exits, and the temporary file is deleted then. Changes you make
in the editor don't go back to the pane. The editor runs on this machine, even for a pane on
another machine.

## Not built yet

- Emacs keys in copy mode. There is only the vi table.
- Changing copy-mode keys: `[[keys.bind]]` with `table = "copy-mode"` has no effect yet.

## Related

- [Mouse](mouse.md)
- [Custom commands](custom-commands.md): popups and `send-keys`
- [Key bindings](keybindings.md)
