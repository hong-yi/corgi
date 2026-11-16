# Contributing to corgi

How work is branched, committed, checked and merged, and the code rules the project keeps.

Start with [docs/dev/architecture.md](docs/dev/architecture.md), then [docs/dev/building.md](docs/dev/building.md)
to get a build, and [docs/dev/testing.md](docs/dev/testing.md) before writing tests. `docs/PLAN.md` says what's
done and what's open.

## Branches

- One branch per piece of work, named for it: `m8-remote`, `m9-perf`, `m9-docs`.
- Branch from `main`. Don't commit on `main` directly.
- `main` moves only by fast-forward merges of finished branches, when the maintainer says so. Rebase your branch
  on `main` first if it has moved; no merge commits.

## Commits

One logical change per commit, with a subject in the style the history uses:

```
<type>[(scope)]: <what it does, lowercase, no full stop>
```

- Types: `feat`, `fix`, `perf`, `test`, `docs`, `chore`. A scope names a crate when it helps:
  `feat(hc-proto): herdr-core client (M1)`.
- Say what changed for someone using or reading corgi: `fix: workspace names follow the folder its first pane is
  in`, `perf: credit output once it's used; bounded draining per frame`.
- A body, when needed, says why and anything surprising.
- Commits written with Claude end with the trailer:

  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  ```

## Before every commit

1. `just check`: rustfmt check, clippy with warnings denied, and every test.
2. For anything that touches the app (`crates/corgi`), run the whole suite three times
   (`cargo test --workspace`), and read each result. The e2e tests drive a real corgi and core, and a flaky one
   shows up only on repeat runs.
3. Check for leftovers after e2e runs: `hc hold`, `hc-launchd` or `target/debug/corgi` processes whose working
   directory is under `target/tmp/` (kill only those), and `git worktree list` showing only the main checkout.
4. Then commit, as a separate step. Never chain the test run and the commit in one command: twice a commit went
   in red that way (`8ab6f07`, `f74ac77`).

`hc` must be installed for the integration and e2e tests; without it they skip, which proves nothing.

## Code rules

- Format with rustfmt, `max_width = 120` (`rustfmt.toml`). `just fmt` fixes it.
- Clippy is clean with `-D warnings`. No `dbg!`, `todo!` or `unimplemented!`.
- Keep deciding code separate from IO. Layout, model, keymap, commands, forms, usage placement and similar
  decisions live in pure modules with unit tests; the app gathers inputs and does the IO. See
  [architecture.md](docs/dev/architecture.md#deciding-code-stays-separate-from-io).
- Comments are plain prose: say what something is and why, in full sentences. No filler, no restating the code,
  no banners. Every module starts with a `//!` paragraph on what it's for.
- `unsafe` stays in `corgi-vt` (the libghostty FFI) and small libc calls (`display.rs`, `nesting.rs`, `process.rs`,
  `usage_sources.rs`, the e2e harness), each with a `// SAFETY:` comment.
- Every dialog is drawn over the whole window and placed with `ui::centered`.
- Every new UI state is a `Mode` variant with its keys in `app/keyboard.rs`, its drawing in `app/draw.rs`, and a
  badge (`mode_badge`, `ModeLabels`, `default-config.toml`).
- New commands go in `commands.rs` (`Action`, `parse`, `NAMES`, a test), are handled in `app/actions.rs::run`, and
  get a key hint in the presets. See [input-and-keymaps.md](docs/dev/input-and-keymaps.md#adding-an-action).
- New config keys get a default, an entry in `default-config.toml` (a test checks the two match), and validation
  with a message naming the key.
- Tests never touch the user's core, config or git checkout. See [testing.md](docs/dev/testing.md).

### File size

Keep source files under about 32 KB (`docs/PLAN.md` §2). When a module grows past that, split it by feature, as
`app.rs` became `app/{mod,actions,draw,…}.rs`. Several files are over it now (`app/mod.rs`, `app/draw.rs`,
`usage_sources.rs`, `corgi-vt/src/lib.rs`, `config.rs`, `app/actions.rs`, `app/dialogs.rs`): split before adding
much to them. The e2e test file is exempt.

## Docs

- Update `docs/PLAN.md` when a task is done or moves.
- Add a line to `CHANGELOG.md` under `Unreleased` for anything a user would notice
  (see [docs/dev/release.md](docs/dev/release.md#the-changelog)).
- Developer docs live in `docs/dev/`, decisions in `docs/dev/adr/`, one file per decision.
