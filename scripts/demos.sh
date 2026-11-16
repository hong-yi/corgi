#!/bin/sh
# Record the README's demos and make GIFs of them: docs/assets/demos/*.{cast,gif}.
# Needs hc and agg (`brew install agg`). Each demo runs on a core session of its own, with HOME a
# scratch folder, so nothing of yours shows in them.
set -e
cd "$(dirname "$0")/.."
cargo test -p corgi-tui --test demos -- --ignored --test-threads=1 "$@"
# catppuccin-mocha, as corgi draws itself on a dark terminal.
theme="1e1e2e,cdd6f4,45475a,f38ba8,a6e3a1,f9e2af,89b4fa,f5c2e7,94e2d5,bac2de,585b70,f38ba8,a6e3a1,f9e2af,89b4fa,f5c2e7,94e2d5,a6adc8"
for cast in docs/assets/demos/*.cast; do
    agg --theme "$theme" --font-size 14 --idle-time-limit 3 "$cast" "${cast%.cast}.gif" >/dev/null
    echo "made ${cast%.cast}.gif"
done
