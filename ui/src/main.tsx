import { I18nProvider } from '@lingui/react'
import { createRouter, RouterProvider } from '@tanstack/react-router'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { i18n } from './lib/i18n'
import './index.css'
import { routeTree } from './routeTree.gen'

const router = createRouter({ routeTree })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}

const rootElement = document.getElementById('root')
if (!rootElement) {
  throw new Error('Root element #root not found')
}

createRoot(rootElement).render(
  <StrictMode>
    <I18nProvider i18n={i18n}>
      <RouterProvider router={router} />
    </I18nProvider>
  </StrictMode>,
)
