/**
 * Shared search input: the landing pill and the /search results-header pill are the same
 * component (search.md: "both inputs share <SearchBox>").
 *
 * Doctrine split: Base UI's `Autocomplete` part owns focus, portal and keyboard (arrow
 * selection, Enter submits in the active mode, Escape closes, Tab fills); daisyUI classes
 * own the look, composed via Base UI's `render` prop.
 *
 * State rungs: the typed value lives in the uncontrolled input (rung 2), read via
 * `FormData` on submit; the `suggest=1` fixture lives in the URL (rung 1) and forces the
 * dropdown open in dev (checkpoint 6). On submit, `suggest` is stripped (the checkpoint's
 * "strip on submit") and navigation goes through the router so back/forward work.
 */
import { Autocomplete } from '@base-ui/react/autocomplete'
import IconSearch from '~icons/lucide/search'
import { Trans } from '@lingui/react/macro'
import { useNavigate } from '@tanstack/react-router'
import { Route } from '../routes/__root.tsx'
import { fixturesEnabled } from '../lib/fixtures.ts'

export interface SearchBoxProps {
  initialQuery?: string
  forceDropdownOpen?: boolean
}

export function SearchBox({ initialQuery = '', forceDropdownOpen }: SearchBoxProps) {
  const navigate = useNavigate()
  const routeSuggest = Route.useSearch({
    select: (s) => ('suggest' in s ? s.suggest : false),
  })
  const dropdownOpen = (forceDropdownOpen ?? routeSuggest) && fixturesEnabled()

  return (
    <Autocomplete.Root items={[]} defaultOpen={dropdownOpen}>
      <form
        onSubmit={(event) => {
          event.preventDefault()
          const formData = new FormData(event.currentTarget)
          const q = String(formData.get('q') ?? '').trim()
          if (q.length > 0) {
            void navigate({ to: '/search', search: { q } })
          }
        }}
      >
        <Autocomplete.InputGroup className="join w-full">
          <Autocomplete.Input
            name="q"
            aria-label="Search query"
            placeholder="Search the web"
            render={<input className="input input-bordered join-item flex-1" defaultValue={initialQuery} />}
          />
          <button type="submit" className="btn btn-neutral join-item" aria-label="Search">
            <IconSearch aria-hidden="true" />
            <span className="sr-only">
              <Trans>Search</Trans>
            </span>
          </button>
        </Autocomplete.InputGroup>
        <Autocomplete.Portal>
          <Autocomplete.Positioner sideOffset={4}>
            <Autocomplete.Popup className="menu rounded-box w-(--popover-width) border border-base-300 bg-base-100 shadow">
              <Autocomplete.Empty>
                <li>
                  <span className="text-base-content/60">
                    <Trans>No matches</Trans>
                  </span>
                </li>
              </Autocomplete.Empty>
              <Autocomplete.List>
                {(item: string) => (
                  <Autocomplete.Item key={item} value={item}>
                    {item}
                  </Autocomplete.Item>
                )}
              </Autocomplete.List>
            </Autocomplete.Popup>
          </Autocomplete.Positioner>
        </Autocomplete.Portal>
      </form>
    </Autocomplete.Root>
  )
}
