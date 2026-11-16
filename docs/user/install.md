# Install corgi

How to install corgi and herdr-core (`hc`), and how to check that they work together.

corgi is a viewer for herdr-core. It needs `hc` on your `PATH`: corgi runs `hc connect` to reach core, and that starts core if it isn't running yet.

## Platforms

| Platform | Status |
|---|---|
| macOS, Apple Silicon | Homebrew, release builds, or from source. |
| macOS, Intel | From source only: there's no release build. |
| Linux (x86_64, arm64) | Homebrew, release builds (glibc 2.35 or newer, e.g. Ubuntu 22.04), or from source. |
| Windows | Not supported yet. The named-pipe transport and Windows key encoding are still to do. |

There's no installer script yet that also installs `hc`: install herdr-core first, as below.

## Install herdr-core

Install `hc` with herdr-core's installer:

```sh
curl -fsSL https://releases.herdr.dev/core/install.sh | sh
```

corgi needs core **0.1.19 or newer**. Check your version:

```sh
hc --version
```

Update core with:

```sh
hc update
```

`hc update` hands the running core over to the new version. Your terminals keep running, and corgi reconnects on its own.

If `hc` is missing, corgi stops before it takes the screen and tells you how to install it:

```
corgi: can't reach herdr-core: `hc` not found; install herdr-core: curl -fsSL https://releases.herdr.dev/core/install.sh | sh
```

If core is too old, corgi says so:

```
corgi: core 0.1.12 is too old; corgi needs 0.1.19 or newer (run `hc update`)
```

## Install corgi with Homebrew

```sh
brew install hong-yi/tap/corgi
```

Update with `brew upgrade corgi`. The formula installs only corgi; its notes remind you how to install `hc`.

## Install a release build

Each [release](https://github.com/hong-yi/corgi/releases) has a `corgi-VERSION-TARGET.tar.gz` for Apple Silicon
macOS (`aarch64-apple-darwin`) and Linux (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`), with a
`.sha256` beside each. Check it, unpack it, and put `corgi` on your `PATH`:

```sh
shasum -a 256 -c corgi-0.1.0-aarch64-apple-darwin.tar.gz.sha256
tar xzf corgi-0.1.0-aarch64-apple-darwin.tar.gz
install corgi-0.1.0-aarch64-apple-darwin/corgi ~/.local/bin/
```

To update, download the new release and do the same.

## Build and install corgi from source

You need:

- Rust stable, 1.88 or newer (the repository's `rust-toolchain.toml` picks stable for you with rustup)
- libclang: Xcode or the Command Line Tools on macOS, `libclang-dev` on Linux
- `git`, `curl`, `python3` and `shasum`, for the bootstrap script

From a checkout of the corgi repository:

```sh
./scripts/bootstrap.sh
cargo install --path crates/corgi --locked
```

`bootstrap.sh` downloads the pinned Zig into `.tools/zig` and the pinned Ghostty source into `vendor/ghostty`. corgi's terminal emulator is built from them. The script installs nothing outside the repository. If you use `just`, `just bootstrap` runs the same script.

`cargo install` puts the `corgi` binary in `~/.cargo/bin`. Make sure that folder is on your `PATH`.

### Update corgi

Pull the new source, then run both steps again:

```sh
./scripts/bootstrap.sh
cargo install --path crates/corgi --locked
```

The bootstrap step only downloads something when the pins in `scripts/pins.env` have changed.

Restart corgi to use the new build. Your terminals live in core, so they keep running while you do this: detach with `C-b d`, then start `corgi` again.

## Check the install

`corgi --version` prints corgi's own version:

```sh
$ corgi --version
corgi 0.1.0
```

It doesn't show the core version yet. To check the whole chain, run `corgi doctor`:

```sh
$ corgi doctor
corgi 0.1.0
core   ✓ herdr-core 0.1.19 on "laptop" (session default, protocol generation 1)
       features: …
       3 live terminal(s), 0 other machine(s)
layout doc "corgi": version 12
corgi in core terminals: none
```

What each line tells you:

- `core`: whether corgi reached core, and core's version, machine name and session. A `✗` here gives the reason, for example a missing `hc` or a core that is too old.
- `features`: what this core supports.
- `live terminal(s)`: the terminals core is running now.
- `layout doc "corgi"`: corgi's saved layout. `not written yet` is normal before your first session.
- `corgi in core terminals`: copies of corgi running inside core terminals. A `✗` here means one runs inside its own pane. `corgi doctor` exits with status 1 in that case. See [Troubleshooting](troubleshooting.md).

`corgi doctor` connects the same way corgi does, so it starts core if it isn't running.

## Optional: a config file

corgi works without a config file. To start one with every default written out:

```sh
mkdir -p ~/.config/corgi
corgi --default-config > ~/.config/corgi/config.toml
```

corgi reads `$XDG_CONFIG_HOME/corgi/config.toml` (usually `~/.config/corgi/config.toml`, on macOS too). `--config <file>` or `CORGI_CONFIG` picks another file. See [Configuration](configuration.md).

## Next

- [Your first session](first-session.md)
- [Concepts](concepts.md)
