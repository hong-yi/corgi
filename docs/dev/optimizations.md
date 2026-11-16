# Herman optimization proposals

Review date: 2026-11-11. Source checked against revision `8a16e35`.

Prioritize unnecessary wakeups, stream buffering, and periodic scheduling. These affect idle CPU use,
responsiveness, memory use, and whether background updates run at all. The terminal emulator already caches
unchanged rows and reuses cell text allocations; further emulator changes should follow measurements of the
complete application.

The items below are proposed changes. The disabled-hints CPU issue was measured directly; the other
performance benefits need targeted measurements.

## Priorities

| Order | Priority | Change | Expected benefit | Effort |
|---|---|---|---|---|
| 1 | High | Stop scheduling hint deadlines when hints are off | Lower idle CPU use | Small |
| 2 | High | Bound queued terminal output and delay credit until consumption | Bounded memory and responsive input under load | Large |
| 3 | High | Keep the periodic timer outside the app loop | Reliable branch refreshes during idle and active periods | Small |
| 4 | Medium | Drain usage-command output while the command runs | Avoid pipe stalls and unnecessary timeout failures | Medium |
| 5 | Medium | Move filesystem persistence off the UI runtime | Avoid input and rendering stalls during writes | Medium |
| 6 | Medium | Cache sidebar and status data | Less repeated traversal and allocation per draw | Medium |
| 7 | Medium | Reuse copy-mode text snapshots until content changes | Faster navigation through long scrollback | Small to medium |
| 8 | Medium | Cache log discovery and bound retained metadata | Lower polling cost and memory growth over long sessions | Medium |

Effort describes relative implementation complexity, not a delivery estimate. Items 2, 5, and 6 need particular
care around ordering, shutdown, and cache invalidation.

## Scheduling and stream processing

### Stop wakeups for disabled hints

In [the app loop](../../crates/herman/src/app/mod.rs), the prefix-mode deadline is scheduled whenever
`hints_due()` is false. [The drawing code](../../crates/herman/src/app/draw.rs) returns false for `Hints::Off`.
Once `hint_delay` has elapsed, the remaining delay saturates to zero, repeatedly waking the loop and requesting
draws even though no hints will appear.

An isolated macOS debug build, with usage segments disabled, consumed about 0.24 CPU seconds over 1.51 seconds
while waiting in prefix mode with hints off: about 16% of one CPU core. Normal idle and idle after Escape each
measured approximately zero CPU seconds over comparable intervals. This is a debug-build observation, not a
release performance estimate.

Schedule a hint deadline only for `Hints::Delay`, and only until the hints become visible. After a deadline
fires, redraw for the actual state transition rather than repeatedly because prefix mode remains active.

Validate disabled, always-visible, and delayed hints with explicit deadline tests. Repeat the idle CPU
comparison in release mode, and verify that delayed hints still open without another input event.

### Preserve backpressure through the app queue

[`Attach::recv()`](../../crates/hc-proto/src/client.rs) returns stream credit as soon as it hands a frame to its
caller. [The pane reader](../../crates/herman/src/pane.rs) then sends that frame to an unbounded app queue,
before the UI feeds the bytes to the emulator. Credit therefore limits the protocol queue without bounding
the full path to consumption. The app also drains all currently queued messages before returning to drawing.

Introduce a byte budget for pending terminal output and acknowledge bytes after the emulator consumes them,
or after a bounded downstream stage accepts responsibility for them. Process a limited number of bytes or a
limited amount of work per app iteration, then return to scheduling. Batch credit acknowledgements to reduce
the number of small outgoing frames.

Keep channel ordering intact, including DATA, RESIZE, and CLOSE. Preserve an independent route for connection
and input control messages. Blocking the shared protocol reader behind one full pane queue would also delay
RPC replies and other panes, so avoid implementing the bound by awaiting a full route inside that reader.

Validate with several noisy panes and an intentionally slow consumer. Measure peak queued bytes, resident
memory, input latency, and frame intervals. Verify lossless output, credit accounting, detach, reconnect, and
progress on other channels when one pane reaches its budget.

### Keep the periodic timer alive across iterations

The app loop initializes `last_tick` immediately before each wait. With normal waits capped at three seconds,
the ten-second `TICK` usually never elapses before that timestamp is replaced. The initial `tick()` also runs
before the layout document has loaded. Moving the elapsed-time check after `select!` does not solve the
timestamp reset while its initialization remains inside the loop.

Keep the timestamp outside the loop and check it after either wakeup path, or use a persistent interval that
participates in `select!`. Refresh after the initial layout becomes available. In
[`App::tick()`](../../crates/herman/src/app/actions.rs), deduplicate folders and prevent overlapping refresh
jobs if a slow scan lasts longer than the interval. Skip missed intervals instead of launching catch-up
bursts.

Validate refreshes after startup, during sustained incoming messages, and while idle. Include a slow git
command and confirm that only one refresh job runs at a time.

## Processes and persistence

### Read command output concurrently

The [`command()` usage provider](../../crates/herman/src/usage_sources.rs) polls for process exit before reading
its piped stdout. A command that fills the pipe cannot finish until that pipe is drained, so an otherwise
successful command can be killed at its timeout. The subsequent `read_to_string()` is outside the timeout;
descendants that retain stdout can keep that read waiting after the shell exits.

Drain stdout while the process runs. Retain a bounded amount for the displayed summary and detail lines,
while continuing to drain or explicitly terminating a producer that exceeds the chosen limit. Apply the
deadline to both process completion and output collection. Use asynchronous process waiting and reading,
or a bounded blocking reader, to replace the 20 ms polling loop.

