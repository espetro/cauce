import { Trans } from '@lingui/react/macro'
import { createFileRoute } from '@tanstack/react-router'
import * as v from 'valibot'
import { settingsSchema } from '../lib/routeSearch.ts'

/**
 * `/dashboard` (dashboard.md): "No params planned: the dashboard window is a build-time
 * constant, not URL state." Only `settings=open` is shared with every other route.
 *
 * Placeholder only: the `GET /api/stats` loader and panel grid are screen-task work.
 */
const dashboardSearchSchema = v.object({
  settings: settingsSchema,
})

export const Route = createFileRoute('/dashboard')({
  validateSearch: dashboardSearchSchema,
  component: DashboardComponent,
})

function DashboardComponent() {
  return (
    <main className="mx-auto max-w-4xl px-4 py-8">
      <h1 className="text-xl font-semibold">
        <Trans>oxe stats</Trans>
      </h1>
    </main>
  )
}
