# Sidebar tokens

The row templates of the sidebar and tree chooser, the tokens they take, and how agents are sorted.

## Templates

Set in `[sidebar]` (see [configuration](configuration.md#sidebar)). The tree chooser (`C-b w`, `C-b s`) uses the same templates as the sidebar.

| Key | Row | Default |
|---|---|---|
| `workspace_row` | A workspace in the tree. | `"{state} {workspace} {branch}"` |
| `window_row` | A window in the tree. | `"{state} {window}"` |
| `pane_row` | A pane in the tree (windows with more than one pane, unfolded). | `"{state} {agent}"` |
| `agent_row` | A row in the agents list. | `"{state} {agent} {workspace}:{window}"` |

Machine rows (once you have other machines) have no template: they show the line's state, the machine's name and what's wrong with it.

## Tokens

| Token | Becomes | `workspace_row` | `window_row` | `pane_row` | `agent_row` |
|---|---|---|---|---|---|
| `{state}` | The status glyph in its colour (rolled up: the most urgent below it), per `ui.indicators`. Nothing when there's no agent. | yes | yes | yes | yes |
| `{workspace}` | The workspace's name: yours, else the checkout's or folder's name. | yes | yes | yes | yes |
| `{branch}` | The git branch of the workspace's folder. Drawn muted. | yes | | | |
| `{machine}` | The workspace's machine; nothing on this machine. | yes | yes | yes | |
| `{window}` | `index:name` (`0:zsh`). | | yes | yes | yes |
| `{pane}` | The pane's terminal address (`t7`, `vps/t7`). | | | yes | yes |
| `{agent}` | The agent's name (what it reports, else the program); for a pane without an agent, the pane's name. | | | yes | yes |
| `{title}` | The title the terminal set, as core reports it. | | | yes | yes |
| `{label:KEY}` | The value of label `KEY` on the pane's terminal (see [labels](labels.md)). | | | yes | yes |

A token with no value (an empty column above, a workspace not in git, a label that isn't set, a token corgi doesn't know) vanishes, and the doubled space it leaves is closed up. A `{` without a closing `}` is kept as text.

A workspace that is a git worktree of another workspace's repository is listed under it with `↳` before its row.

```toml
[sidebar]
workspace_row = "{state} {workspace} {machine} {branch}"
pane_row = "{state} {agent} {title}"
agent_row = "{state} {agent} · {label:role}"
```

## Agent sorting

`sidebar.agent_sort`:

| Value | Order |
|---|---|
| `"grouped"` (default) | The tree's order: workspaces (each worktree workspace after its repository's), windows, then panes in the order they were made. |
| `"priority"` | Most urgent first: waiting for you, finished unseen, working, idle, unknown; ties in the tree's order. |

`select-agent N` and `next-agent` always use the tree's order, whatever `agent_sort` says.

Agents that don't fit become "+N more"; with none, the list says "no agents running". The status states are explained in [agent status](agent-status.md).
