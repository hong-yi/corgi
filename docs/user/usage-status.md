# Usage status in the tab bar

Show agent counts, Claude and Codex limits, a command's output, the time, the host or the load in the tab bar's free space.

Usage segments sit in whatever room the window tabs leave. Each is a coloured block. When space is short, segments shrink to a shorter form or step aside, by priority. Window tabs always come first.

```
 NORMAL  0:zsh  1:claude*  +        1 waiting · 2 working   claude 52% left of 5h   codex 80% left of 5h
```

Press `C-b u` (`display-usage`) or click the segments to see everything each one knows: bars for each limit, when they reset, token breakdowns, where the numbers come from.

## The defaults

Without any `[[usage.segment]]` in your config, corgi shows three:

| Provider | Priority |
|---|---|
| `agents` | 90 |
| `claude` | 60 |
| `codex` | 50 |

A segment with nothing to show (no agents, no Claude usage today) takes no room. Once you add any `[[usage.segment]]`, your list replaces the defaults completely. To hide them all:

```toml
[usage]
enabled = false
```

## Providers

| Provider | Shows | Data from | Read on |
|---|---|---|---|
| `agents` | `1 waiting · 2 working · 1 done`, or `3 idle` | Agent status (OSC 7501), no IO | — |
| `claude` | Share of Claude Code's limit left, or tokens today | `~/.claude/projects`, and `corgi claude-statusline` | This machine |
| `codex` | Share of Codex's tightest rate-limit window left, or tokens today | `~/.codex/sessions` | This machine |
| `command` | The first line a shell line prints | `sh -c <cmd>` | `on` (default `here`) |
| `clock` | Local time, `14:05` | The system clock | This machine |
| `host` | The hostname, up to the first dot | `gethostname`, or `hostname` there | `on` (default `workspace`) |
| `system` | `load 1.23` (1-minute load average) | `getloadavg`, or `/proc/loadavg` / `sysctl -n vm.loadavg` there | `on` (default `workspace`) |

Memory, CPU percentage and cost aren't built yet.

### agents

Counts every agent across all workspaces: waiting for you (blocked), working, and finished but not yet seen. With only idle agents, it shows `N idle`. Its short form is `!1 ●2 ✓1`. Its fields: `{waiting}` `{working}` `{done}` `{idle}`.

### claude

