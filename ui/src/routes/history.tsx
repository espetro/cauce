import { Trans, useLingui } from '@lingui/react/macro'
import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import * as v from 'valibot'
import { history, type ClickItem, type HistoryResponse } from '../lib/historyApi.ts'
import { settingsSchema } from '../lib/routeSearch.ts'
import { Shell } from '../components/Shell.tsx'

const SINCE_VALUES = ['24', '168', '720'] as const

/**
 * `/history` (history.md, checkpoints 15/16/17). `since` is the hours filter (`24`=last day,
 * `168`=last week, `720`=last month; absent is the default "all time" and is stripped from
 * the url per userflow-checkpoints.md). `qf` is the client-side query-text substring filter
 * (named `qf` rather than `q` because the backend's `GET /api/history` `q` param has
 * different semantics — checkpoint 17).
 */
const historySearchSchema = v.object({
  since: v.optional(v.picklist(SINCE_VALUES)),
  qf: v.optional(v.string()),
  settings: settingsSchema,
})

export const Route = createFileRoute('/history')({
  validateSearch: historySearchSchema,
  loaderDeps: ({ search: { since } }) => ({ since }),
  loader: ({ deps: { since } }) => history({ since, limit: HISTORY_LIMIT }),
  component: HistoryComponent,
})

/** history.md: "Cap of 200 rows per view; use the filters to reach older entries." */
const HISTORY_LIMIT = 200

/**
 * The URL `since` param (hours as string) matches the API's query-param values exactly —
 * `GET /api/history?since=` takes the same '24'/'168'/'720' strings the URL carries. The
 * response echoes `since` back as the numeric enum, not the string.
 */

const SINCE_LABELS: Record<(typeof SINCE_VALUES)[number] | 'all', string> = {
  all: 'all time',
  '24': 'last 24h',
  '168': 'last week',
  '720': 'last month',
}

/** history.md responsive rule: the url column hides below ~768px. */
const URL_CELL_CLASS = 'hidden md:table-cell'

