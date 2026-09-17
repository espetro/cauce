import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { gzipSync } from "node:zlib";

// Size budget check: built JS gzip <= 200KB, CSS gzip <= 60KB.
// Ceiling and target are set per the v0.5.0 archive-rebuild plan
// (.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md, day-0 gate 8):
// "Wave 1 builds a hello-world of the real stack and records the actual
// gz floor. Ceiling 200KB gz, target derived from the measured floor."
//
// Measured floor (2026-09-17, one TanStack Router route + one Lingui
// message, React 19 + SWC React Compiler): 98.82KB gz JS, 0.81KB gz CSS.
// React 19's compiler runtime + router + Lingui runtime cost more at
// zero features than the old Preact skeleton (~37KB gz full app), which
// is the tradeoff the framework-migration research already accepted.
const JS_BUDGET = 200 * 1024; // ceiling
const JS_TARGET = 130 * 1024; // aim point; soft target, not enforced
const RECALIBRATE = 170 * 1024; // warn band: recalibrate when crossing this
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
