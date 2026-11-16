#!/bin/sh
# Fetch the pinned Zig into .tools/zig and Ghostty into vendor/ghostty.
# Touches nothing outside the repo.
set -eu
cd "$(dirname "$0")/.."
. scripts/pins.env

# CI (or anyone) with the pinned Zig already on PATH can skip the download: SKIP_ZIG=1.
if [ "${SKIP_ZIG:-}" = 1 ]; then
  echo "zig $(zig version) (from PATH)"
elif [ ! -x .tools/zig/zig ] || [ "$(.tools/zig/zig version)" != "$ZIG_VERSION" ]; then
  os=$(uname -s); arch=$(uname -m)
  case "$os" in Darwin) os=macos ;; Linux) os=linux ;; *) echo "unsupported OS $os" >&2; exit 1 ;; esac
  case "$arch" in arm64|aarch64) arch=aarch64 ;; x86_64|amd64) arch=x86_64 ;; *) echo "unsupported arch $arch" >&2; exit 1 ;; esac
  key="$arch-$os"
  meta=$(curl -fsSL https://ziglang.org/download/index.json | python3 -c "
import json,sys; d=json.load(sys.stdin)['$ZIG_VERSION']['$key']; print(d['tarball'], d['shasum'])")
  url=${meta% *}; sum=${meta#* }
  rm -rf .tools/zig .tools/dl && mkdir -p .tools/dl
  curl -fsSLo .tools/dl/zig.tar.xz "$url"
  echo "$sum  .tools/dl/zig.tar.xz" | shasum -a 256 -c -
  tar -xJf .tools/dl/zig.tar.xz -C .tools/dl
  mv .tools/dl/zig-*/ .tools/zig && rm -rf .tools/dl
  echo "zig $(.tools/zig/zig version)"
else
  echo "zig $(.tools/zig/zig version)"
fi

if [ "$(git -C vendor/ghostty rev-parse HEAD 2>/dev/null || true)" != "$GHOSTTY_COMMIT" ]; then
  mkdir -p vendor/ghostty
  git -C vendor/ghostty init -q
  git -C vendor/ghostty remote add origin https://github.com/ghostty-org/ghostty 2>/dev/null || true
  git -C vendor/ghostty fetch -q --depth 1 origin "$GHOSTTY_COMMIT"
  git -C vendor/ghostty checkout -q FETCH_HEAD
fi
echo "ghostty $(git -C vendor/ghostty rev-parse --short HEAD)"