function formatTimestamp(unixSeconds: number): string {
  const date = new Date(unixSeconds * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ` +
    `${pad(date.getHours())}:${pad(date.getMinutes())}`
  )
}

/** The stats line (history.md): clicks in last 24h · total · oldest (dash when empty). */
function statsLine(data: HistoryResponse): { clicks24h: number; total: number; oldest: string } {
  const stats = data.stats
  return {
    clicks24h: stats.last_24h,
    total: stats.total,
    oldest: stats.oldest === null ? '—' : formatTimestamp(stats.oldest),
  }
}

/**
 * Client-side `qf` substring filter (checkpoint 17): narrows the fetched rows by query text
 * without refetching — the server's `q` param is deliberately not used for it, per
 * checkpoint 17's "`qf` keeps the UI filter distinct and maps to the server-side `q`".
 */
function filterByQf(items: ClickItem[], qf: string): ClickItem[] {
  const needle = qf.trim().toLowerCase()
  if (needle === '') return items
  return items.filter((item) => item.query.toLowerCase().includes(needle))
}

function HistoryComponent() {
  const { t } = useLingui()
  const data = Route.useLoaderData()
  const search = Route.useSearch()
  const navigate = Route.useNavigate()
  const { qf } = search
  const since = search.since ?? 'all'
  const [filterText, setFilterText] = useState(qf ?? '')

  const { clicks24h, total, oldest } = statsLine(data)
  const hasFilter = since !== 'all' || (qf ?? '') !== ''
  const rows = filterByQf(data.items, qf ?? '')

  const setParam = (updates: { since?: string; qf?: string }) => {
    const next: Record<string, string | undefined> = {}
    if (updates.since !== undefined && updates.since !== 'all') next.since = updates.since
    if (updates.qf !== undefined && updates.qf !== '') next.qf = updates.qf
    void navigate({ search: (prev) => ({ ...prev, ...next }) })
  }

  return (
    <Shell size="lg">
      <h1 className="text-xl font-semibold">
        <Trans>Click history</Trans>
      </h1>

      <p className="mt-1 text-sm text-base-content/60" data-testid="history-stats">
        <Trans>
          {clicks24h} clicks in last 24h · {total} total · {oldest} (oldest)
        </Trans>
      </p>

      <div className="mt-4 flex flex-wrap items-center gap-2">
        <select
          aria-label={t`Filter by time`}
          className="select select-bordered select-sm"
          value={since}
          onChange={(event) => {
            setParam({ since: event.target.value })
          }}
        >
          {(['all', '24', '168', '720'] as const).map((value) => (
            <option key={value} value={value}>
              {SINCE_LABELS[value]}
            </option>
          ))}
        </select>
        <input
          type="search"
          aria-label={t`Filter by query text`}
          className="input input-bordered input-sm w-64"
          placeholder={t`filter by query text...`}
          value={filterText}
          onChange={(event) => {
            setFilterText(event.target.value)
          }}
          onBlur={(event) => {
            setParam({ qf: event.target.value })
          }}
          onKeyDown={(event) => {
            if (event.key === 'Enter') setParam({ qf: filterText })
          }}
        />
        {hasFilter ? (
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={() => {
              void navigate({ search: (prev) => ({ ...prev, since: undefined, qf: undefined }) })
            }}
          >
            <Trans>clear</Trans>
          </button>
        ) : null}
      </div>

      {total === 0 ? (
        <p className="mt-8 text-base-content/60">
          <Trans>no clicks yet — open a result from the search page.</Trans>
        </p>
      ) : rows.length === 0 ? (
        <p className="mt-8 text-base-content/60">
          <Trans>no rows match the current filters.</Trans>
        </p>
      ) : (
        <div className="mt-4 overflow-x-auto">
          <table className="table table-sm">
            <thead>
              <tr>
                <th>
                  <Trans>clicked</Trans>
                </th>
                <th>
                  <Trans>query</Trans>
                </th>
                <th>
                  <Trans>title</Trans>
                </th>
                <th className={URL_CELL_CLASS}>
                  <Trans>url</Trans>
                </th>
                <th>
                  <Trans>src</Trans>
                </th>
                <th>{/* copy json */}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((item) => (
                <tr key={`${item.clicked_at}:${item.result_id}`}>
                  <td className="whitespace-nowrap">{formatTimestamp(item.clicked_at)}</td>
                  <td>
                    <a href={`/row/${item.query_hash}`} className="link link-hover">
                      {item.query}
                    </a>
                  </td>
                  <td className="max-w-48 truncate">
                    <a href={item.url} target="_blank" rel="noreferrer" className="link link-hover">
                      {item.title === '' ? item.url : item.title}
                    </a>
                  </td>
                  <td className={`${URL_CELL_CLASS} max-w-64 truncate`}>{item.url}</td>
                  <td>{item.source}</td>
                  <td>
                    <CopyJsonButton query={item.query} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Shell>
  )
}

/**
 * history.md's per-row `copy json`: re-fetches the query from `GET /search`
 * (`Accept: application/json`) and copies the Exa-shaped payload to the clipboard.
 */
function CopyJsonButton({ query }: { query: string }) {
  const { t } = useLingui()
  const [state, setState] = useState<'idle' | 'busy' | 'done' | 'error'>('idle')

  const copy = async () => {
    setState('busy')
    try {
      const params = new URLSearchParams({ q: query, format: 'json' })
      const response = await fetch(`/search?${params}`, { headers: { Accept: 'application/json' } })
      if (!response.ok) throw new Error(String(response.status))
      await navigator.clipboard.writeText(JSON.stringify(await response.json(), null, 2))
      setState('done')
    } catch {
      setState('error')
    }
  }

  return (
    <button type="button" className="btn btn-ghost btn-xs" onClick={() => void copy()}>
      {state === 'busy' ? (
        <span className="loading loading-dots loading-xs" aria-label={t`copying`} />
      ) : state === 'done' ? (
        <Trans>copied</Trans>
      ) : state === 'error' ? (
        <Trans>copy failed</Trans>
      ) : (
        <Trans>copy json</Trans>
      )}
    </button>
  )
}
