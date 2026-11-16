# Agent status

Where an agent's status comes from, what each state means, and how corgi shows it from pane up to workspace.

## Agents report their own status

A coding agent tells herdr-core what it is doing by printing an OSC 7501 escape sequence to its terminal. Core reads it, stores it on the terminal, and passes it on to every client. corgi shows what core has. It never guesses.

The sequence is a list of `key=value` fields separated by `:`, between `ESC ] 7501 ;` and `ESC \`:

```sh
printf '\033]7501;state=working:app=my-agent\033\\'
```

The fields corgi uses:

| Field | What it is |
|---|---|
| `state` | What the agent is doing: `working`, `blocked`, `error`, `done`, `idle`. |
| `app` | The agent's name, for example `claude-code`. corgi shows it in the Agents list. Without it, corgi uses the name of the program in front. |
| `title` | What it is doing, in a few words, base64-encoded. It appears in the pane's breadcrumbs or tab. |
| `msg` | A longer message, base64-encoded. corgi uses it when there is no `title`, and as a toast's text. |

`state=clear` removes the status.

Whether an agent reports at all depends on the agent and its herdr-core integration. corgi shows any program that sends the sequence, whatever it is.

## The states

| corgi shows | From `state` | Meaning |
|---|---|---|
| **blocked** | `blocked` or `error` | It is waiting for you: a permission, a question, a login, or an error. |
| **working** | `working` | It is busy with a turn. |
| **done** | `done` or `idle`, with a turn you haven't seen | It finished a turn while you were looking elsewhere. |
| **idle** | `done` or `idle`, already seen | It finished, and you have seen the result. |
| **unknown** | anything else | It reported a state corgi doesn't know. |

A pane that never reported has no status. It is not listed as an agent, and it gets no dot. Its breadcrumbs row still shows a muted spinner while a command runs in it.

An agent stops counting as soon as the pane's own shell is back in front, even if it never cleared its status (for example, because it was killed). A terminal started as the agent itself (`hc run claude`) keeps its status while the agent runs.

## Done and seen

"Done" means: a turn ended that you haven't looked at.

Core counts each agent's turns. corgi remembers the last turn you saw in the terminal's `herdr.seen` label. When the turn count is higher than `herdr.seen`, the agent is **done**. When you look at it, corgi sets `herdr.seen` to the current turn and the agent becomes **idle**.

You are "looking at" a pane when it's the focused pane of the window on your screen and your terminal window has focus. Looking at a pane also dismisses its toasts.

The label lives on the terminal in core, so every client agrees on it: other corgi clients, and the herdr desktop app, which uses the same `herdr.seen` label. Seeing a result in one marks it seen everywhere.

## Where status shows

- **Breadcrumbs.** At the end of each pane's breadcrumbs row: a spinner while the agent works, a `●` in the state's colour for the others (blocked is red and done green in the default themes). A plain shell at its prompt shows nothing.
- **Tab bar.** Each window's tab has a dot for its most urgent pane.
- **Sidebar, Workspaces.** Each workspace row has a dot for its most urgent window. In the tree, windows and panes get dots too.
- **Sidebar, Agents.** Every agent in every workspace, with its state, name and `workspace:window`. `sidebar.agent_sort = "priority"` puts blocked first, then done, working and idle. The default, `"grouped"`, keeps workspace order.
- **Usage segment.** The `agents` segment in the tab bar counts them: `1 waiting · 2 working · 1 done`.
- **Toasts.** When an agent becomes blocked or done in a pane you aren't looking at, a toast appears. Click it to go there. While your terminal window isn't focused, corgi also passes the notice on to your terminal. See [Notifications](notifications.md).

With `ui.indicators = "dots"` (the default), blocked, done and working are all `●` in their own colour, idle is `○` and unknown is `·`. With `"symbols"`, they are `×` blocked, `✓` done, `◐` working, `○` idle and `·` unknown. The colours are the theme's `blocked`, `done`, `working`, `idle` and `unknown`. See [Themes](themes.md).

## Rollup

A window takes the most urgent state of its panes, and a workspace the most urgent of its windows. From most to least urgent:

```
blocked  >  done  >  working  >  idle  >  unknown
```

So a workspace with one agent working and another waiting for you shows as blocked. Done ranks above working: a finished result you haven't seen needs you more than an agent that's still busy.

## Jump to an agent

| Key (tmux preset) | Command | Goes to |
|---|---|---|
| `C-b a` | `next-agent -b` | an agent waiting for you; press again for the next |
| `C-b A` | `next-agent -d` | an agent that finished unseen |

Both work across workspaces. Other forms you can run at the `C-b :` prompt or bind to keys:

```
next-agent -a        any agent
previous-agent       the previous one waiting for you (-d: finished, -a: any)
select-agent 3       the third agent in the sidebar's order
```

The herdr preset has no agent keys by default. See [Key bindings](keybindings.md).

## Why corgi doesn't read the screen

corgi never works out status from what a pane shows. herdr-core has the same rule. Reading the screen means matching an agent's text and spinners, which change with every release, theme and language, and a wrong match marks an agent as waiting when it isn't. A status the agent reports itself is exact, and it reaches every client the same way.

## Agents that never report

An agent that doesn't send OSC 7501 has no status in corgi. You still see that something runs in its pane: the breadcrumbs row shows a muted spinner while a program is in front.

What you can do:

- Check whether the agent, or herdr-core, has an integration that reports status for it.
- Have the agent's hooks print the sequence. For example, a hook that runs when the agent needs permission writes `\033]7501;state=blocked:app=my-agent\033\\` to the terminal the agent runs in, and one that runs when a turn ends prints `state=done`.
- If the agent sends desktop notifications with OSC 9 or OSC 777, corgi shows those as toasts, but only for panes in the window on your screen: panes in other windows aren't attached, so their notifications don't reach corgi. They don't give the agent a status.

## Related

- [First session: run an agent](first-session.md#run-an-agent-and-watch-its-status)
- [Pane tabs](pane-tabs.md): where an agent's summary comes from
- [Labels](labels.md)
- [Notifications](notifications.md)
