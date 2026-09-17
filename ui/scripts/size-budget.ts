import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { gzipSync } from "node:zlib";

// Size budget check: built JS gzip <= 150KB, CSS gzip <= 60KB.
// Dynamic band: aim ~100KB JS, recalibrate when we approach 150KB
// (see ui/AGENTS.md "Hard constraints" + .agents/plans/2026-09-17-v0.5.0-plan.md
// for the derivation: localhost SPA on desktop V8 parses 100KB gz in
// ~30-60ms, well under the 100ms RAIL "feels immediate" threshold).
// Bumping to 150KB ceiling from the v0.4.0 45KB cap to make room for
// v0.5.0 multi-turn chat, Zag headless primitives, citation popovers,
// source rail, and tools toggle without per-feature bundling fights.
const JS_BUDGET = 150 * 1024;  // ceiling; recalibrate when approaching
const JS_TARGET = 100 * 1024;  // aim point; soft target, not enforced
const CSS_BUDGET = 60 * 1024;

const dist = new URL("../dist/assets", import.meta.url).pathname;

let jsTotal = 0;
let cssTotal = 0;

for (const f of readdirSync(dist)) {
  const gz = gzipSync(readFileSync(join(dist, f))).length;
  if (f.endsWith(".js")) jsTotal += gz;
  else if (f.endsWith(".css")) cssTotal += gz;
}

const kb = (n: number) => `${(n / 1024).toFixed(1)}KB`;
console.log(`JS gzip total: ${kb(jsTotal)} / ceiling ${kb(JS_BUDGET)} (target ${kb(JS_TARGET)})`);
console.log(`CSS gzip total: ${kb(cssTotal)} / budget ${kb(CSS_BUDGET)}`);

let fail = false;
const RECALIBRATE = 130 * 1024; // warn band: recalibrate when crossing this
if (jsTotal > JS_BUDGET) {
  console.error(`FAIL: JS over ceiling ${kb(JS_BUDGET)}`);
  fail = true;
} else if (jsTotal > RECALIBRATE) {
  console.error(
    `WARN: JS in recalibrate band (>${kb(RECALIBRATE)}). ` +
      `Review ui/AGENTS.md budget; bump ceiling with rationale or trim bundle.`,
  );
}
if (cssTotal > CSS_BUDGET) {
  console.error(`FAIL: CSS over budget`);
  fail = true;
}
process.exit(fail ? 1 : 0);
