# ADR 0001: libghostty-vt as the pane emulator

- Status: accepted (M0 spike, 2026-11-09)
- Context: [PLAN.md](../../PLAN.md) §1 D1

## Context

herdr-core sends each viewer the raw output bytes of a terminal. To draw several panes on one screen, corgi needs
its own emulator per pane. That emulator turns the bytes into a grid of styled cells, which corgi then composites
with ratatui.

The candidates were:

- `ghostty-vt`, the same parser core uses. It needs Zig 0.16 and a C binding.
- `alacritty_terminal`, pure Rust.
- `vt100`, too small for modern full-screen programs.

## What the spike did

1. Pinned Ghostty `1e5cdb8`, the commit herdr-core 0.1.19 vendors, and Zig 0.16.0 in `scripts/pins.env`.
   `scripts/bootstrap.sh` downloads them into the repo (`.tools/`, `vendor/`) and installs nothing system-wide.
2. `crates/corgi-vt/build.rs` builds the static `libghostty-vt.a` with Zig, then generates the FFI with bindgen
   (libclang). The bindings cover the render-state API: rows, cells, style, colours, cursor and dirty flags.
3. `corgi_vt::Vt` exposes a small safe API: `new`, `write`, `resize`, and `frame()`, which returns a `Frame` of
   `Cell`s and the `Cursor`.
4. `corgi/src/ui.rs::draw_frame` copies a `Frame` into a ratatui `Buffer`. The spike screen (`corgi` with no
   arguments for now) shows two bordered panes: an ANSI sample, and an echo of your keystrokes.

## Results (macOS arm64, Apple Silicon)

| Check | Result |
|---|---|
| Zig build of libghostty-vt, cold | about 55 s, a one-off. Cached by Zig and cargo after that. |
| Clean `cargo build --workspace` (Zig cache warm) | 47 s |
| Release binary | 2.6 MB, statically linked, no runtime dependencies |
| Rendering in a real PTY | Bold, italic, underline, faint, strike and inverse all correct. 16, 256 and RGB colours correct. CJK, Hangul and emoji wide characters correct. Box drawing correct. Absolute cursor moves land in the right place. The cursor shows only in the focused pane. |
| Parse throughput | about 360 MB/s (10 MB of coloured log lines) |
| `frame()` for 200×50, fully styled | about 0.32 ms per capture |
| Terminal restore | Restored on quit, panic and SIGTERM/SIGHUP: alternate screen left, mouse, paste and focus reporting off, kitty keyboard flags popped, cooked mode back. SIGKILL can't be caught; run `reset`. |

One snag: Ghostty's install step also tries to make an Apple xcframework through `xcodebuild`, and that step failed
in the spike's build environment. Passing `-Demit-xcframework=false` skips it. We only link the `.a`.

## Decision

Use `ghostty-vt`.

- Panes are parsed by the same code as core's screens, so corgi shows what core would.
- Speed is not an issue: one frame costs about 1/50 of a 60 fps budget.
- The library also ships the key and mouse encoders M2 needs.

Keep `corgi-vt` as the only crate that touches the FFI. If Zig ever becomes a blocker, `alacritty_terminal` can
replace it behind the same `Vt`/`Frame` API.

## Consequences

- Building needs Zig 0.16 and libclang:
  - Xcode or the Command Line Tools on macOS
  - `libclang-dev` on Linux
  - LLVM on Windows
- `scripts/bootstrap.sh` supplies Zig. CI uses `mlugg/setup-zig`.
- Prebuilt release binaries need none of this.
- Bindings are generated at build time. If needing libclang becomes a burden, commit the generated `sys.rs` per
  platform instead.
- `frame()` currently allocates a `String` per cell. Before the many-pane work in M9, switch it to the render-state
  dirty rows and keep a cached frame per pane.

## Not yet verified

- Linux and Windows builds. The CI workflow is written but has not run, because the repo has no remote yet.
- Key and mouse encoding through ghostty's encoders (M2).
- Behaviour against a live herdr-core, which isn't installed on this machine (M1/M2).
