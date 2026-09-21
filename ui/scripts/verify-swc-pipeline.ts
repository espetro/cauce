/**
 * Proves the SWC pipeline composes: React Compiler (`jsc.transform.reactCompiler`) and the
 * Lingui SWC plugin (`@lingui/swc-plugin`) run in the same `jsc.experimental.plugins` pass
 * without one clobbering the other's output.
 *
 * `src/routes/index.tsx` is the component both passes must touch: it carries a `<Trans>`
 * macro (Lingui) inside JSX the compiler will memoize (React Compiler). We run it through
 * `@swc/core` with the exact same `jsc` shape `vite.config.ts` configures (via the shared
 * constants in `swc-options.ts`) and check the emitted code for two independent
 * fingerprints:
 *
 *   1. `import { c as _c } from "react/compiler-runtime"` — proves React Compiler ran.
 *   2. The Lingui-extracted message id for "your local web intel layer", computed the same way
 *      `@lingui/swc-plugin` computes it (`@lingui/message-utils/generateMessageId`) —
 *      proves the macro was expanded, not passed through as literal JSX.
 *
 * Rollup inlines and renames the `react/compiler-runtime` specifier away once it bundles
 * the app for production, so this checks the direct SWC transform output rather than the
 * final `dist/` chunk for that fingerprint. `verify-swc-pipeline.test.ts` separately runs a
 * real `bun run build` end to end and checks the message id survives all the way into
 * `dist/`, so both "the two SWC passes compose" and "the full build pipeline produces a
 * working bundle" are covered.
 */
import { transform } from '@swc/core'
import { generateMessageId } from '@lingui/message-utils/generateMessageId'
import { LINGUI_SWC_PLUGIN, REACT_COMPILER_ENABLED } from './swc-options.ts'

export const SOURCE_MESSAGE = 'your local web intel layer'
export const EXPECTED_MESSAGE_ID = generateMessageId(SOURCE_MESSAGE)
export const COMPILER_RUNTIME_MARKER = 'react/compiler-runtime'

export interface SwcPipelineCheck {
  hasCompilerRuntimeImport: boolean
  hasLinguiMessageId: boolean
  code: string
}

/** Runs `source` through the same SWC jsc shape the real Vite build uses, and checks both fingerprints. */
export async function checkSwcOutputForPipelineArtifacts(
  source: string,
  filename: string,
): Promise<SwcPipelineCheck> {
  const { code } = await transform(source, {
    filename,
    jsc: {
      parser: { syntax: 'typescript', tsx: true },
      transform: {
        react: { runtime: 'automatic' },
        reactCompiler: REACT_COMPILER_ENABLED,
      },
      experimental: {
        plugins: [LINGUI_SWC_PLUGIN],
      },
    },
  })

  return {
    hasCompilerRuntimeImport: code.includes(COMPILER_RUNTIME_MARKER),
    hasLinguiMessageId: code.includes(EXPECTED_MESSAGE_ID),
    code,
  }
}
