/**
 * Markdown checkpoint adapter: one Playwright test per tests/e2e/<n>-<slug>.md.
 *
 * The behaviors live in the .md files ("## Behavior under test"); this adapter
 * only parses each file's `status:` frontmatter (skip | active) and dispatches
 * to the implemented test body for that checkpoint number. Files marked
 * `status: skip` emit a `test.skip()` — visible in the report as skipped, not
 * silently absent. Unskipping a file without wiring its behavior here fails
 * fast via tests/test_e2e_adapter_parity.py.
 */
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test, type Page } from '../../ui/node_modules/@playwright/test'
// Playwright runs with cwd = ui/ (where the config lives).
// This file is transpiled to CJS by Playwright's loader; import.meta is
// unavailable. Resolve from __dirname when present (transpiled), else cwd.
const E2E_DIR = join(__dirname)

interface Checkpoint {
  file: string
  stem: string
  status: 'skip' | 'active'
  number: string
}

function parseStatus(content: string): 'skip' | 'active' {
  const match = /^status:\s*(skip|active)\s*$/m.exec(content)
  if (!match) {
    throw new Error(`missing/invalid 'status:' frontmatter (skip|active)`)
  }
  return match[1] as 'skip' | 'active'
}

function parseNumber(stem: string): string {
  const m = /^(\d+)-/.exec(stem)
  if (!m) throw new Error(`${stem}: filename must start with '<n>-'`)
  return m[1]
}

function loadCheckpoints(): Checkpoint[] {
  const out: Checkpoint[] = []
  for (const entry of readdirSync(E2E_DIR)) {
    if (!entry.endsWith('.md')) continue
    const content = readFileSync(join(E2E_DIR, entry), 'utf8')
    out.push({
      file: entry,
      stem: entry.replace(/\.md$/, ''),
      status: parseStatus(content),
      number: parseNumber(entry.replace(/\.md$/, '')),
    })
  }
  return out
}

/* ------------------------------------------------------------------ */
/* Active checkpoint behaviors                                         */
/* ------------------------------------------------------------------ */

/** Checkpoint 1 "Landing idle": / renders the idle landing headline. */
async function landingIdle(page: Page): Promise<void> {
  await page.goto('/')
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Welcome to oxe')
}

/**
 * Checkpoint 3 "Search classic, results page 1": /search?q=... renders the
 * results list with a positive results count against the real backend.
 */
async function searchClassicResultsPage1(page: Page): Promise<void> {
  await page.goto('/search?q=playwright')
  const meta = page.getByTestId('results-meta')
  await expect(meta).toBeVisible()
  const text = (await meta.textContent()) ?? ''
  expect(parseInt(text, 10)).toBeGreaterThan(0)
}

const BEHAVIORS: Record<string, (page: Page) => Promise<void>> = {
  '1': landingIdle,
  '3': searchClassicResultsPage1,
}

/* ------------------------------------------------------------------ */
/* Test generation                                                     */
/* ------------------------------------------------------------------ */

for (const cp of loadCheckpoints()) {
  const behavior = BEHAVIORS[cp.number]
  test(`checkpoint ${cp.stem}`, async ({ page }) => {
    if (cp.status === 'skip') {
      test.skip(true, `placeholder scaffold (see ${cp.file})`)
    }
    expect(behavior, `${cp.file} is active but has no behavior in checkpoints.md.ts`).toBeDefined()
    await behavior(page)
  })
}
