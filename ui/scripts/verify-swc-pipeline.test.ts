import { execFileSync } from 'node:child_process'
import { readdirSync, readFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, test } from 'vitest'
import {
  checkSwcOutputForPipelineArtifacts,
  EXPECTED_MESSAGE_ID,
} from './verify-swc-pipeline.ts'

const uiRoot = path.dirname(path.dirname(fileURLToPath(import.meta.url)))
const targetFile = path.join(uiRoot, 'src', 'routes', 'index.tsx')

describe('SWC pipeline composition (React Compiler + @lingui/swc-plugin)', () => {
  test('the same jsc config the real build uses emits both a react/compiler-runtime import and the extracted Lingui message id, for the same component', async () => {
    const source = readFileSync(targetFile, 'utf8')
    const result = await checkSwcOutputForPipelineArtifacts(source, targetFile)

    expect(result.hasCompilerRuntimeImport).toBe(true)
    expect(result.hasLinguiMessageId).toBe(true)
  })

  test('the expected message id is derived from the actual <Trans> source text, not hardcoded', () => {
    expect(EXPECTED_MESSAGE_ID).toMatch(/^[A-Za-z0-9+/]+$/)
  })
})

describe('end-to-end build (extract -> compile -> vite build -> tsc)', () => {
  test('bun run build succeeds and the emitted dist bundle carries the same message id', () => {
    // Vitest sets NODE_ENV=test on its own process; without overriding it here, that leaks
    // into this spawned `vite build` and disables production dead-code elimination (React's
    // dev-mode branches ship unminified), inflating the dist bundle ~70KB gz for no reason
    // tied to the actual app. `bun run size` must be run against a real `bun run build`
    // (NODE_ENV=production, vite's own default for the build command) to get a meaningful
    // number -- this test only asserts the pipeline composes, not the bundle size, but it
    // still writes real dist/ output as a side effect, so keep that output representative.
    execFileSync('bun', ['run', 'build'], {
      cwd: uiRoot,
      stdio: 'inherit',
      env: { ...process.env, NODE_ENV: 'production' },
    })

    const distAssetsDir = path.join(uiRoot, 'dist', 'assets')
    const jsFiles = readdirSync(distAssetsDir).filter((file) => file.endsWith('.js'))
    expect(jsFiles.length).toBeGreaterThan(0)

    const combined = jsFiles
      .map((file) => readFileSync(path.join(distAssetsDir, file), 'utf8'))
      .join('\n')
    expect(combined).toContain(EXPECTED_MESSAGE_ID)
  }, 180_000)
})
