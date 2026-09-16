// Size budget check: built JS gzip <= 45KB, CSS gzip <= 35KB.
// Budgets are set ~5-10KB above current usage (39.1 / 21.0) so the next
// features have headroom without letting regressions slide.
const JS_BUDGET = 45 * 1024;
const CSS_BUDGET = 35 * 1024;

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
