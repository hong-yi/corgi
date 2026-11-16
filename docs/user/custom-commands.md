# Custom commands

Run any corgi command from the `:` prompt, bind commands to keys, and use popups, shell commands,
`send-keys`, confirmations and key tables of your own.

Every key binding in corgi runs a command in a tmux-style language. The same commands work at the
`C-b :` prompt, in `[[keys.bind]]`, and in the action menus. [Commands](commands.md) lists them
all with their flags; this page shows how to put them to use.

## The command prompt

`C-b :` (`command-prompt`) opens a prompt in the status line. The badge says COMMAND. Type a
command and press `Enter`:

```
:split-window -h -p 30 htop
```

| Key | Does |
|---|---|
| `Enter` | Run it. A mistake shows in the status line, with a suggestion when the name is close ("did you mean split-window?"). |
| `Tab` | Complete the command's name: the only match, or what the matches start with. With nothing more to add, the matches are listed in the status line. |
| `↑` / `↓` | Earlier / later commands you ran (kept while corgi runs). |
| `Backspace`, `C-u` | Delete a character / clear the line. |
| `Esc`, `C-c`, `C-g` | Cancel. |

`C-b ?` (`list-keys`) lists every binding. Type to filter, and press `Enter` to run the selected
binding's command.

### Prompting for part of a command

`command-prompt` takes a label (`-p`), text to start with (`-I`), and a template: what you type
replaces each `%%` in it. The tmux preset's `C-b '` is
`command-prompt -p index "select-window -t ':%%'"`. This one asks for a host and opens a window
with ssh to it:

```toml
[[keys.bind]]
key = "S"
command = 'command-prompt -p host "new-window -n %% ssh %%"'
note = "ssh to…"
```

## Command syntax

- Words are split on spaces. Quotes (`"…"` or `'…'`) keep a word together; a backslash escapes
  the next character (not inside single quotes).
- Options follow the command, tmux style: `-h`, `-p 30` or `-p30`, `-hp 30`. `--` ends the
  options. Options are read only before the first other word, so `split-window -h vim -R` passes
  `-R` to vim.
- In TOML, use a single-quoted string when the command has double quotes in it:
  `command = 'confirm-before -p "sure? (y/n)" kill-window'`.

## Binding commands to keys

Add a `[[keys.bind]]` to your config (`~/.config/corgi/config.toml`), then reload with `C-b R`:

```toml
[[keys.bind]]
key = "g"
command = "display-popup -E -w 90% -h 90% -T lazygit lazygit"
note = "lazygit"
group = "Tools"
```

| Field | Meaning |
|---|---|
| `key` | The key, in tmux notation (see below). Required. |
| `command` | A corgi command, as at the `:` prompt. Required. |
| `table` | `"prefix"` (the default: after `C-b`), `"root"` (no prefix), `"resize"`, or a table of your own. |
| `repeat` | `true`: in the prefix table, more presses within `keys.repeat_time` need no prefix; in other tables, the table stays in use after the key. Default `false`. |
| `note` | What the key-hints dialog and `C-b ?` call it. Default: the command. |
| `group` | The key-hints dialog group it goes in. Default `"Custom"`, shown last. |

A binding replaces whatever the preset had on that key in that table. A `root` binding sees the
key before the pane does, so the program in the pane never gets that key:

```toml
[[keys.bind]]
table = "root"
key = "M-h"
command = "select-pane -L"
```

### Key notation

| Write | For |
|---|---|
| `a`, `%`, `N`, `\|` | A character. Shifted characters are written as what they type: `N`, not `S-n`. |
| `C-a`, `M-a`, `C-M-a` | With Ctrl, with Alt (Meta), with both. |
| `Up` `Down` `Left` `Right` | Arrow keys (`C-Left`, `M-Up`, …). |
| `Enter` `Tab` `BTab` (or `S-Tab`) `Space` `Escape` (or `Esc`) `BSpace` | Other keys. |
| `Home` `End` `PageUp` (`PPage`, `PgUp`) `PageDown` (`NPage`, `PgDn`) `Delete` (`DC`) `Insert` (`IC`) | Other keys. |
| `F1` … `F12` | Function keys. |

### Removing keys

`keys.unbind` removes keys from the preset. A plain key means the prefix table; put a table name
first for another table:

```toml
[keys]
unbind = ["%", "root M-h", "resize h"]
```

`keys.preset = "none"` starts from nothing but `d`, `:`, `?`, `m` and `u` after the prefix (and the
prefix twice, which sends it); build the rest with `[[keys.bind]]`.

A binding corgi can't read is an error that names it. At startup corgi stops before it takes
the screen; on `C-b R` it keeps the config it has and says why in the status line.

## Popups

`display-popup` (alias `popup`) runs a program in a terminal that floats over everything:

```
display-popup [-E] [-w width] [-h height] [-d dir] [-T title] [command]
```

| Flag | Meaning |
|---|---|
| `-E` | Close the popup when the program ends. Without it, the popup stays and its title says `exited (code): any key closes`. |
| `-w`, `-h` | Width and height: a percentage of the screen (`60%`, 1 to 100) or cells (3 or more). Default `80%` each. |
| `-d dir` | The folder to start in. Default: the focused pane's. |
| `-T title` | The title on its border. Default: the command, or `shell`. |
| `command` | What to run. Default: a shell. |

While a popup is open, the badge says POPUP and every key goes to it, pastes included. `C-b x`
closes it (ending its program); `C-b C-b` sends `C-b` to it; other keys after `C-b` do nothing.
Only one popup can be open at a time.

The popup runs on the current workspace's machine. It is a core terminal of its own, not part of
any window, and corgi removes it when the popup closes.

## Shell commands

`run-shell` (alias `run`) runs a shell line in the background, on the current workspace's machine,
in the focused pane's folder, with a 60-second limit. The status line then shows its first line of
output, `<line>: done` if it printed nothing, or `<line>: exit N` and the first line of its errors
if it failed.

```toml
[[keys.bind]]
key = "P"
command = "run-shell git pull --ff-only"
note = "git pull"
```

For a program you want to watch or type into, use a pane, a window or a popup instead:

```
split-window -h -p 30 cargo watch -x test
new-window -n logs tail -f log/app.log
display-popup -E lazygit
```

A pane or window started with a command closes when the command ends.

### How a command line reaches the shell

corgi passes the words after the options to the shell as you wrote them, so `$VARS`, `~`, `|`,
`;` and `&&` work: `run-shell make test 2>&1 | tail -n 1`. A word you quoted (because it has
spaces or quotes in it) is passed in single quotes, so it stays one word for the shell, and
`$VARS` inside it are not expanded. That means you should not quote the whole line:
`run-shell 'make test'` asks the shell for a program called `make test`. To quote for the shell
itself, use `sh -c`: `display-popup -E sh -c 'make test; read'`.

## Sending keys to a pane

`send-keys` (alias `send`) types into the focused pane. Each word that is a key name (in the
notation above: `Enter`, `C-c`, `Up`, `F5`) is sent as that key; any other word is sent as text.

```toml
[[keys.bind]]
table = "root"
key = "M-t"
command = "send-keys 'cargo test' Enter"
note = "run the tests"
```

A single character is a key name too, so `send-keys a` types `a`. `send-prefix` sends the prefix
key itself, as `C-b C-b` does.

## Asking first

`confirm-before` (alias `confirm`) asks y/n in the status line, then runs the command on `y`. Any
other key cancels.

```toml
[[keys.bind]]
key = "D"
command = 'confirm-before -p "detach from #S? (y/n)" detach-client'
```

`-p` sets the question (default `are you sure? (y/n)`). These placeholders are filled in:

| Placeholder | Becomes |
|---|---|
| `#P` | The focused pane's address (`t3`). |
| `#I` | The window's index. |
| `#W` | The window's name. |
| `#S` | The workspace's name. |

