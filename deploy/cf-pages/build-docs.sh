#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

# Docs-only CF Pages build — for a dedicated docs Pages project (e.g.
# `cauce-docs` serving docs.cauce.fyi or the docs path). Emits just the
# docmd site; the main project keeps using build.sh (app+landing).
#
# Pages project settings:
#   Build command: bash deploy/cf-pages/build-docs.sh
#   Output dir:    docs-site
#
# The docmd output is relative-linked, so it also mounts under /docs/ on
# any domain unchanged.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# docmd needs Node 20+; the Pages image default can be older, so move
# to a newer toolchain via nvm when the image ships one.
node_major="$(node -p 'process.versions.node.split(".")[0]')"
if [ "$node_major" -lt 20 ]; then
  export NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
  # shellcheck disable=SC1091
  if [ -s "$NVM_DIR/nvm.sh" ]; then
    . "$NVM_DIR/nvm.sh"
    nvm install 20
  fi
fi
node -v

# Developer docs are canonical in .agents/docs; docmd doesn't follow
# symlinks and skips gitignored paths, so stage a real copy (the dir is
# deliberately not in .gitignore — see its comment). cp, not rsync —
# rsync is not in the Pages build image.
rm -rf docs/developers docs-site
mkdir -p docs/developers
cp -a .agents/docs/. docs/developers/

npx -y @docmd/core build

echo "docs-site ready"
