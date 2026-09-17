import { Trans } from '@lingui/react/macro'
import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/')({
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
