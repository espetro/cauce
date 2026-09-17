/**
 * Single source of truth for the two SWC-native passes that must compose in the same
 * `jsc.experimental.plugins` pipeline: React Compiler (`jsc.transform.reactCompiler`) and
 * the Lingui SWC plugin (`@lingui/swc-plugin`). Both `vite.config.ts` (the real build) and
 * `verify-swc-pipeline.test.ts` (the build-time proof) import these constants, so the test
 * can never silently drift from what the real build actually configures.
 */
export const REACT_COMPILER_ENABLED = true as const
export const LINGUI_SWC_PLUGIN: [string, Record<string, never>] = ['@lingui/swc-plugin', {}]
