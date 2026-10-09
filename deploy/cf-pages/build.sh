#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

# CF Pages build for the cauce SPA (docs/deploy/cf-pages.md). The web UI
# is fully same-origin — every runtime call is a relative /api/* fetch —
# so the Pages project is only the built assets plus the /api proxy
# functions in the repo-root `functions/` dir (Pages reads functions from
# the checkout, not the output dir).
#
# Pages project settings:
#   Build command: bash deploy/cf-pages/build.sh
#   Output dir:    pages-dist
#   Env var:       API_ORIGIN=https://<backend origin, e.g. api.cauce.fyi>

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

: "${API_ORIGIN:?set API_ORIGIN on the Pages project (e.g. https://api.cauce.fyi)}"

# The Pages image ships Node via nvm but no pnpm; corepack resolves the
# version pinned in package.json#packageManager.
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0
corepack enable 2>/dev/null || npm install -g corepack
corepack prepare \
  "$(node -p "require('./crates/cauce-server/package.json').packageManager")" \
  --activate

pnpm --dir crates/cauce-server install --frozen-lockfile
pnpm --dir crates/cauce-server build:spa

rm -rf pages-dist
mkdir -p pages-dist/app
cp -r crates/cauce-server/assets/spa/. pages-dist/app/

# `_redirects` carries static rules only — the API proxy lives in
# `functions/` because a 200-rewrite cannot target an external origin.
cat > pages-dist/_redirects <<'EOF'
/ /app/ 302
EOF

# No SPA-fallback rule is needed: without a top-level 404.html, Pages
# serves the root index.html for any unmatched path, which boots the
# app for deep links like /app/search?q=...
cp crates/cauce-server/assets/spa/index.html pages-dist/index.html

echo "pages-dist ready (API_ORIGIN=$API_ORIGIN)"
