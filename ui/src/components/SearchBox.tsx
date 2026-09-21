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
import IconSparkles from '~icons/lucide/sparkles'
import { Trans, useLingui } from '@lingui/react/macro'
import { useNavigate } from '@tanstack/react-router'
import { useRef } from 'react'
import { Route } from '../routes/__root.tsx'
import { fixturesEnabled } from '../lib/fixtures.ts'

export interface SearchBoxProps {
  initialQuery?: string
  forceDropdownOpen?: boolean
  mode?: 'ai'
  onModeChange: (next: 'ai' | undefined, typedQuery: string) => void
  aiDisabled?: boolean
  autoFocus?: boolean
  className?: string
}

export function SearchBox({
  initialQuery = '',
  forceDropdownOpen,
  mode,
  onModeChange,
  aiDisabled = false,
  autoFocus,
  className = '',
}: SearchBoxProps) {
  const { t } = useLingui()
  const navigate = useNavigate()
  const formRef = useRef<HTMLFormElement>(null)
  const routeSuggest = Route.useSearch({
    select: (s) => ('suggest' in s ? s.suggest : false),
  })
  const dropdownOpen = (forceDropdownOpen ?? routeSuggest) && fixturesEnabled()
  const aiMode = mode === 'ai'
  const typedQuery = () => String(new FormData(formRef.current ?? undefined).get('q') ?? '').trim()

  return (
    <Autocomplete.Root items={[]} defaultOpen={dropdownOpen} defaultValue={initialQuery}>
      <form
        ref={formRef}
        className={`w-full ${className}`}
        onSubmit={(event) => {
          event.preventDefault()
          const q = typedQuery()
          if (q.length > 0) {
            void navigate({ to: '/search', search: aiMode ? { q, mode: 'ai' } : { q } })
          }
        }}
      >
        <Autocomplete.InputGroup
          className="group flex h-14 w-full items-center gap-2 rounded-full border border-base-300 bg-base-100 pl-6 pr-2 shadow-sm transition-colors focus-within:border-primary"
        >
          <Autocomplete.Input
            name="q"
            aria-label={t`Search query`}
            placeholder={aiMode ? t`Ask anything privately` : t`Search privately`}
            autoFocus={autoFocus}
            render={
              <input
                type="search"
                className="min-w-0 flex-1 bg-transparent outline-none placeholder:text-base-content/50"
              />
            }
          />
          <div className="join shrink-0 rounded-full bg-base-200 p-1" role="radiogroup" aria-label={t`Mode`}>
            <button
              type="button"
              role="radio"
              aria-checked={!aiMode}
              className={`btn btn-xs join-item rounded-full ${!aiMode ? 'btn-neutral' : 'btn-ghost'}`}
              onClick={() => {
                onModeChange(undefined, typedQuery())
              }}
            >
              <IconSearch aria-hidden="true" />
              <Trans>Search</Trans>
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={aiMode}
              disabled={aiDisabled}
              title={aiDisabled ? t`Configure a model in settings` : undefined}
              className={`btn btn-xs join-item rounded-full ${aiMode ? 'btn-neutral' : 'btn-ghost'}`}
              onClick={() => {
                onModeChange('ai', typedQuery())
              }}
            >
              <IconSparkles aria-hidden="true" />
              <Trans>AI</Trans>
            </button>
          </div>
          <button type="submit" className="btn btn-neutral btn-circle shrink-0 transition-opacity group-has-[input:placeholder-shown]:opacity-40" aria-label={t`Search`}>
            <IconSearch aria-hidden="true" />
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
