// Size budget check: built JS gzip <= 40KB, CSS gzip <= 30KB.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { gzipSync } from "node:zlib";

const dist = new URL("../dist/assets", import.meta.url).pathname;

const JS_BUDGET = 40 * 1024;
const CSS_BUDGET = 30 * 1024;

let jsTotal = 0;
let cssTotal = 0;

for (const f of readdirSync(dist)) {
  const gz = gzipSync(readFileSync(join(dist, f))).length;
  if (f.endsWith(".js")) jsTotal += gz;
  else if (f.endsWith(".css")) cssTotal += gz;
}

const kb = (n: number) => `${(n / 1024).toFixed(1)}KB`;
console.log(`JS gzip total: ${kb(jsTotal)} / budget ${kb(JS_BUDGET)}`);
console.log(`CSS gzip total: ${kb(cssTotal)} / budget ${kb(CSS_BUDGET)}`);

let fail = false;
if (jsTotal > JS_BUDGET) {
  console.error(`FAIL: JS over budget`);
  fail = true;
}
if (cssTotal > CSS_BUDGET) {
  console.error(`FAIL: CSS over budget`);
  fail = true;
}
process.exit(fail ? 1 : 0);
