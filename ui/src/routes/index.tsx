import { Trans } from '@lingui/react/macro'
import { createFileRoute } from '@tanstack/react-router'
import * as v from 'valibot'
import { modeSchema, settingsSchema } from '../lib/routeSearch.ts'

/**
 * Landing route search params (checkpoint 1 + 2 + 18 in userflow-checkpoints.md): `mode=ai`
 * puts the pill in AI mode before any query is typed, `settings=open` opens the settings
 * dialog overlay. Neither is implemented yet (routing skeleton only, per this foundation
 * task) -- screen tasks wire the actual pill/dialog behavior against these params.
 */
const landingSearchSchema = v.object({
  mode: modeSchema,
  settings: settingsSchema,
})

export const Route = createFileRoute('/')({
  validateSearch: landingSearchSchema,
  component: HomeComponent,
})

function HomeComponent() {
  return (
    <main>
      <h1>
        <Trans>Welcome to oxe</Trans>
      </h1>
    </main>
  )
}
