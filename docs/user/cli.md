# CLI

corgi's command-line options and subcommands, the environment variables it reads, and its exit codes.

## corgi

```
corgi [OPTIONS] [COMMAND]
```

With no command, corgi connects to herdr-core and shows your workspaces. If none exist yet, it starts one.

| Option | Environment | Default | Meaning |
|---|---|---|---|
| `--session <SESSION>` | `HC_SESSION` | `default` | The herdr-core session to use (passed to `hc connect` as `HC_SESSION`). |
| `--socket <SOCKET>` | `HC_SOCKET` | none | Connect to this core socket instead of running `hc connect`. Unix only. |
| `--config <CONFIG>` | `CORGI_CONFIG` | `~/.config/corgi/config.toml` (see [configuration](configuration.md#the-file)) | The config file. |
| `--default-config` | | | Print the default config, with comments, and exit. |
| `-h`, `--help` | | | Help. |
| `-V`, `--version` | | | Print corgi's version (`corgi 0.1.0`). |

The options go before the subcommand: `corgi --session work doctor`, not `corgi doctor --session work`.

| Command | Does |
|---|---|
| `attach <TERMINAL>` | View one core terminal. |
| `doctor` | Check the connection to herdr-core. |
| `claude-statusline [CMD]...` | Claude Code's status line, keeping its rate limits for corgi. |
| `migrate` | Make a corgi config from the herdr TUI's. |
| `help [COMMAND]` | Help for corgi or a subcommand. |

### corgi attach

```
corgi attach <TERMINAL>
```

Opens corgi and shows the core terminal `<TERMINAL>`, an address such as `t7`, or `vps/t7` for one on another machine. corgi checks the terminal exists before it takes the screen.

### corgi doctor

```
corgi doctor
```

Connects to core and prints:

- corgi's version;
- core: `✓ herdr-core <version> on "<name>" (session <session>, protocol generation <n>)`, its features, how many live terminals and other machines it has; or `✗` and why it couldn't connect;
- the layout document `corgi`: its version, or "not written yet";
- every corgi running in a core terminal, with its address and pid; `✗` for one running inside a corgi pane, with how to leave it or stop it;
- the core terminal the doctor itself runs in, if any.

It exits 1 when it can't connect or finds a corgi inside a corgi pane. See [troubleshooting](troubleshooting.md).

### corgi claude-statusline

```
corgi claude-statusline -- <your status line command>
```

Use it as Claude Code's `statusLine` command:

```json
"statusLine": {"type": "command",
               "command": "corgi claude-statusline -- <your command>"}
```

It reads Claude Code's status line JSON from stdin, saves `rate_limits.five_hour` and `rate_limits.seven_day` (with the time) to `claude-usage.json` in corgi's state folder for the `claude` usage segment, then runs your command with the same input, so your status line looks as before. Input without rate limits leaves the saved ones as they were.

| `CMD` | Runs |
|---|---|
| nothing | Nothing; prints nothing. |
| one word (`"~/bin/line.sh --x"` quoted as one) | Through `sh -c`. |
| several words | The first as the program, the rest as its arguments. |

### corgi migrate

```
corgi migrate [--from <FROM>] [--print] [--force]
```

Reads the herdr TUI's config and writes a corgi config on the herdr key preset. What has no counterpart is listed in the file and on stderr. See [migrating from herdr](migrating-from-herdr.md).

| Option | Meaning |
|---|---|
| `--from <FROM>` | herdr's config. Default: `$XDG_CONFIG_HOME/herdr/config.toml`, else `~/.config/herdr/config.toml`. |
| `--print` | Print the new config instead of writing it. |
| `--force` | Replace a corgi config that's there, keeping the old one as `<file>.bak`. |

It writes to `--config` (or `CORGI_CONFIG`), else the usual config path. Without `--force` it won't replace an existing file.

Not yet: `corgi cmd`, `corgi list` and `corgi plugin` (they come with plugins), and showing the core's version in `--version`.

## Environment variables

### Read by corgi

| Variable | Meaning |
|---|---|
| `HC_SESSION` | The core session (`--session`). Default `default`. |
| `HC_SOCKET` | A core socket to connect to (`--socket`). |
| `CORGI_CONFIG` | The config file (`--config`). |
| `CORGI_LOG` | Log level or filter for the log file, in `tracing` `EnvFilter` syntax: `debug`, `corgi=debug,hc_proto=info`. Default `info`. |
| `CORGI_RECORD` | A folder: every byte each pane receives is appended to `<folder>/<address>-<attach>.bin` (a `/` in the address becomes `_`), for replaying with `corgi-vt`'s `dump` example. The folder must exist. |
| `CORGI_NESTED` | Set (to anything) to let corgi start inside a pane of another corgi. That pane then shows a placeholder instead of itself. |
| `HC_TERMINAL_ID` | Set by core inside its terminals; corgi uses it (with its session id) to find the terminal it runs in. |
| `CLAUDE_CONFIG_DIR` | Claude Code's folder; the `claude` segment reads `<it>/projects`. Default `~/.claude`. |
| `CODEX_HOME` | Codex's folder; the `codex` segment reads `<it>/sessions`. Default `~/.codex`. |
| `XDG_CONFIG_HOME` | Config folder base. Default `~/.config`. |
| `XDG_STATE_HOME` | State folder base (logs, backups, views). Default `~/.local/state`. |
| `APPDATA`, `LOCALAPPDATA`, `USERPROFILE` | Windows: config folder, state folder, home. |
| `HOME` | Your home folder. |
| `VISUAL`, `EDITOR` | The editor for `C-b e`; else `vi`. |
| `TERM_PROGRAM`, `TERM`, `LC_TERMINAL`, `KITTY_WINDOW_ID` | How `notifications.host = "auto"` picks a protocol. |
| `TMUX` | Inside tmux: notifications and OSC 52 copies are wrapped for tmux's passthrough. |
| `WAYLAND_DISPLAY` | Linux: read clipboard images with `wl-paste` rather than `xclip`. |

### Set for programs in corgi's panes

| Variable | Value |
|---|---|
| `TERM` | `xterm-256color` |
| `COLORTERM` | `truecolor` |
| `TERM_PROGRAM` | `corgi` |
| `TERM_PROGRAM_VERSION` | corgi's version |

## Exit codes

| Code | When |
|---|---|
| `0` | You detached (`C-b d`), or every program ended (corgi prints `[exited]`). `--default-config`, `--help`, `--version`. `migrate` that wrote or printed a config. `doctor` with no problems. |
| `1` | corgi couldn't start: no core (`hc` not found, core too old, can't connect), a broken config, a terminal `attach` can't find, or a refused nested start. It prints `corgi: <reason>` and the log's path. Also `doctor` finding a problem, and `migrate` failing (it won't replace a config without `--force`, can't read herdr's). |
| `2` | A command-line mistake (an unknown option or subcommand). |
| `128 + n` | corgi was stopped by signal `n` (`143` for SIGTERM, `129` for SIGHUP); it puts your terminal back first. |
| `claude-statusline` | Your command's exit code; `127` if it can't be started; `1` if it was killed; `0` with no command. |

## Related

- [Files and paths](files-and-paths.md)
- [Troubleshooting](troubleshooting.md)
