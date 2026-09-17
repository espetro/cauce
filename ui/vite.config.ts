import tailwindcss from '@tailwindcss/vite'
import { tanstackRouter } from '@tanstack/router-plugin/vite'
import react from '@vitejs/plugin-react-swc'
import { defineConfig } from 'vite'
import Icons from 'unplugin-icons/vite'
import { LINGUI_SWC_PLUGIN, REACT_COMPILER_ENABLED } from './scripts/swc-options.ts'

// https://vite.dev/config/
export default defineConfig({
  /* Dev-server proxy: the UI fetches /api and /search same-origin, so Vite
   * forwards both to the FastAPI backend on 4479 (also used by Playwright).
   * bypass: SPA navigations to /search want HTML — let Vite serve the app
   * shell instead of proxying to the backend's search endpoint. */
  server: {
    proxy: {
      '/api': process.env.E2E_BACKEND ?? 'http://127.0.0.1:4479',
      '/search': {
        target: process.env.E2E_BACKEND ?? 'http://127.0.0.1:4479',
        bypass: (req) => {
          if (req.headers.accept?.includes('text/html')) return req.url
          return undefined
        },
      },
    },
  },
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
    // Tailwind v4's Vite plugin: scans src for used classes, no tailwind.config.js needed.
    tailwindcss(),
    // unplugin-icons: `~icons/lucide/<name>` imports resolve to tree-shaken Lucide SVG
    // components at build time, so only the icons actually imported ship in the bundle.
    // `compiler: 'jsx'` (React auto-detected) needs `@svgr/core` + `@svgr/plugin-jsx`, both
    // devDependencies only -- the generated output is plain JSX, no runtime dependency on
    // svgr ships in the built bundle.
    Icons({ compiler: 'jsx', jsx: 'react', autoInstall: false }),
  ],
})
