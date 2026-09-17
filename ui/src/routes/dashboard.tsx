import { Trans } from '@lingui/react/macro'
import { createFileRoute } from '@tanstack/react-router'
import type { ReactNode } from 'react'
import * as v from 'valibot'
import { dashboardHasLogData, stats, type StatsResponse } from '../lib/historyApi.ts'
import { settingsSchema } from '../lib/routeSearch.ts'

/**
 * `/dashboard` (dashboard.md, checkpoint 20): live SPA route fetching aggregate usage and
 * cache stats from `GET /api/stats` via the router loader. "No params planned: the dashboard
 * window is a build-time constant, not URL state." Only `settings=open` is shared with every
 * other route.
 */
const dashboardSearchSchema = v.object({
  settings: settingsSchema,
})

/** dashboard.md mockup: "window: last 30 days". */
const STATS_WINDOW_DAYS = 30

export const Route = createFileRoute('/dashboard')({
  validateSearch: dashboardSearchSchema,
  loader: () => stats(STATS_WINDOW_DAYS),
  component: DashboardComponent,
})

/** dashboard.md: `repeat(auto-fit, minmax(20rem, 1fr))` responsive panel grid. */
const PANEL_GRID_CLASS = 'grid grid-cols-[repeat(auto-fit,minmax(20rem,1fr))] gap-4'

function DashboardComponent() {
  const data = Route.useLoaderData()
  const hasLogData = dashboardHasLogData(data)

  return (
    <main className="mx-auto max-w-6xl px-4 py-8">
      <h1 className="text-xl font-semibold">
        <Trans>oxe stats</Trans>
      </h1>
      <p className="mt-1 text-sm text-base-content/60">
        <Trans>window: last 30 days</Trans>
      </p>

      <div className={`${PANEL_GRID_CLASS} mt-6`}>
        <Panel title={<Trans>searches per day</Trans>}>
          <NoLogData />
        </Panel>
        <Panel title={<Trans>cache hit rate</Trans>}>
          <HitRatePanel data={data} />
        </Panel>
        <Panel title={<Trans>network latency</Trans>}>
          {hasLogData ? (
            <p className="text-sm">
              p50 {data.latency_ms.p50 ?? '—'} · p90 {data.latency_ms.p90 ?? '—'} · p99{' '}
              {data.latency_ms.p99 ?? '—'} ms
            </p>
          ) : (
            <NoLogData />
          )}
        </Panel>
        <Panel title={<Trans>client split</Trans>}>
          {hasLogData && data.client_split.length > 0 ? (
            <ul className="text-sm">
              {data.client_split.map((entry) => (
                <li key={entry.client}>
                  {entry.client}: {entry.count}
                </li>
              ))}
            </ul>
          ) : (
            <NoLogData />
          )}
        </Panel>
        <div className={`${PANEL_GRID_CLASS} col-span-full`}>
          {/* Backend gap: GET /api/stats ships no `cache` key (rows/unexpired/db size —
              dashboard.md). Adding it is future backend work; the panel stays muted. */}
          <Panel title={<Trans>cache</Trans>} wide>
            <CachePanel />
          </Panel>
        </div>
      </div>
    </main>
  )
}

/** dashboard.md: muted flat placeholder line for panels the backend does not aggregate yet. */
function NoLogData() {
  return (
    <p className="text-sm text-base-content/50">
      <Trans>no search log data yet</Trans>
    </p>
  )
}

function Panel({
  title,
  wide,
  children,
}: {
  title: ReactNode
  wide?: boolean
  children: ReactNode
}) {
  return (
    <section className={`card bg-base-200 ${wide ? 'col-span-full' : ''}`}>
      <div className="card-body p-4">
        <h2 className="card-title text-sm text-base-content/70">{title}</h2>
        {children}
      </div>
    </section>
  )
}

/** dashboard.md: hit-rate percentage (cache_hits / total across the window) + total hits. */
function HitRatePanel({ data }: { data: StatsResponse }) {
  const rate =
    data.hit_rate.total > 0
      ? `${Math.round(((data.hit_rate.cache_hits ?? 0) / data.hit_rate.total) * 100)}%`
      : null
  if (rate === null) {
    return <NoLogData />
  }
  return (
    <p className="text-sm">
      <span className="text-2xl font-semibold">{rate}</span>{' '}
      <span className="text-base-content/60">({data.hit_rate.total} total hits)</span>
    </p>
  )
}

/**
 * dashboard.md's cache table (rows, unexpired, db size, newest) is NOT served by
 * `GET /api/stats` — there is no `cache` key in the wire contract. Adding one is future
 * backend work; until then this panel renders its muted placeholder unconditionally.
 */
function CachePanel() {
  return (
    <p className="text-sm text-base-content/50">
      <Trans>no cache stats yet — backend does not aggregate cache stats.</Trans>
    </p>
  )
}

