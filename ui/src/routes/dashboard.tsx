import { Trans } from '@lingui/react/macro'
import { createFileRoute } from '@tanstack/react-router'
import type { ReactNode } from 'react'
import * as v from 'valibot'
import {
  cacheHitRatePercent,
  dashboardHasLogData,
  stats,
  type StatsResponse,
} from '../lib/historyApi.ts'
import { settingsSchema } from '../lib/routeSearch.ts'
import { Shell } from '../components/Shell.tsx'

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
    <Shell size="xl">
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
              p50 {data.latency_ms.p50 ?? 'n/a'} · p90 {data.latency_ms.p90 ?? 'n/a'} · p99{' '}
              {data.latency_ms.p99 ?? 'n/a'} ms
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
          <Panel title={<Trans>cache</Trans>} wide>
            <CachePanel data={data} />
          </Panel>
        </div>
      </div>
    </Shell>
  )
}

/** dashboard.md: muted flat placeholder line for panels the backend does not aggregate yet. */
function NoLogData() {
  return (
    <p className="text-sm text-base-content/60">
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

/** dashboard.md: hit-rate percentage (unexpired/rows) + total hits, both from the cache table. */
function HitRatePanel({ data }: { data: StatsResponse }) {
  const percent = cacheHitRatePercent(data)
  if (percent === null) {
    return <NoCacheRows />
  }
  return (
    <p className="text-sm">
      <span className="text-2xl font-semibold">{percent}%</span>{' '}
      <span className="text-base-content/60">
        <Trans>({data.cache.total_hits} total hits)</Trans>
      </span>
    </p>
  )
}

function NoCacheRows() {
  return (
    <p className="text-sm text-base-content/60">
      <Trans>no cached searches yet</Trans>
    </p>
  )
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`
  }
  const units = ['KB', 'MB', 'GB']
  let value = bytes / 1024
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${value.toFixed(1)} ${units[unit]}`
}

/** dashboard.md mockup: "2026-09-14 19:42" (UTC). */
function formatNewest(ts: number | null): string {
  return ts === null ? 'n/a' : new Date(ts * 1000).toISOString().slice(0, 16).replace('T', ' ')
}

/** dashboard.md cache table: rows, unexpired, db size, newest. */
function CachePanel({ data }: { data: StatsResponse }) {
  const { rows, unexpired, db_size_bytes: dbSize, newest } = data.cache
  return (
    <dl className="grid grid-cols-2 gap-x-6 gap-y-1 text-sm">
      <div className="flex gap-2">
        <dt className="text-base-content/60">
          <Trans>rows</Trans>
        </dt>
        <dd>{rows}</dd>
      </div>
      <div className="flex gap-2">
        <dt className="text-base-content/60">
          <Trans>unexpired</Trans>
        </dt>
        <dd>{unexpired}</dd>
      </div>
      <div className="flex gap-2">
        <dt className="text-base-content/60">
          <Trans>db size</Trans>
        </dt>
        <dd>{formatBytes(dbSize)}</dd>
      </div>
      <div className="flex gap-2">
        <dt className="text-base-content/60">
          <Trans>newest</Trans>
        </dt>
        <dd>{formatNewest(newest)}</dd>
      </div>
    </dl>
  )
}