Validate small output, output larger than pipe capacity, nonzero exit, timeout, and a descendant that retains
stdout. Check bounded memory and completion within the deadline, including cleanup of timed-out processes.

### Move persistence off the UI runtime

The application uses a current-thread Tokio runtime. Its save task calls
[`persist::backup()`](../../crates/herman/src/persist.rs) synchronously after `doc_set` completes. That backup
creates directories, writes and renames a file, lists the backup directory, sorts entries, and deletes old
files. `save_view()` also writes synchronously from UI actions. `tokio::spawn` alone does not move these
operations off the runtime thread.

Use a serialized persistence worker backed by `spawn_blocking`, and coalesce pending view updates to the
latest state. Keep backup versions ordered and preserve atomic replacement and retention behavior. Decide
explicitly whether backup completion must delay `Msg::Saved`; filesystem latency should not unnecessarily
hold up the next core document save. Flush pending view state on shutdown with a bounded wait.

Validate ordering and recovery with an injected slow writer. Check that input and drawing continue during a
write, the latest view survives shutdown, failures are reported, and backup retention remains correct.

## Rendering and local data

### Cache sidebar and status data

[`draw_sidebar()`](../../crates/herman/src/app/draw.rs) builds the tree twice, clones rows, and makes another
pass over windows to gather agents. Status-line drawing also clones the active workspace. These operations
repeat during terminal output and spinner animation even when the underlying sidebar information is unchanged.

First remove unnecessary whole-workspace and row clones, and gather agent rows directly from the model.
Then cache derived tree rows and status summaries if profiling still shows a material cost. Invalidate them
on relevant changes: layout, terminal status or title, focus, fold state, filter, branches, configuration, and
theme. Avoid using raw output arrival as a blanket invalidation when the output changes none of these inputs.

Benchmark many workspaces and hidden windows with a fixed visible terminal area. Measure draw time and
allocations while output is busy and while only a spinner changes. Verify fold, filter, focus, title, and
agent-state updates to guard against stale caches.

### Reuse copy-mode snapshots

[`copy_key()`](../../crates/herman/src/app/copymode.rs) calls `screen_lines()` on every keypress.
[`Vt::screen_lines()`](../../crates/herman-vt/src/lib.rs) formats the full scrollback, copies it into a string,
and creates a string for every row. Moving the cursor through unchanged history therefore repeats work
proportional to the history size.

Track a terminal content revision and refresh the text snapshot only when output or a resize changes that
revision. Keep scrolling the viewport separate from changing the text. Preserve the mapping between row
indices, selections, and retained history when old rows are evicted.

Validate repeated motions and searches over a large unchanged history, then append output, resize, and
trigger history eviction. Measure formatting calls and key-handling latency, and verify copied text and
selection positions.

### Cache log discovery and bound metadata

[`Logs::new_lines()`](../../crates/herman/src/usage_sources.rs) already remembers file offsets, but recursively
discovers files and checks their metadata on every poll. It reads each changed file's remaining bytes into
memory at once. Usage entries expire after a week, while the Claude deduplication set and file-offset map
have no corresponding eviction.

Cache discovered paths and rediscover periodically, or add filesystem notifications with periodic
reconciliation. Read changed files in bounded chunks and retain incomplete lines between polls. Expire
deduplication keys with their accounting window and prune metadata for deleted files. Preserve protection
against recounting retained records when files are rediscovered, truncated, or replaced. Recompute usage
aggregates only when new entries arrive or a time boundary changes the result.

Validate large directories, new and deleted files, partial lines, truncation, same-path replacement, and
records repeated across files. Compare filesystem calls, bytes read, and retained metadata over repeated
polls while confirming that usage totals remain unchanged.

## Measurement and regression checks

The existing [VT benchmark](../../crates/herman-vt/examples/bench.rs) reports approximately 280 MB/s parsing,
125 microseconds for an all-row invalidation, 3.7 microseconds for a single-row update, and 67 nanoseconds for
an unchanged capture in the review run. These are local release measurements. Its all-row loop clears the
screen with `ESC[2J`, so it measures erased content rather than repeated captures of the initially styled
screen. Extend it with scrolling styled text, wide glyphs, and actual per-frame content changes before using
it to justify further emulator work.

Add an application benchmark that includes frame capture, copying into the ratatui buffer, sidebar and status
construction, diffing, and output. Compare the same release build, terminal size, configuration, and workload
before and after each optimization. Record idle CPU, median and tail input latency, frame time, queued bytes,
and peak memory as relevant; avoid treating a faster VT microbenchmark as proof of a faster application.

The separate coverage snapshot recorded these provisional production Rust line figures:

| Component | Line coverage |
|---|---:|
| herman | 86.7% |
| hc-proto | 84.5% |
| herman-vt | 89.8% |
| Overall | 86.7% |

That snapshot had 176 passing tests and two failing end-to-end tests:
`sidebar_toggle_and_config_reload` and `worktrees_listed_opened_grouped_and_removed`. LLVM reported 21 functions
with mismatched profile data, and subprocess cleanup can discard counters. The figures exclude test code,
dependencies, generated bindings, build scripts, and native Ghostty/Zig code. They describe observed line
execution, not branch coverage or protection against the scheduling and buffering failures above.

For implementation changes, add the targeted regression checks described with each proposal and run the
repository checks from [the justfile](../../justfile):

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --release --locked -p herman-vt --example bench
```

Use an environment without `NO_COLOR` for the end-to-end tests that assert emitted colors; the original
review needed to unset an inherited `NO_COLOR=1`. Keep core tests isolated with their existing session and
scratch-directory helpers. Recheck coverage after the failing tests and profile collection issues are
resolved, and use the affected paths' behavior as the acceptance criteria for each optimization.
