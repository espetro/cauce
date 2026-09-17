import react from '@vitejs/plugin-react-swc'
import { defineConfig } from 'vite'
import { REACT_COMPILER_ENABLED } from './scripts/swc-options.ts'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    react({
      useAtYourOwnRisk_mutateSwcOptions(options) {
        options.jsc ??= {}
        options.jsc.transform ??= {}
        options.jsc.transform.reactCompiler = REACT_COMPILER_ENABLED
      },
    }),
  ],
})