If the command after the question has quotes of its own, quote the whole command:
`confirm-before -p "send? (y/n)" "send-keys 'git push' Enter"`.

`kill-pane`, `kill-window` and `kill-workspace` already ask when `ui.confirm_close` says to (see
[workspaces and windows](workspaces-and-windows.md#closing-and-when-corgi-asks-first)).

## Key tables of your own

`switch-client -T name` (alias `switchc`) makes the next key come from the table `name`, instead
of the usual keys. Give a table bindings with `table = "name"`, and a key to enter it:

```toml
[[keys.bind]]
key = "G"
command = "switch-client -T git"
note = "git…"

[[keys.bind]]
table = "git"
key = "s"
command = "display-popup -T 'git status' git status"
note = "status"

[[keys.bind]]
table = "git"
key = "l"
command = "display-popup -E -w 90% -h 90% lazygit"
note = "lazygit"
```

Now `C-b G` then `s` shows `git status` in a popup. While a table is in use:

- The badge shows the table's name in capitals (GIT), and the key-hints dialog lists its keys
  (unless `keys.hints = "off"`).
- A key bound there runs its command. Then you are back to the usual keys, unless the binding has
  `repeat = true`, which keeps the table in use.
- `Esc` or `Enter` leaves the table. A key the table doesn't bind leaves it too, and does nothing
  else.
- The prefix leaves the table and starts a prefix key.

The built-in `resize` table works this way: the herdr preset enters it with `C-b r`.

## Related

- [Commands](commands.md): every command and flag
- [Key bindings](keybindings.md): the default keys, table by table
- [Configuration](configuration.md): `[keys]`
- [Coming from tmux](coming-from-tmux.md)
