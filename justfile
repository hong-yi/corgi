# corgi tasks. First time: `just bootstrap`.

default: check

# Fetch pinned Zig (.tools/zig) and Ghostty (vendor/ghostty).
bootstrap:
    ./scripts/bootstrap.sh

fmt:
    cargo fmt --all

lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

# Everything CI runs. Run before proposing a commit.
check: lint test

run *ARGS:
    cargo run -p corgi-tui -- {{ARGS}}

bench:
    cargo run --release -p corgi-vt --example bench

# Record the README's demos and make GIFs of them (needs agg).
demos *ARGS:
    ./scripts/demos.sh {{ARGS}}
