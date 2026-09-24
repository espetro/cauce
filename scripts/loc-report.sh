#!/usr/bin/env bash
# W3-08: non-gating LOC/complexity report. Prints totals (impl vs test),
# the largest files and the largest functions, plus the warning
# thresholds. Never fails — it is a signal source, not a gate.
#
#   mise run loc
#
# Thresholds (warning only, documented in the W3-08 plan step):
#   file      > 1000 LOC  → review required
#   file      > 1500 LOC  → decomposition issue required unless justified
#   function  >  150 LOC  → consider extraction
#   test file >  800 LOC  → split helpers or convert repetition to
#                           tables/fixtures
set -uo pipefail
cd "$(dirname "$0")/.."

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Per-file code lines (non-blank, non-`//` comment; `//!` module docs and
# `/* */` blocks still count — the report is coarse by design).
find crates sdk tests -type f -name '*.rs' 2>/dev/null | while read -r f; do
    case "$f" in
        */tests/*|*/tests.rs|*/support/*) kind="test" ;;
        */examples/*|*/benches/*) kind="bench" ;;
        *) kind="impl" ;;
    esac
    code=$(grep -cvE '^\s*$|^\s*//' "$f" || true)
    printf '%s\t%s\t%s\n' "$code" "$kind" "$f"
done > "$tmp/files.tsv"

echo "=== totals (code lines, blank/line-comments excluded) ==="
awk -F'\t' '{tot[$2] += $1; n[$2]++}
END {
    for (k in tot) printf "%-6s %8d LOC  (%d files)\n", k, tot[k], n[k]
}' "$tmp/files.tsv" | sort

echo
echo "=== largest files ==="
sort -rn "$tmp/files.tsv" | head -15 | awk -F'\t' '{printf "%6d  %-5s  %s\n", $1, $2, $3}'
big=$(awk -F'\t' '$1 > 1500 {n1500++} $1 > 1000 {n1000++} END {printf "%d %d", n1000+0, n1500+0}' "$tmp/files.tsv")
read -r over1000 over1500 <<< "$big"

echo
echo "=== largest functions (impl files; brace-matched, coarse) ==="
find crates sdk -type f -name '*.rs' ! -path '*/tests/*' ! -path '*/support/*' | \
xargs awk '
/^[[:space:]]*(pub(\(crate\)|\(super\))? )?(async )?fn [A-Za-z_]/ {
    if (match($0, /fn [A-Za-z_][A-Za-z0-9_]*/)) {
        fname = substr($0, RSTART + 3, RLENGTH - 3)
        in_fn = 1; depth = 0; started = 0; flines = 0
    }
}
in_fn {
    flines++
    for (i = 1; i <= length($0); i++) {
        c = substr($0, i, 1)
        if (c == "{") { depth++; started = 1 }
        else if (c == "}") { depth-- }
    }
    if (started && depth <= 0) {
        if (flines > 80) printf "%5d  %s  (%s)\n", flines, fname, FILENAME
        in_fn = 0
    }
}' | sort -rn | head -15

echo
echo "=== thresholds ==="
echo "files >1000 LOC (review):    $over1000"
echo "files >1500 LOC (decompose): $over1500"
echo "run 'git diff --stat \$(git merge-base HEAD origin/main)..HEAD' for per-branch changed LOC"
exit 0
