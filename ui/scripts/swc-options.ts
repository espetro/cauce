/**
 * Single source of truth for the SWC-native passes configured in `jsc.experimental.plugins`.
 * Both `vite.config.ts` (the real build) and `verify-swc-pipeline.test.ts` (the build-time
 * proof) import these constants, so the test can never silently drift from what the real
 * build actually configures.
 */
export const REACT_COMPILER_ENABLED = true as const
