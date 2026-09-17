import { Trans } from '@lingui/react/macro'
import { createFileRoute } from '@tanstack/react-router'
import type { ReactNode } from 'react'
import * as v from 'valibot'
import { client } from '../lib/api.ts'
import { settingsSchema } from '../lib/routeSearch.ts'
import type { components } from '../lib/types.gen.ts'

type DashboardResponse = components['schemas']['DashboardResponse']

/**
 * `/dashboard` (dashboard.md): "No params planned: the dashboard window is a build-time
 * constant, not URL state." Only `settings=open` is shared with every other route.
 */
const dashboardSearchSchema = v.object({
  settings: settingsSchema,
})

export const Route = createFileRoute('/dashboard')({
  validateSearch: dashboardSearchSchema,
  loader: async (): Promise<DashboardResponse> => {
    const { data, error } = await client.GET('/api/stats')
    if (error) {
      throw error
    }
    return data
  },
  component: DashboardComponent,
})

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

function formatLocalTimestamp(ts: number | null): string {
  if (ts === null) {
    return '—'
  }
  const d = new Date(ts * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

function Panel({
  title,
  children,
  wide,
}: {
  title: ReactNode
  children: ReactNode
  wide?: boolean
}) {
  return (
    <section className={`card bg-base-200 p-4 ${wide ? 'sm:col-span-2' : ''}`}>
      <h2 className="mb-2 text-sm font-semibold text-base-content/70">{title}</h2>
      {children}
    </section>
  )
}

function NoLogData() {
  return (
    <p className="text-sm text-base-content/50 italic">
      <Trans>no search log data yet</Trans>
    </p>
  )
}

function DashboardComponent() {
  const data = Route.useLoaderData()
  const hasLogData = data.log.hit_rate.total > 0

  return (
    <main className="mx-auto max-w-4xl px-4 py-8">
      <h1 className="text-xl font-semibold">
        <Trans>oxe stats</Trans>
      </h1>
      <p className="mb-4 text-sm text-base-content/70">
        <Trans>window: last {data.log.days} days</Trans>
      </p>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <Panel title={<Trans>searches per day</Trans>}>
          {hasLogData ? (
            <ul className="text-sm">
              {data.log.searches_per_day.map((day) => (
                <li key={day.day} className="flex justify-between">
                  <span>{day.day}</span>
                  <span>{day.total}</span>
                </li>
              ))}
            </ul>
          ) : (
            <NoLogData />
          )}
        </Panel>

        <Panel title={<Trans>cache hit rate</Trans>}>
          {data.hit_rate_pct === null ? (
            <NoLogData />
          ) : (
            <div>
              <div className="text-4xl font-bold">{data.hit_rate_pct}%</div>
              <p className="text-sm text-base-content/70">
                <Trans>{data.cache.total_hits} total hits</Trans>
              </p>
            </div>
          )}
        </Panel>

        <Panel title={<Trans>network latency</Trans>}>
          {hasLogData ? (
            <ul className="text-sm">
              <li>
                <Trans>p50: {data.log.latency_ms.p50} ms</Trans>
              </li>
              <li>
                <Trans>p90: {data.log.latency_ms.p90} ms</Trans>
              </li>
              <li>
                <Trans>p99: {data.log.latency_ms.p99} ms</Trans>
              </li>
            </ul>
          ) : (
            <NoLogData />
          )}
        </Panel>

        <Panel title={<Trans>client split</Trans>}>
          {data.log.client_split.length > 0 ? (
            <ul className="text-sm">
              {data.log.client_split.map((c) => (
                <li key={c.client} className="flex justify-between">
                  <span>{c.client}</span>
                  <span>{c.count}</span>
                </li>
              ))}
            </ul>
          ) : (
            <NoLogData />
          )}
        </Panel>

        <Panel title={<Trans>cache</Trans>} wide>
          <div className="grid grid-cols-2 gap-2 text-sm sm:grid-cols-4">
            <div>
              <div className="text-base-content/50">
                <Trans>rows</Trans>
              </div>
              <div>{data.cache.rows}</div>
            </div>
            <div>
              <div className="text-base-content/50">
                <Trans>unexpired</Trans>
              </div>
              <div>{data.cache.unexpired_rows}</div>
            </div>
            <div>
              <div className="text-base-content/50">
                <Trans>db size</Trans>
              </div>
              <div>{formatBytes(data.cache.db_size_bytes)}</div>
            </div>
            <div>
              <div className="text-base-content/50">
                <Trans>newest</Trans>
              </div>
              <div>{formatLocalTimestamp(data.cache.newest)}</div>
            </div>
          </div>
        </Panel>
      </div>
    </main>
  )
}
