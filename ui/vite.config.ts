import { tanstackRouter } from '@tanstack/router-plugin/vite'
import react from '@vitejs/plugin-react-swc'
import { defineConfig } from 'vite'
import { LINGUI_SWC_PLUGIN, REACT_COMPILER_ENABLED } from './scripts/swc-options.ts'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    tanstackRouter({
      target: 'react',
      autoCodeSplitting: true,
      routesDirectory: './src/routes',
      generatedRouteTree: './src/routeTree.gen.ts',
    }),
    react({
      useAtYourOwnRisk_mutateSwcOptions(options) {
        options.jsc ??= {}
        options.jsc.transform ??= {}
        options.jsc.transform.reactCompiler = REACT_COMPILER_ENABLED

        options.jsc.experimental ??= {}
        options.jsc.experimental.plugins = [
          ...(options.jsc.experimental.plugins ?? []),
          LINGUI_SWC_PLUGIN,
        ]
      },
    }),
  ],
})