Claude Code keeps no record of its plan limits on disk. It tells them only to its status line command, so corgi needs to sit in front of yours to see them. See [Real Claude limits](#real-claude-limits).

With them, the segment shows the tightest of the 5-hour and weekly windows: `claude 52% left of 5h`. A window past its reset time counts as fresh.

Without them, it shows tokens today: `claude 1.2M today · 2h13m left` (the time left in the current 5-hour block, counted as ccusage does). Set `limit` to get an estimate instead:

| `limit` | The 5-hour block's budget |
|---|---|
| unset (default) | No estimate: tokens |
| `"max"` | Your biggest finished block in the past week |
| a number, e.g. `20000000` | That many tokens |

Its fields: `{left}` `{used}` `{window}` (`5h` or `week`) `{limit}` `{tokens_today}` `{messages_today}` `{block_tokens}` `{block_left}`.

### codex

Codex writes its own rate-limit windows to its session logs. The segment shows the tightest: `codex 80% left of 5h`. Without any, it shows `codex 1.2M today`.

Its fields: `{left}` `{used}` `{window}` `{tokens_today}` `{primary_left}` `{primary_used}` `{secondary_left}` `{secondary_used}`.

### command, clock, host, system

| Provider | Fields |
|---|---|
| `command` | `{output}` (the first non-empty line) |
| `clock` | `{time}` |
| `host` | `{host}` (the full hostname) |
| `system` | `{load}` |

A `command` segment shows its command's first non-empty line. Its short form is the first 16 characters. If the command fails, or doesn't finish within `timeout`, the segment hides until the next run that works. `C-b u` shows its first 12 lines.

## Segment options

```toml
[[usage.segment]]
provider = "claude"
priority = 60
interval = "30s"
format = "{tokens_today} · {block_left}"
short = "{tokens_today}"
bg = "#d97757"
fg = "#1e1e2e"
show = "left"
limit = "max"
```

| Key | Default | What it does |
|---|---|---|
| `provider` | — | One of `agents`, `claude`, `codex`, `command`, `clock`, `host`, `system`. Required. |
| `priority` | `50` | Higher keeps its place, and its full text, first |
| `interval` | `"30s"` | How often it reads. At least `"1s"`. `agents` updates as status changes. |
| `format` | the provider's | The full text, from its fields |
| `short` | the provider's | The short text, for when the full one doesn't fit |
| `cmd` | — | `command`: the shell line. Required for `command`. |
| `timeout` | `"5s"` | How long a command (or a read on another machine) may take |
| `bg`, `fg` | the provider's | The block's colours: `#rrggbb`, `#rgb` or a colour name. `bg = "none"`: plain text, no block. |
| `show` | `"left"` | `claude`, `codex`: `"left"` (percent left), `"used"` (percent used) or `"tokens"` (tokens today) |
| `limit` | unset | `claude`: the estimate's budget (above) |
| `on` | see above | `command`, `host`, `system`: `"here"` or `"workspace"` |

A field the provider doesn't have right now (`{block_left}` outside a block, say) stays in the text as written.

A reading older than three intervals is drawn dimmed.

Default colours: `agents` in the theme's accent, `claude` in Claude's coral (`#d97757`), `codex` in the theme's badge colour, `command` in the active border colour, and the rest in the border colour.

## Add a command segment

```toml
[[usage.segment]]
provider = "command"
cmd = "gh pr status --json number -q '.currentBranch.number'"
format = "PR #{output}"
interval = "2m"
timeout = "5s"
priority = 20
```

The line runs with `sh -c`. Keep it fast and quiet: only its first line shows.

With `on = "workspace"`, the line runs on the active workspace's machine through herdr-core, and runs again at once when you switch to a workspace on another machine. When that machine's line is down, the segment keeps its last reading, which dims once it's old. See [remote-machines.md](remote-machines.md).

## Priority and shrinking

Every time the tab bar is drawn:

1. The window tabs take what they need.
2. Segments are taken in priority order, highest first. Equal priorities keep the order they're written in.
3. Each gets the widest of its forms that still fits: full, then short, then icon.
4. A segment that can't fit in any form is skipped. Smaller, lower-priority segments after it may still fit.
5. The segments that made it are drawn in the order they're written in, left to right.

So the most important segment keeps its full text longest, and a long low-priority one never pushes out a short high-priority one.

The forms each provider has:

| Provider | Full | Short | Icon |
|---|---|---|---|
| `agents` | `1 waiting · 2 working` | `!1 ●2` | `●3` |
| `claude` | `claude 52% left of 5h` | `claude 52%` | `✻52%` |
| `codex` | `codex 80% left of 5h` | `codex 80%` | — |
| `command` | its first line | its first 16 characters, `…` | — |
| `system` | `load 1.23` | `1.23` | — |

`format` replaces the full form and `short` the short one; the icon stays.

## Real Claude limits

Claude Code passes its rate limits (`rate_limits.five_hour` and `.seven_day`) only to the status line command in `~/.claude/settings.json`. `corgi claude-statusline` reads them, saves them for corgi, then runs your own status line command with the same input, so your status line looks as it did.

Put it in front of your command:

```json
{
  "statusLine": {
    "type": "command",
    "command": "corgi claude-statusline -- ~/.claude/statusline.sh"
  }
}
```

- With no command after `--`, it saves the limits and prints nothing.
- One word after `--` runs through `sh -c`, so you can quote a whole line: `corgi claude-statusline -- "~/bin/status --short | head -1"`. Several words run as a program and its arguments.
- Its exit code is your command's.
- When Claude Code sends no rate limits (an API key login, or an older Claude Code), the last saved ones stay.

Claude Code runs its status line as you use it. The `claude` segment picks up the saved limits on its next read, and `C-b u` says how old they are.

## Privacy: what corgi reads

Everything stays on this machine. corgi sends none of it anywhere.

| What | Where | What corgi keeps |
|---|---|---|
| Claude Code's logs | `.jsonl` files under `$CLAUDE_CONFIG_DIR/projects`, else `~/.claude/projects`, changed in the past week | Each message's time and token counts, and its message and request IDs (to count it once). Lines without `"usage"` are skipped unparsed. |
| Claude Code's limits | `claude-usage.json` in corgi's state folder: `$XDG_STATE_HOME/corgi`, else `~/.local/state/corgi` (`%LOCALAPPDATA%\corgi` on Windows) | Written by `corgi claude-statusline`: the time, and the two windows' used percentage and reset time. Nothing else from the status line input. |
| Codex's logs | `.jsonl` files under `$CODEX_HOME/sessions`, else `~/.codex/sessions`, changed in the past week | `token_count` events: time, tokens, and the rate-limit windows. |

corgi reads each log file from where it stopped last time, at most 4 MB per file per read, and looks for new files once a minute. It keeps a week of counts in memory and nothing on disk.

`command`, `host` and `system` run programs: on this machine, or on another through herdr-core with `on = "workspace"`.

## Related

- [usage-providers.md](usage-providers.md): every provider's fields and options, for reference
- [remote-machines.md](remote-machines.md): segments on the workspace's machine
- [themes.md](themes.md): the colours segments use by default
- [configuration.md](configuration.md): `[usage]` and `[[usage.segment]]`
