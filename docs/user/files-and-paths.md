# Files and paths

Every file corgi reads or writes, per OS, and the core document that holds your layout.

## Folders

| Folder | Linux, macOS | Windows |
|---|---|---|
| Config | `$XDG_CONFIG_HOME/corgi`, else `~/.config/corgi` | `%APPDATA%\corgi` |
| State | `$XDG_STATE_HOME/corgi`, else `~/.local/state/corgi` | `%LOCALAPPDATA%\corgi` |

An empty `XDG_*` variable counts as unset. macOS uses the same paths as Linux (not `~/Library`).

## Files

| File | Where | What |
|---|---|---|
| `config.toml` | Config folder, or `--config` / `CORGI_CONFIG` | Your configuration. Read at startup and on `C-b R`. See [configuration](configuration.md). |
| `config.toml.bak` | Beside the config | The previous config, kept by `corgi migrate --force`. |
| `corgi.log` | State folder | The log, appended to. Level from `CORGI_LOG` (default `info`). |
| `view-<session>.json` | State folder | This client's view, per core session: the workspace you're looking at, which tree rows are folded, and the sidebar width you set. |
| `backups/<ms>-v<version>.json` | State folder | The last 20 layout documents corgi saved, newest last (`<ms>` is the time in milliseconds). |
| `claude-usage.json` | State folder | Claude Code's rate limits, saved by `corgi claude-statusline`. |
| `<address>-<attach>.bin` | The `CORGI_RECORD` folder | With `CORGI_RECORD` set: every byte each pane received. |
| `corgi-scrollback-<pid>-<pane>.txt` | The system temp folder | `C-b e`'s scrollback copy; removed when its popup closes. |
| `<worktrees.directory>/<repository>/<branch>` | On the workspace's machine (default `~/.corgi/worktrees`) | Git worktrees made from the New Workspace dialog. |

corgi reads these files of other programs:

| Path | For |
|---|---|
| `$CLAUDE_CONFIG_DIR/projects/**/*.jsonl`, else `~/.claude/projects/**/*.jsonl` | The `claude` usage segment (files changed within a week). |
| `$CODEX_HOME/sessions/**/*.jsonl`, else `~/.codex/sessions/**/*.jsonl` | The `codex` usage segment. |
| `$XDG_CONFIG_HOME/herdr/config.toml`, else `~/.config/herdr/config.toml` | `corgi migrate`. |

## The layout lives in core

Your workspaces, windows and panes are not in a local file. They are a JSON document named `corgi` on the local herdr-core (in its session, `HC_SESSION`). Every corgi connected to that core reads and writes the same document, so they show the same layout, and it survives corgi exiting and restarting.

- corgi writes it with compare-and-swap. When another client wrote first, corgi takes theirs; its own terminals come back through their [labels](labels.md).
- A document corgi can't read is never written over: corgi says "layout document unreadable (…); not saving over it" and keeps it.
- Each document corgi saves is also copied to `backups/` locally.
- `corgi doctor` shows the document's version.

The terminals themselves live in core too: closing corgi leaves them running.

## Related

- [CLI](cli.md) for the environment variables.
- [Troubleshooting](troubleshooting.md) for collecting logs and using backups.
