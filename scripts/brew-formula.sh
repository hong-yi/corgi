#!/bin/sh
# Print the Homebrew formula for a corgi release: scripts/brew-formula.sh VERSION DIST [REPO]
#
# DIST holds the release's corgi-VERSION-TARGET.tar.gz.sha256 files (the release workflow's
# build artifacts); REPO is the GitHub repo the release is on (default hong-yi/corgi). The
# release workflow runs this and pushes the result to hong-yi/homebrew-tap as Formula/corgi.rb.
set -eu
version=$1
dist=$2
repo=${3:-hong-yi/corgi}

sum() {
  f="$dist/corgi-$version-$1.tar.gz.sha256"
  [ -f "$f" ] || { echo "missing $f" >&2; exit 1; }
  cut -d' ' -f1 "$f"
}
url() { echo "https://github.com/$repo/releases/download/v$version/corgi-$version-$1.tar.gz"; }

mac_arm=aarch64-apple-darwin
linux_x86=x86_64-unknown-linux-gnu
linux_arm=aarch64-unknown-linux-gnu
# Read first, outside the heredoc, so a missing checksum stops the script.
mac_arm_sum=$(sum $mac_arm)
linux_x86_sum=$(sum $linux_x86)
linux_arm_sum=$(sum $linux_arm)

cat <<EOF
# typed: strict
# frozen_string_literal: true

# Written by corgi's release workflow (scripts/brew-formula.sh in $repo): edits here are replaced
# by the next release.
class Corgi < Formula
  desc "Tmux-style terminal UI for herdr-core"
  homepage "https://github.com/$repo"
  version "$version"
  license "MIT"

  on_macos do
    # Apple Silicon only: there's no Intel Mac build.
    depends_on arch: :arm64

    on_arm do
      url "$(url $mac_arm)"
      sha256 "$mac_arm_sum"
    end
  end

  on_linux do
    on_intel do
      url "$(url $linux_x86)"
      sha256 "$linux_x86_sum"
    end
    on_arm do
      url "$(url $linux_arm)"
      sha256 "$linux_arm_sum"
    end
  end

  def install
    bin.install "corgi"
  end

  def caveats
    <<~EOS
      corgi runs on herdr-core (\`hc\`). If \`hc\` isn't installed yet:
        curl -fsSL https://releases.herdr.dev/core/install.sh | sh
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/corgi --version")
  end
end
EOF
