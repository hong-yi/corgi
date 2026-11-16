# Usage providers

Each usage segment provider: where its data comes from, the fields you can put in `format` and `short`, its options, and its defaults.

Segments are set with `[[usage.segment]]` (see [configuration](configuration.md#usagesegment)); what they look like and how they share the tab bar is in [usage status](usage-status.md).

## Common to every segment

| Key | Default | Meaning |
|---|---|---|
| `provider` | required | One of the providers below. |
| `priority` | `50` | Higher is placed first, in the widest of its full, short and icon forms that fits; window tabs always come before segments. |
| `interval` | `"30s"` for every provider | How often it's read (at least 1 s). `agents` ignores it. |
| `format` | the provider's text | The full text. `{field}` is replaced by the field's value; a field the provider doesn't have right now stays as written. |
| `short` | the provider's short text | The text used when the full one doesn't fit. |
| `bg`, `fg` | the provider's colours | The block's colours. `bg = "none"`: plain text, no block. |

With no `[[usage.segment]]` at all you get `agents` (priority 90), `claude` (60) and `codex` (50). A segment with nothing to show (no usage today, a failed command) takes no room. A reading older than three intervals is drawn dimmed, and `C-b u` marks it "(not updated lately)". `C-b u` (or a click on the segments) shows every segment's details.

## agents

Agents across every workspace, by state, from the status they report (no files read).

| Field | Value |
|---|---|
| `{waiting}` | Agents waiting for you. |
| `{working}` | Agents working. |
| `{done}` | Agents that finished a turn you haven't seen. |
| `{idle}` | Idle agents (and those in an unknown state). |

| | Text |
|---|---|
| Full | `2 waiting · 1 working · 3 done` (the parts that aren't 0), or `4 idle` |
| Short | `!2 ●1 ✓3` |
| Icon | `●` and the total |
| Colour | The theme's `accent` |

Read whenever the tab bar is drawn; hidden when there are no agents.

## claude

Claude Code's limits and tokens.

Where the data comes from:

- Its rate limits (the 5-hour and 7-day windows), as Claude Code tells its status line command. Put `corgi claude-statusline -- <your command>` in front of your status line in `~/.claude/settings.json`; it saves them to `claude-usage.json` in corgi's state folder (see [CLI](cli.md#corgi-claude-statusline) and [files and paths](files-and-paths.md)).
- Tokens: the `.jsonl` logs under `$CLAUDE_CONFIG_DIR/projects`, else `~/.claude/projects`, changed within the last week. Each message is counted once.

| Field | Value |
|---|---|
| `{tokens_today}` | Tokens today (input, output, cache written and read), as `950`, `12.3k`, `4.5M`. |
| `{messages_today}` | Messages today. |
| `{block_tokens}` | Tokens in the current 5-hour block. |
| `{block_left}` | Time left in the current block (only while one is running). |
| `{left}` | Percent left of the tightest limit (`78%`). Only when a limit is known. |
| `{used}` | Percent used of it. |
| `{window}` | Which limit: `5h` or `week` from Claude Code; empty for an estimate. |
| `{limit}` | The token budget an estimate uses (only for an estimate). |

Options:

| Key | Default | Meaning |
|---|---|---|
| `show` | `"left"` | `left`: percent left; `used`: percent used; `tokens`: tokens today. |
| `limit` | unset | Without Claude Code's numbers: estimate the 5-hour block's share left of a token budget. `"max"`: your biggest finished block in the last week. A number: that many tokens. Unset: no estimate, tokens instead. |

| | Text |
|---|---|
| Full, with a limit | `claude 78% left of week` (`used` with `show = "used"`); an estimate has no `of …` |
| Short, with a limit | `claude 78%` |
| Full, tokens | `claude 12.3k today · 2h 10m left` |
| Short, tokens | `claude 12.3k` |
| Icon | `✻` and the percent or the tokens |
| Colour | Claude's coral, `#d97757` |

A window past its reset time counts as fresh (0% used).

## codex

Codex's tokens and its own rate-limit windows, from the session logs under `$CODEX_HOME/sessions`, else `~/.codex/sessions`.

| Field | Value |
|---|---|
| `{tokens_today}` | Tokens today. |
| `{primary_used}`, `{primary_left}` | The first rate-limit window, percent used and left. |
| `{secondary_used}`, `{secondary_left}` | The second window. |
| `{left}`, `{used}` | The tightest window, percent left and used. |
| `{window}` | Its length: `5h`, `7d`, `30m`. |

Option `show`, as for `claude` (`"left"` by default).

| | Text |
|---|---|
| Full, with a window | `codex 64% left of 5h` |
| Short, with a window | `codex 64%` |
| Full, tokens | `codex 1.5k today · 64% of 5h left`, or `codex 1.5k today` |
| Short, tokens | `codex 1.5k` |
| Colour | The theme's `badge` |

## command

The first non-empty line a shell line prints (through `sh -c`).

| Field | Value |
|---|---|
| `{output}` | That line. |

| Key | Default | Meaning |
|---|---|---|
| `cmd` | required | The shell line. |
| `timeout` | `"5s"` | Past this it's stopped and shows nothing. |
| `on` | `"here"` | `"workspace"`: run it on the active workspace's machine (through core), read again at once when that changes. |

Full text: the line. Short: its first 16 characters and `…` (only when it's longer). `C-b u` shows its first 12 lines. A command that fails or times out shows nothing. Colour: the theme's `border_active`.

```toml
[[usage.segment]]
provider = "command"
cmd = "gh pr status --json number -q '.currentBranch.number'"
interval = "2m"
priority = 20
```

## clock

| Field | Value |
|---|---|
| `{time}` | Local time, `HH:MM`. |

Always read on this machine. Colour: the theme's `border`.

## host

| Field | Value |
|---|---|
| `{host}` | The full host name. |

Full text: the name up to its first dot. `on` defaults to `"workspace"` (on another machine it runs `hostname` there). Colour: the theme's `border`.

## system

| Field | Value |
|---|---|
| `{load}` | The 1-minute load average (`0.42`). |

Full text `load 0.42`; short `0.42`. `C-b u` shows the 1, 5 and 15 minute averages. `on` defaults to `"workspace"`; on another machine it reads `/proc/loadavg` or `sysctl -n vm.loadavg`. Not available on Windows. Colour: the theme's `border`.

## Timing for other machines

Segments with `on = "workspace"` (`command`, `host`, `system`) read through core's `machine.exec`, limited by the segment's `timeout` (at least 1 s). While that machine's line is down they aren't read. `claude`, `codex`, `clock` and `agents` always read here.

Not yet: memory and CPU, cost (it needs a price table), and segments from plugins.
