# Building

The toolchain, the pinned Zig and Ghostty that `corgi-vt` builds from, the `just` recipes, and notes on cross-compiling and Windows.

## What you need

- Rust stable with `rustfmt` and `clippy` (`rust-toolchain.toml`); the workspace's `rust-version` is 1.88 and its
  edition 2024.
- libclang, for bindgen: the Xcode Command Line Tools on macOS, `libclang-dev` on Linux.
- `curl`, `python3`, `tar`, `shasum` and `git`, for `scripts/bootstrap.sh`.
- `just`, for the recipes (optional: each is one cargo command).
- `hc` (herdr-core) on `PATH` to run corgi and the integration and e2e tests:
  `curl -fsSL https://releases.herdr.dev/core/install.sh | sh`.

## First time

```sh
just bootstrap      # or ./scripts/bootstrap.sh
cargo run           # corgi against your core (C-b d detaches)
```

`scripts/bootstrap.sh` reads `scripts/pins.env` and fetches into the repo only, installing nothing system-wide:

- Zig `ZIG_VERSION` (0.16.0) into `.tools/zig`, from ziglang.org's index, checked against its SHA-256. It skips the
  download when `.tools/zig/zig` already has that version, or when `SKIP_ZIG=1` (Zig is already on `PATH`, as in
  CI). Supported hosts: macOS and Linux, aarch64 and x86_64.
- Ghostty at `GHOSTTY_COMMIT` into `vendor/ghostty`, a shallow fetch of that one commit, skipped when it's already
  checked out.

`.tools/` and `vendor/ghostty` are in `.gitignore`.

## The pins

`scripts/pins.env`:

```sh
GHOSTTY_COMMIT=1e5cdb865807abc990181efb8ebfa9122955cce2
ZIG_VERSION=0.16.0
```

The Ghostty commit is the one herdr-core 0.1.19 vendors, so panes parse output the way core's own screens do. Bump
both together, record it in `CHANGELOG.md`, rerun the bootstrap, and run the full suite. When core moves to a newer
Ghostty, follow it.

## How corgi-vt builds

`crates/corgi-vt/build.rs`:

1. Finds the Ghostty source: `GHOSTTY_SRC`, else `vendor/ghostty`. It panics with "run scripts/bootstrap.sh" when
   that's missing.
2. Finds Zig: `ZIG`, else `.tools/zig/zig`, else `zig` on `PATH`.
3. Runs `zig build -Demit-lib-vt -Demit-xcframework=false -Doptimize=ReleaseFast -Dcpu=baseline` with its prefix and
   cache in `OUT_DIR`. Baseline CPU, so release binaries run on machines other than the builder.
4. Copies only the static archive (`libghostty-vt.a`, or `ghostty-vt-static.lib` on Windows) to a directory of its
   own and links it statically, so the linker can't pick the dylib.
5. Runs bindgen on `include/ghostty/vt.h` with `-DGHOSTTY_STATIC`, keeping `ghostty_*` functions, `Ghostty*` types
   and `GHOSTTY_*` constants, into `$OUT_DIR/sys.rs`.

It reruns when Ghostty's `src`, `build.zig`, `build.zig.zon` or `include` change, or `ZIG` / `GHOSTTY_SRC` /
`CORGI_ZIG_CACHE` do. The first build of `corgi-vt` takes a while (minutes on a slow machine); later builds reuse
`OUT_DIR`. Zig's own build cache is in `OUT_DIR`, or in `CORGI_ZIG_CACHE` when that's set: CI keeps that folder
between runs, so a fresh build folder gets Ghostty's library from it in seconds (42 s → 5 s here).

Dependencies of the workspace are built optimised even in debug builds (`[profile.dev.package."*"] opt-level = 3`),
because ratatui's diff, crossterm and tokio run every frame. Release builds use thin LTO, 16 codegen units and
stripped symbols.

## just recipes

From the `justfile`:

| Recipe | Runs |
|---|---|
| `just` / `just check` | `lint` then `test`: everything CI runs. Run it before proposing a commit. |
| `just bootstrap` | `./scripts/bootstrap.sh` |
| `just fmt` | `cargo fmt --all` |
| `just lint` | `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` |
| `just test` | `cargo test --workspace` |
| `just run ARGS…` | `cargo run -p corgi-tui -- ARGS…` (e.g. `just run attach t3`, `just run doctor`) |
| `just bench` | `cargo run --release -p corgi-vt --example bench` |

`default-members` is `crates/corgi`, so plain `cargo run` and `cargo build` act on the app.

## Lints and dependency policy

- Clippy runs with `-D warnings`; the workspace also warns on `dbg_macro`, `todo`, `unimplemented` and
  `redundant_clone` (`[workspace.lints.clippy]`).
- `rustfmt.toml` sets `max_width = 120`.
- `deny.toml` (cargo-deny, run in CI): yanked crates denied; licenses limited to Apache-2.0, MIT, BSD-2/3-Clause,
  ISC, Unicode-3.0, Zlib, MPL-2.0; unknown registries and git sources denied; duplicate versions warned.

## CI

`.github/workflows/ci.yml` runs on Linux and macOS for every push to `main` and every pull request: it installs
Rust (latest stable) and Zig 0.16.0, fetches Ghostty with `SKIP_ZIG=1 ./scripts/bootstrap.sh`, installs `hc`, then
fmt, clippy and tests with `ZIG=zig`, keeping Zig's build cache (`CORGI_ZIG_CACHE`) and Rust's dependencies
(`Swatinem/rust-cache`) between runs. A separate job runs cargo-deny.

`.github/workflows/release.yml` builds the release binaries (Apple Silicon macOS, Linux x86_64 and ARM, the Linux
ones on Ubuntu 22.04) on a `v*` tag, making a GitHub release with them; started by hand, it only builds, and keeps
them as the run's artifacts. See [release](release.md).

## Cross-compiling

`build.rs` passes `-Dtarget=<arch>-<os>[-<abi>]` to Zig when Cargo's target differs from the host (or targets
Windows), translating the triple in `zig_target` (`apple` → `macos`, `windows`, else `linux`; `musl`, `gnu` and
`msvc` kept). Zig cross-compiles libghostty-vt itself; you still need the Rust target
(`rustup target add …`) and a linker for it. No cross build is set up or tested in CI yet.

## Windows notes

Windows isn't supported yet (M9):

- The core transport is `hc connect` only; `--socket` is Unix-only and a named pipe isn't implemented.
- DEC 9001 (win32-input) key encoding isn't implemented.
- The e2e tests are Unix only, and CI skips installing `hc` on Windows, so the integration tests skip there.
- Paths follow Windows conventions already (`paths.rs`: `%LOCALAPPDATA%\corgi` for state, `%APPDATA%\corgi` for
  config), and `build.rs` links `ghostty-vt-static.lib`.
- `bootstrap.sh` doesn't run on Windows; install Zig 0.16.0 yourself, fetch Ghostty with `SKIP_ZIG=1` from a POSIX
  shell, and set `ZIG`.
