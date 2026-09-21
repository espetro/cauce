import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { defineConfig } from '@playwright/test'

// Throwaway cache dir: the cache key ignores the engine, so the developer's real cache would leak
// into baselines. Wikipedia is the default e2e engine (keyless, no DuckDuckGo rate limit).
const e2eCacheDir = mkdtempSync(join(tmpdir(), 'oxe-e2e-cache-'))

/**
 * e2e config: testDir points at the repo's tests/e2e/ markdown checkpoints.
 * The tiny adapter (e2e.md.ts) reads each .md file's `status:` frontmatter and
 * emits one test per checkpoint, skipped when `status: skip`. Dual-server
 * setup: FastAPI backend on 4479 (uv run oxe) + Vite dev server proxying /api
 * and /search to it, so the UI's same-origin relative fetches work unchanged.
 */
export default defineConfig({
  testDir: '../tests/e2e',
  testMatch: '**/*.md.ts',
  fullyParallel: false,
  workers: 1,
  use: {
    baseURL: 'http://localhost:5173',
  },
  webServer: [
    {
      command: 'uv run uvicorn oxe.app:app --host 127.0.0.1 --port 4577',
      cwd: '..',
      env: {
        ...process.env,
        OXE_BACKENDS: process.env.OXE_BACKENDS ?? 'wikipedia-opensearch',
        OXE_CACHE_DIR: e2eCacheDir,
      },
      url: 'http://127.0.0.1:4577/health',
      reuseExistingServer: false,
      // Dedicated port: the developer's long-running oxe (the local search
      // proxy) holds 4479; a fresh e2e instance avoids stale-state coupling.
      timeout: 30_000,
    },
    {
      command: 'bun run dev --strictPort',
      env: { ...process.env, E2E_BACKEND: 'http://127.0.0.1:4577' },
      url: 'http://localhost:5173',
      reuseExistingServer: false,
      timeout: 30_000,
    },
  ],
})
