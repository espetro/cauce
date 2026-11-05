#!/bin/sh
# Install the latest cauce release binary.
#
#   curl -fsSL https://raw.githubusercontent.com/espetro/cauce/main/install.sh | sh
#
# Environment:
#   CAUCE_VERSION      release tag to install (default: latest), e.g. v0.9.1
#   CAUCE_INSTALL_DIR  target directory (default: ~/.local/bin)
set -eu

repo="espetro/cauce"
dir="${CAUCE_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '%s\n' "$*" >&2; }
die() { say "install.sh: $*"; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1"; }

need curl
need tar
need uname

case "$(uname -s)" in
  Linux) os=linux ;;
  Darwin) os=macos ;;
  *) die "unsupported OS: $(uname -s) (see https://github.com/$repo#install-it)" ;;
esac

case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  arm64 | aarch64) arch=aarch64 ;;
  *) die "unsupported architecture: $(uname -m)" ;;
esac

case "$os-$arch" in
  linux-x86_64) target=x86_64-unknown-linux-musl ;;
  linux-aarch64) target=aarch64-unknown-linux-musl ;;
  macos-x86_64) target=x86_64-apple-darwin ;;
  macos-aarch64) target=aarch64-apple-darwin ;;
esac

tag="${CAUCE_VERSION:-}"
if [ -z "$tag" ]; then
  # /releases/latest redirects to /releases/tag/<tag>; no API token or rate limit.
  tag="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest")"
  tag="${tag##*/}"
fi
[ -n "$tag" ] || die "could not resolve the latest release"

asset="cauce-$tag-$target.tar.gz"
base="https://github.com/$repo/releases/download/$tag"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading $asset"
curl -fsSL "$base/$asset" -o "$tmp/$asset" || die "download failed: $base/$asset"

# Verify against the release's SHA256SUMS.txt when a hashing tool is present.
if curl -fsSL "$base/SHA256SUMS.txt" -o "$tmp/SHA256SUMS.txt" 2>/dev/null; then
  want="$(grep " $asset\$" "$tmp/SHA256SUMS.txt" | cut -d' ' -f1)"
  if command -v sha256sum >/dev/null 2>&1; then
    got="$(sha256sum "$tmp/$asset" | cut -d' ' -f1)"
  elif command -v shasum >/dev/null 2>&1; then
    got="$(shasum -a 256 "$tmp/$asset" | cut -d' ' -f1)"
  else
    got=""
  fi
  if [ -n "$want" ] && [ -n "$got" ] && [ "$want" != "$got" ]; then
    die "checksum mismatch for $asset"
  fi
fi

tar -xzf "$tmp/$asset" -C "$tmp" cauce
mkdir -p "$dir"
install -m 0755 "$tmp/cauce" "$dir/cauce"
[ "$os" = macos ] && xattr -d com.apple.quarantine "$dir/cauce" 2>/dev/null || true

say "installed cauce $tag to $dir/cauce"
case ":$PATH:" in
  *":$dir:"*) ;;
  *) say "add $dir to your PATH to run it" ;;
esac
say "run: cauce serve   # UI + API + MCP on http://127.0.0.1:4479"
