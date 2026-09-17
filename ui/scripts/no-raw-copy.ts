#!/usr/bin/env bun
// Guardrail: fail when user-visible copy appears as raw literals in ui/src
// instead of paraglide messages (m.key() from lib/i18n).
//
// Excluded (may contain raw text):
//   - src/paraglide/**        (generated)
//   - src/lib/generated/**    (generated API types)
//   - src/lib/i18n.ts         (re-export point)
//   - src/lib/format.ts       (known debt: units like "ms" / date glue)
//   - *.test.ts(x)            (test fixtures are copy, not copy surfaces)
//   - src/lib/paraglide-runtime-shim.js (generated-code stand-in)
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const ROOT = new URL("../src", import.meta.url).pathname;
const EXCLUDE = [
  /src\/paraglide\//,
  /src\/lib\/generated\//,
  /src\/lib\/i18n\.ts$/,
  /src\/lib\/format\.ts$/,
  /src\/lib\/paraglide-runtime-shim\.js$/,
  /\.test\.(ts|tsx)$/,
];

// Pragmatic patterns for the common cases:
//  1. JSX text:  >Text here<   (2+ chars, starting uppercase/lowercase letter)
//  2. label="…", title="…", placeholder="…", aria-label="…", data-tip="…"
const TEXT_RE = /::?text|format\(/; // skip files doing heavy string work
const JSX_TEXT = />([^<>{}]*[A-Za-z][^<>{}]*)</g;
const ATTR = /\b(label|title|placeholder|aria-label|data-tip)="([^"{}]*[A-Za-z][^"{}]*)"/g;

function* walk(dir) {
  for (const e of readdirSync(dir)) {
    const p = join(dir, e);
    if (statSync(p).isDirectory()) yield* walk(p);
    else yield p;
  }
}

/** Heuristic strips: lines that look like code/comments rather than markup
 * (the JSX-text regex has no parser, so it matches across statements like
 * `useState<T>(null);` or block comments). Match only when the captured
 * text looks like a sentence-ish copy fragment on a markup-ish line. */
function looksLikeCode(text, line) {
  if (line.includes("//") || line.includes("*") || line.includes("/*")) return true;
  if (/(=>|\{|\}|;|\(|\)|=|\bconst\b|\breturn\b|\bexport\b)/.test(text)) return true;
  if (/^https?:\/\//.test(text)) return true;
  return false;
}

// Allowlist: intentional non-message copy / false positives.
const ALLOW_TEXT = new Set([
  "oxe", // logotype (brand name, never translated)
  "Promise", // TS type token misread by the text regex
  "fieldErrors[k] ?", // ternary misread by the text regex
]);
const ALLOW_ATTR = new Set(["main"]); // nav landmark label

let bad = 0;
for (const file of walk(ROOT)) {
  if (EXCLUDE.some((re) => re.test(file))) continue;
  if (!/\.(tsx|ts)$/.test(file)) continue;
  const src = readFileSync(file, "utf8");
  for (const [re, tag] of [
    [JSX_TEXT, "jsx-text"],
    [ATTR, "attr-literal"],
  ]) {
    for (const m of src.matchAll(re)) {
      const text = (m[2] ?? m[1] ?? "").trim();
      // Ignore pure symbols/punctuation, numbers, and single letters
      if (!/[A-Za-z]{2,}/.test(text)) continue;
      if ((tag === "jsx-text" ? ALLOW_TEXT : ALLOW_ATTR).has(text)) continue;
      const lineText = src.split("\n")[src.slice(0, m.index).split("\n").length - 1];
      if (looksLikeCode(text, lineText)) continue;
      const line = src.slice(0, m.index).split("\n").length;
      console.error(`${file}:${line}: raw ${tag}: "${text.slice(0, 60)}"`);
      bad++;
    }
  }
}
if (bad > 0) {
  console.error(
    `\nno-raw-copy: ${bad} violation(s). Move copy to messages/en.json and use m.key() from lib/i18n.`,
  );
  process.exit(1);
}
console.log("no-raw-copy: OK");
