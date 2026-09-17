import { Trans } from '@lingui/react/macro'
import { createFileRoute } from '@tanstack/react-router'
import * as v from 'valibot'
import { settingsSchema } from '../lib/routeSearch.ts'

const SINCE_VALUES = ['24', '168', '720'] as const

/**
 * `/history` (history.md, checkpoints 15/16/17). `since` is the hours filter (`24`=last day,
 * `168`=last week, `720`=last month; `all`/absent is the default and is stripped from the
 * url per userflow-checkpoints.md). `qf` is the client-side query-text substring filter
 * (named `qf` rather than `q` because the backend's `GET /api/history` `q` param has
 * different semantics).
 *
 * Placeholder only: table rendering, filtering and the stats line are screen-task work
 * (history.md).
 */
const historySearchSchema = v.object({
  since: v.optional(v.picklist(SINCE_VALUES)),
  qf: v.optional(v.string()),
  settings: settingsSchema,
})

export const Route = createFileRoute('/history')({
  validateSearch: historySearchSchema,
  component: HistoryComponent,
})

function HistoryComponent() {
  return (
    <main className="mx-auto max-w-4xl px-4 py-8">
      <h1 className="text-xl font-semibold">
        <Trans>Click history</Trans>
      </h1>
    </main>
  )
}
