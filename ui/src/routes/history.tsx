import { Trans, useLingui } from '@lingui/react/macro'
import { createFileRoute } from '@tanstack/react-router'
import { useReducer } from 'react'
import * as v from 'valibot'
import { client } from '../lib/api.ts'
import { copyJsonReducer, initialCopyJsonState } from '../lib/copyJson.ts'
import { settingsSchema } from '../lib/routeSearch.ts'
import type { components } from '../lib/types.gen.ts'

const SINCE_VALUES = ['24', '168', '720'] as const
type SinceValue = (typeof SINCE_VALUES)[number]

type HistoryRow = components['schemas']['HistoryRow']
type HistoryResponse = components['schemas']['HistoryResponse']

/**
 * `/history` (history.md, checkpoints 15/16/17). `since` is the hours filter (`24`=last day,
 * `168`=last week, `720`=last month; `all`/absent is the default and is stripped from the
 * url per userflow-checkpoints.md). `qf` is the client-side query-text substring filter
 * (named `qf` rather than `q` because the backend's `GET /api/history` `q` param has
 * different semantics).
 */
const historySearchSchema = v.object({
  since: v.optional(v.picklist(SINCE_VALUES)),
  qf: v.optional(v.string()),
  settings: settingsSchema,
})

export const Route = createFileRoute('/history')({
  validateSearch: historySearchSchema,
  loaderDeps: ({ search }) => ({ since: search.since, qf: search.qf }),
  loader: async ({ deps }): Promise<HistoryResponse> => {
    const { data, error } = await client.GET('/api/history', {
      params: {
        query: {
          since: deps.since ? Number(deps.since) : undefined,
          q: deps.qf || undefined,
        },
      },
    })
    if (error) {
      throw error
    }
    return data
  },
  component: HistoryComponent,
})

function formatLocalTimestamp(ts: number): string {
  const d = new Date(ts * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

function StatsLine({ stats }: { stats: HistoryResponse['stats'] }) {
  return (
    <p className="text-sm text-base-content/70">
      <Trans>{stats.last_24h} clicks in last 24h</Trans> ·{' '}
      <Trans>{stats.total} total</Trans> ·{' '}
      {stats.oldest === null ? (
        <Trans>— (oldest)</Trans>
      ) : (
        <Trans>{formatLocalTimestamp(stats.oldest)} (oldest)</Trans>
      )}
    </p>
  )
}

function CopyJsonButton({ query }: { query: string }) {
  const { t } = useLingui()
  const [state, dispatch] = useReducer(copyJsonReducer, initialCopyJsonState)

  const label =
    state.status === 'loading'
      ? t`copying…`
      : state.status === 'copied'
        ? t`copied!`
        : state.status === 'error'
          ? t`copy failed`
          : t`copy json`

  return (
    <button
      type="button"
      className="btn btn-ghost btn-xs"
      aria-label={t`Copy this query's JSON payload`}
      title={state.status === 'error' ? state.message : undefined}
      onClick={() => {
        dispatch({ type: 'start' })
        client.GET('/search', { params: { query: { q: query } } }).then(
          ({ data, error }) => {
            if (error || !data) {
              dispatch({ type: 'failure', message: t`search request failed` })
              return
            }
            navigator.clipboard.writeText(JSON.stringify(data)).then(
              () => dispatch({ type: 'success' }),
              () => dispatch({ type: 'failure', message: t`clipboard write failed` }),
            )
          },
          () => dispatch({ type: 'failure', message: t`search request failed` }),
        )
      }}
    >
      {label}
    </button>
  )
}

function HistoryRowView({ row }: { row: HistoryRow }) {
  return (
    <tr>
      <td className="whitespace-nowrap">{formatLocalTimestamp(row.clicked_at)}</td>
      <td className="max-w-[16rem] truncate">{row.query}</td>
      <td className="max-w-[16rem] truncate">
        <a href={row.url} target="_blank" rel="noreferrer" className="link">
          {row.title || row.url}
        </a>
      </td>
      <td className="hidden max-w-[16rem] truncate md:table-cell">{row.url}</td>
      <td>{row.source}</td>
      <td>
        <CopyJsonButton query={row.query} />
      </td>
    </tr>
  )
}

function FilterControls() {
  const search = Route.useSearch()
  const navigate = Route.useNavigate()
  const { t } = useLingui()
  const hasFilter = search.since !== undefined || Boolean(search.qf)

  return (
    <div className="flex flex-wrap items-center gap-2 py-2">
      <select
        className="select select-sm"
        aria-label={t`Time range`}
        value={search.since ?? 'all'}
        onChange={(event) => {
          const value = event.target.value
          navigate({
            search: (prev) => ({
              ...prev,
              since: value === 'all' ? undefined : (value as SinceValue),
            }),
            replace: true,
          })
        }}
      >
        <option value="all">
          <Trans>all time</Trans>
        </option>
        <option value="24">
          <Trans>last 24h</Trans>
        </option>
        <option value="168">
          <Trans>last week</Trans>
        </option>
        <option value="720">
          <Trans>last month</Trans>
        </option>
      </select>
      <input
        type="text"
        className="input input-sm"
        placeholder={t`filter by query text...`}
        aria-label={t`Filter by query text`}
        value={search.qf ?? ''}
        onChange={(event) => {
          const value = event.target.value
          navigate({
            search: (prev) => ({ ...prev, qf: value || undefined }),
            replace: true,
          })
        }}
      />
      {hasFilter && (
        <button
          type="button"
          className="btn btn-ghost btn-sm"
          onClick={() => {
            navigate({ search: (prev) => ({ ...prev, since: undefined, qf: undefined }) })
          }}
        >
          <Trans>clear</Trans>
        </button>
      )}
    </div>
  )
}

function HistoryComponent() {
  const data = Route.useLoaderData()

  return (
    <main className="mx-auto max-w-4xl px-4 py-8">
      <h1 className="text-xl font-semibold">
        <Trans>Click history</Trans>
      </h1>
      <StatsLine stats={data.stats} />
      <FilterControls />
      {data.rows.length === 0 ? (
        <p className="py-8 text-base-content/70">
          {data.stats.total === 0 ? (
            <Trans>no clicks yet — open a result from the search page.</Trans>
          ) : (
            <Trans>no clicks match your filters.</Trans>
          )}
        </p>
      ) : (
        <div className="overflow-x-auto">
          <table className="table">
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
                <th className="hidden md:table-cell">
                  <Trans>url</Trans>
                </th>
                <th>
                  <Trans>source</Trans>
                </th>
                <th />
              </tr>
            </thead>
            <tbody>
              {data.rows.map((row) => (
                <HistoryRowView key={row.id} row={row} />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </main>
  )
}
