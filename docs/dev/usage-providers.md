# Usage providers

How the tab bar's usage segments are read and placed: the `Source` enum, the per-segment loops, the watched target that makes segments follow the workspace's machine, the bounded program runner, and how to add a provider.

For the user's side (what each segment shows, the `[[usage.segment]]` keys) see the user reference.

## The pieces

| File | Role | Pure? |
|---|---|---|
| `crates/corgi/src/config.rs` | `Usage`, `Segment` (provider, priority, interval, format, short, cmd, timeout, bg, fg, show, limit, on), `PROVIDERS`, validation | yes |
| `crates/corgi/src/usage_sources.rs` | `Source`: where a value comes from, and the log readers | no: reads files, runs programs |
| `crates/corgi/src/app/usage.rs` | the loops, the watched target, readings, drawing helpers, `C-b u` | app side |
| `crates/corgi/src/usage.rs` | `Reading`, `place` (the allocator), `fill`, number and time formatting | yes |
| `crates/corgi/src/process.rs` | `process::run`: a program's output within a deadline | no |

There is no provider trait; `docs/PLAN.md` §5 sketched one, and the code uses an enum.

## Readings

A provider produces a `usage::Reading`:

- `fields`: values a `format` or `short` template can use (`{tokens_today}`, `{left}`, `{output}`, …);
- `full`, `short`, `icon`: renderings, widest first; empty ones are skipped;
- `detail`: lines for the `C-b u` dialog, `true` marking a warning.

`None` means nothing to show (no usage today, a failed command): the segment takes no room.

## Sources

`usage_sources::Source` is one enum variant per provider: `Claude(Logs, Show, Option<Limit>, PathBuf)`,
`Codex(Logs, Show)`, `Command { line, timeout }`, `Clock`, `Host`, `System`. `agents` isn't a source: the app counts
agents from status on every draw (`App::agents_reading`), with no IO.

- `Source::new(&Segment)` picks the variant from `seg.provider`; `None` for `agents`, or `command` without `cmd`.
- `Source::read(&mut self, now)` blocks (files, processes). It's always run off the UI task.
- `Source::remote_line()`: what to run on another machine to read it there (`command`'s line, `hostname`, the load
  average), or `None` for sources that only make sense here (logs, the clock).
- `Source::remote_reading(&self, stdout)`: the reading from what that line printed.

A source keeps state between reads. `Logs` (Claude and Codex) remembers each `.jsonl` file's offset and reads only
new complete lines, at most 4 MiB per file per read (`CHUNK`); it looks for new files once a minute
(`REDISCOVER`), forgets files that went, skips files untouched for a week, and counts each Claude message once.
The Claude source also reads the rate limits `corgi claude-statusline` saved (`claude_tap::path`).

## The loops

`App::start_usage` (`app/usage.rs`) runs at startup and on every config reload:

```
generation += 1                          (old loops see the change and stop)
for each segment i with a Source:
  spawn loop while generation unchanged:
     (rev, where) = target.borrow_and_update()       only if the segment follows the workspace
     where = Here  ─► spawn_blocking(source.read(now))
     where = There(machine, client) and remote_line ─►
             select { exec_for(client, line, machine, timeout) ─► remote_reading,
                      target changed ─► read again for the new target }
     where = Down(machine) and remote_line ─► wait for the target to change; the last reading goes stale
     send Msg::Usage(generation, i, rev, reading)
     sleep interval  (or until the target changes)
```

`App::usage_arrived` drops readings from an old generation, and, for segments that follow the workspace, readings
whose `rev` isn't the current target's: they were read for the machine the workspace was on before. It applies the
segment's `format` and `short` with `usage::fill` and stores `(Reading, Instant)`.

### The watched target

`usage::State::target` is a `tokio::sync::watch` of `(revision, Where)`:

- `Where::Here`: the active workspace is on this machine;
- `Where::There(name, Client)`: on another machine whose line is up, read through this client;
- `Where::Down(name)`: on another machine whose line is down.

`App::usage_target` recomputes it from the active workspace and `App::machines`, and bumps the revision only when it
really changed (`Where::same`). It's called from `App::sync`, `App::machines_arrived` and `start_usage`. A loop
waiting on `rx.changed()` wakes at once, so a workspace switch re-reads `host` and `system` immediately, and a
read in flight for the old machine is abandoned.

`Segment::follows_workspace` (`config.rs`): `on = "workspace"`, or, with `on` unset, the `host` and `system`
providers. Sources without a `remote_line` always read here.

## Failure and staleness

- A source that fails returns `None`; the segment disappears until a read succeeds.
- A remote read that errors (machine.exec failed, timed out after the segment's `timeout`) also stores `None`.
- A reading older than 3× the segment's `interval` is stale: `App::usage_now` marks it, and it's drawn dimmed. A
  machine that's down stops its segment's reads, so its last value goes stale rather than vanishing.
- The `C-b u` dialog (`App::open_usage`) shows each segment's `full` and `detail`, notes "not updated lately" for a
  stale one, and the core version.

## Placing segments

`usage::place(free, &[(priority, Option<&Reading>)])` is pure:

1. Segments with a reading, highest priority first (stable for equal priorities).
2. For each, the widest rendering that still fits, counting `PAD` (a space each side) and `GAP` (a column between
   blocks).
3. A segment that can't fit is skipped; smaller, lower-priority ones may still go in.
4. The result is returned in configured order, left to right.

`App::draw_status_line` computes `free` from what the window tabs and the message leave; tabs are never pushed out.
`App::segment_style` gives each block its colours: the segment's `bg`/`fg`, else the provider's default.

## The process runner

`process::run(prog, args, timeout, keep)` runs a short program for its output, used by the `command` source
(`sh -c <line>`, 64 KiB kept) and the clipboard reader:

- stdin is null, stderr dropped; on Unix the program leads its own process group;
- a thread reads stdout as it comes, keeping the first `keep` bytes and dropping the rest (`Output::cut`), so a full
  pipe can't stall it;
- one deadline covers the exit and the output closing; past it, the whole group is killed and the error says
  "took too long";
- if the program ends but something it started still holds stdout, that's killed after 250 ms (`LINGER`) and what
  was printed is kept.

Its tests (`output_exit_and_limits`, `the_deadline_holds`) cover big output, a hang and a background straggler.

## Adding a provider

1. Add a variant to `Source` and a match arm in `Source::new`, taking what it needs from `Segment`. Add new
   `Segment` options to `config.rs` with defaults in `Default for Segment`.
2. Implement it in `Source::read`. Keep reads incremental and bounded: a read runs every `interval` for as long as
   corgi runs. Run programs through `process::run`, never `Command::output`.
3. Fill `fields` for templates, give `full` and, where it helps, `short` and `icon`, and `detail` lines.
4. If it can be read on another machine, add `remote_line` and `remote_reading`, and decide its
   `follows_workspace` default in `config.rs`.
5. Add the name to `config::PROVIDERS` (validation rejects unknown providers), document it in
   `default-config.toml`, and give it a block colour in `App::segment_style` if the default doesn't suit.
6. Test it in `usage_sources.rs`'s `tests` with scratch folders and a fixed `now`, as the Claude and Codex tests do.
   If it reads logs, point its folder at the test's own (the e2e harness sets `CLAUDE_CONFIG_DIR` and
   `CODEX_HOME` per test for this reason).
