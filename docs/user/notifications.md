# Notifications

Get told when an agent needs you or finishes, or when a program sends a notification, in panes you aren't looking at.

corgi delivers a notice in two ways:

- **Toasts**: small boxes over the panes, inside corgi.
- **Host notifications**: passed on to the terminal corgi runs in, which turns them into desktop notifications. Only while that terminal's window isn't focused.

## What makes a notice

| Source | Toast | When |
|---|---|---|
| An agent's status (OSC 7501) turns to blocked | `● claude needs you` | It now waits for you |
| An agent's status turns to done | `✓ claude finished` | A turn ended; again after each new turn |
| A program sends OSC 9 or OSC 777 | `◆ <its title>` | Each notification |
| A machine with workspaces goes down, or comes back | `● vps unreachable`, `✓ vps is back` | See [remote-machines.md](remote-machines.md) |

An agent's toast shows where it is (`workspace:window`) after its title, and its status message (or title) underneath. A program's toast uses the title it sent; without one, the agent's or pane's name.

`notifications.agents = false` turns off the agent-status notices (toasts and host notifications). Notifications that programs send still come through.

Status comes only from OSC 7501, which agents report themselves. See [agent-status.md](agent-status.md).

### Nothing for the pane you're looking at

corgi never notifies you about the pane you're looking at. That's the focused pane, while your terminal window has focus and the keyboard isn't in the sidebar. Looking at a pane also dismisses its toasts.

### Limits

- OSC 9 and OSC 777 reach corgi only from panes in the window on screen. Panes in other windows aren't attached, so only their agent status arrives.
- kitty's OSC 99 from programs in panes isn't read. corgi can still send OSC 99 to your terminal (below).

## Toasts

```toml
[notifications]
toasts = true
position = "top-right"
duration = "6s"
agents = true
```

| Key | Default | Values |
|---|---|---|
| `toasts` | `true` | Show toasts inside corgi |
| `position` | `"top-right"` | `top-left`, `top-center`, `top-right`, `bottom-left`, `bottom-center`, `bottom-right` (`top` and `bottom` mean the centre) |
| `duration` | `"6s"` | How long each one stays: `"400ms"`, `"6s"`, `"1m"`, or a number of milliseconds |
| `agents` | `true` | Also notify on agent status, not only on programs' own notifications |

How toasts behave:

- At most four are on screen. A fifth pushes out the oldest.
- The newest is nearest the edge `position` names.
- A newer notice about the same pane replaces the older one. An agent that reports its status and sends an OSC 9 at the same moment shows once.
- Click a toast to go to its pane.
- Control characters and escape sequences in what programs send become spaces. Nothing reaches the screen raw.

## Host notifications

corgi passes notices on to the terminal it runs in, as escape sequences the terminal shows as desktop notifications. Nothing outside the terminal is involved (no D-Bus, no `osascript`), so this works over ssh too.

```toml
[notifications]
host = "auto"
```

| Value | Sends | Terminals |
|---|---|---|
| `"auto"` | One of the below, by what the environment says your terminal is | — |
| `"osc777"` | `OSC 777 ; notify ; title ; body` | Ghostty, WezTerm, foot, urxvt |
| `"osc9"` | `OSC 9 ; title: body` | iTerm2 |
| `"osc99"` | kitty's OSC 99 | kitty |
| `"bell"` | BEL | Any: a sound, bounce or badge |
| `"off"` | Nothing | — |

What `auto` picks:

| If | Then |
|---|---|
| `KITTY_WINDOW_ID` is set, or `TERM=xterm-kitty` | `osc99` |
| `TERM_PROGRAM` is `ghostty`, `WezTerm` or `corgi`; or `TERM` is `xterm-ghostty`, `wezterm`, `foot…` or `rxvt-unicode…` | `osc777` |
| `TERM_PROGRAM=iTerm.app`, or `LC_TERMINAL=iTerm2` | `osc9` |
| Anything else | `bell` |

`TERM_PROGRAM` doesn't survive ssh or tmux; `LC_TERMINAL` often does. If `auto` picks the bell for a terminal that can do better, set `host` yourself.

When corgi sends:

- Only while your terminal's window isn't focused. Inside corgi, the toast is enough.
- If your terminal never reports focus, corgi can't tell, and sends for every pane you aren't looking at.
- At most one per pane within 2 seconds.
- `toasts = false` doesn't stop host notifications. Set `host = "off"` for that.

### Inside tmux

When corgi runs inside tmux (`TMUX` is set), it wraps the sequences so they pass through tmux to the terminal outside. tmux must allow it:

```tmux
set -g allow-passthrough on
```

The bell needs no wrapping.

## Bells from panes

A program in an attached pane that rings the bell (BEL) rings your terminal's bell too. This doesn't depend on `[notifications]`.

## Turn notifications off

| To stop | Set |
|---|---|
| Toasts | `toasts = false` |
| Desktop notifications | `host = "off"` |
| Agent-status notices (toasts and desktop) | `agents = false` |

## Not built yet

- Sounds: corgi plays none of its own, by agent or otherwise. Only the bell above, or `host = "bell"`.
- A log of past toasts.

## Related

- [agent-status.md](agent-status.md): where blocked and done come from
- [remote-machines.md](remote-machines.md): machine notices
- [configuration.md](configuration.md): every `[notifications]` key
