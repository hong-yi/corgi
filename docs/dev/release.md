# Releasing

Versioning, the changelog, bumping the minimum core version, and what a release tag does (builds, the GitHub release, Homebrew).

## Where versions live

- corgi's version is the workspace's: `version = "0.1.0"` in the root `Cargo.toml`, shared by all three crates
  (`version.workspace = true`). Every crate is `publish = false`; nothing goes to crates.io.
- `corgi --version` prints it (clap's `version`). corgi also passes it to the terminals it starts as
  `TERM_PROGRAM_VERSION` (`App::spawn`), with `TERM_PROGRAM=corgi`.
- `hc_proto::GENERATION` (1) is the core protocol generation corgi speaks; `hc_proto::MIN_CORE` (`"0.1.19"`) is the
  oldest core it accepts (`crates/hc-proto/src/lib.rs`).
- `scripts/pins.env` pins Zig and the Ghostty commit (see [building.md](building.md#the-pins)).

## Versioning policy

`docs/PLAN.md` M9 lists a versioning policy as still to decide. This is the proposed one; change this section if
the decision differs. corgi follows semantic versioning, read for 0.x:

- Until 1.0, a minor bump (0.1 → 0.2) may change config keys, defaults, key bindings or the layout document in
  ways that need action from you. The changelog says what to do.
- A patch bump (0.1.0 → 0.1.1) is fixes and additions only: existing configs and layouts keep working.
- A new `MIN_CORE` is at least a minor bump: people must update `hc` first.
- From 1.0, breaking changes to the config, the CLI, the commands or the layout document need a major bump.

Bump the version in the root `Cargo.toml` only; `Cargo.lock` follows on the next build.

## The changelog

`CHANGELOG.md` at the repo root, newest first, in the Keep a Changelog shape:

```markdown
## [Unreleased]

### Added
- One line per change, in the user's words: what they can now do, and the key or command.

### Changed
### Fixed
### Removed

## [0.1.0] - 2026-12-01
```

- Write entries as you merge each piece of work, under `Unreleased`.
- Group by `Added`, `Changed`, `Fixed`, `Removed`; within a release, an area prefix (Layout, Keys, Sidebar, Usage,
  Remote, …) helps when the list is long.
- Say what someone must do: a renamed config key, a new minimum core, a changed default binding.
- Record toolchain and pin bumps (`scripts/pins.env`), as that file asks.
- At release, rename `Unreleased` to the version and date, and start a new empty `Unreleased`.

## Bumping the minimum core version

Raise `MIN_CORE` when corgi starts depending on something an older core lacks: a method, a feature, a field, or
a behaviour fix.

1. Change `MIN_CORE` in `crates/hc-proto/src/lib.rs`.
2. If corgi now needs a new method, add it to `USES` there, so `check` refuses a core without it at hello instead
   of failing later at the call.
3. Update `hello_gates_on_generation_version_and_methods` in the same file: it uses `"0.1.19"` as the accepted
   version and `"0.1.11"` as too old.
4. Check whether the Ghostty pin should follow the commit the new core vendors.
5. Install that core locally and run the whole suite three times. CI installs the latest `hc`, so CI alone won't
   catch a too-low minimum.
6. Note it in the changelog as something people must do: "needs herdr-core X.Y.Z or newer (`hc update`)".

The refusal message for an old core tells people to run `hc update` (`check` in `lib.rs`).

## Release checklist

1. `just check` passes, and the whole suite has passed three times on the release commit.
2. No leftover test processes or git worktrees (see [testing.md](testing.md#after-an-e2e-run)).
3. Version bumped in `Cargo.toml`; `CHANGELOG.md` has the version and date.
4. A fast-forward merge to `main`, then a tag on it: `git tag v0.1.0 && git push origin v0.1.0`.
5. Watch the `release` workflow; the release appears under the repo's Releases.

## What a tag does

`.github/workflows/release.yml`, on a pushed `v*` tag:

1. **build** (three runners at once): `aarch64-apple-darwin` (macos-15), `x86_64-unknown-linux-gnu`
   (ubuntu-22.04) and `aarch64-unknown-linux-gnu` (ubuntu-22.04-arm). Each checks the tag is the version in
   `Cargo.toml`, fetches the pinned Zig and Ghostty, builds `cargo build --release --locked -p corgi-tui`, runs
   `corgi --version`, and packs `corgi-VERSION-TARGET.tar.gz` (the binary, README, CHANGELOG, LICENSE) with a
   `.sha256` beside it.
2. **release**: a GitHub release named after the tag with the six files, its notes pointing at the changelog.
   A tag with a suffix (`v0.2.0-rc.1`) makes a prerelease.
3. **homebrew**: `scripts/brew-formula.sh` writes `Formula/corgi.rb` for this version from the `.sha256`
   files, and the job pushes it to [hong-yi/homebrew-tap](https://github.com/hong-yi/homebrew-tap). Skipped for
   prereleases, and off until the repository variable `HOMEBREW_TAP` is `on`.

Run the workflow by hand (Actions → release → Run workflow) to get the three builds as artifacts without a
release; they're named `corgi-dev-SHA-TARGET`.

Caches keep a release to a few minutes: the pinned Zig and Ghostty, Zig's build cache for Ghostty's library
(`CORGI_ZIG_CACHE`), and the Rust dependencies.

## Homebrew

`brew install hong-yi/tap/corgi` installs the binary for the machine (Apple Silicon Macs, Linux x86_64 and
arm64; no Intel Mac build). The formula's caveats say how to install `hc`, which isn't in Homebrew.

brew downloads release files without logging in, so the formula only works while `hong-yi/corgi` is public;
that's why the job is switched off. To switch it on, once the repo is public:

```sh
gh variable set HOMEBREW_TAP -b on -R hong-yi/corgi
```

The job pushes with a deploy key: its public half is on `homebrew-tap` (Settings → Deploy keys, "corgi release
workflow", write access to that repo only), its private half is the `HOMEBREW_TAP_DEPLOY_KEY` secret here. To
replace it, make a new key, add it to the tap with `gh repo deploy-key add KEY.pub -R hong-yi/homebrew-tap
--allow-write`, store it with `gh secret set HOMEBREW_TAP_DEPLOY_KEY -R hong-yi/corgi < KEY`, then delete the
old deploy key and the local files.

Check a formula change before a release: `./scripts/brew-formula.sh 0.1.0 DIR > corgi.rb && brew style
corgi.rb`, with `DIR` holding the three `.sha256` files (any contents of the right shape).

## Not set up yet

- Notices for the third-party code built into the binary (Ghostty, the Rust crates), shipped in the tarball.
- An install script that also installs `hc` when it's missing.
- Windows builds.

Until a release exists, install from a checkout: `cargo install --path crates/corgi --locked` (after `just
bootstrap`).
