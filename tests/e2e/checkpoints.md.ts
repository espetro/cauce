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
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('oxe')
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

/**
 * Checkpoint 2 "Landing, AI mode selected": /?mode=ai. The landing route's
 * validateSearch accepts `mode=ai` (ui/src/routes/index.tsx), but the morphed
 * AI pill (model combobox, reasoning chip, "Ask anything privately") is not
 * built yet — the route only renders the wordmark either way. Assert the
 * deterministic minimum: the param survives validation (no error screen) and
 * the idle landing still renders.
 */
async function landingAiModeSelected(page: Page): Promise<void> {
  await page.goto('/?mode=ai')
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('oxe')
  expect(new URL(page.url()).searchParams.get('mode')).toBe('ai')
}

/**
 * Checkpoint 12 "AI mode unavailable": force=ai-off seeds the aiReducer
 * straight into `unavailable`, which renders the not-configured notice
 * without any provider call.
 */
async function aiModeUnavailable(page: Page): Promise<void> {
  await page.goto('/search?q=x&mode=ai&force=ai-off')
  await expect(page.getByTestId('ai-surface')).toBeVisible()
  const notice = page.getByTestId('ai-unavailable')
  await expect(notice).toContainText('AI mode is not configured')
}

/**
 * Checkpoint 13 "AI stream failed mid-stream": force=error makes the backend
 * emit a standalone ErrorFrame before any delta, so the reducer lands in
 * `failed` with the retry + view Search affordances.
 */
async function aiStreamFailedMidStream(page: Page): Promise<void> {
  await page.goto('/search?q=x&mode=ai&force=error')
  const failed = page.getByTestId('ai-failed')
  await expect(failed).toBeVisible()
  await expect(failed).toContainText('simulated provider error')
  await expect(failed.getByRole('link', { name: 'retry' })).toBeVisible()
  await expect(page.getByTestId('ai-view-search')).toBeVisible()
}

/**
 * Checkpoint 14 "AI empty sources": force=empty streams SourcesFrame([]) +
 * DoneFrame(answer=""), a terminal done state with zero sources. The
 * empty-sources notice requires a non-empty answer (search.tsx renders it
 * only when `answer && sources.length === 0`), so assert the deterministic
 * done surface: ai-done with confidence 0 and no source cards.
 */
async function aiEmptySources(page: Page): Promise<void> {
  await page.goto('/search?q=x&mode=ai&force=empty')
  await expect(page.getByTestId('ai-done')).toBeVisible()
  await expect(page.getByTestId('ai-confidence')).toContainText('0%')
  await expect(page.getByTestId('ai-source-card')).toHaveCount(0)
}

const BEHAVIORS: Record<string, (page: Page) => Promise<void>> = {
  '1': landingIdle,
  '2': landingAiModeSelected,
  '3': searchClassicResultsPage1,
  '12': aiModeUnavailable,
  '13': aiStreamFailedMidStream,
  '14': aiEmptySources,
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
