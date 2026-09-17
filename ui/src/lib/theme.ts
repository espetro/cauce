/**
 * Theme store: System / Light / Dark, first of the plan's rung-4 `@xstate/store`
 * global stores (hard cap of 3 -- see ui/AGENTS.md's state ladder).
 *
 * Per `.agents/docs/screens/userflow-checkpoints.md`'s "URL param contract" section, theme
 * is explicitly *not* URL state: it's a cross-session UI preference, not a shareable result,
 * so it stays on rung 2 (`localStorage`) for persistence, fronted by a rung-4 `@xstate/store`
 * for reactive reads/writes from components. The store is the single source of truth at
 * runtime; `localStorage` is only ever read once at module-init time (to hydrate) and written
 * on every change (to persist) -- never read again after hydration, so there's no
 * split-brain between the store and the disk.
 *
 * The checkpoint doc requires "a rung-2 valibot codec for the localStorage value (does not
 * exist yet; needed so a stale stored shape from a prior release fails closed instead of
 * throwing)" -- `themeCodec` below is that codec: an unparseable/legacy value decodes to
 * `'system'` instead of throwing.
 *
 * No `useEffect`: hydration reads `localStorage` synchronously at module init (this file is
 * imported once, before any component mounts), and persistence happens inside the store's
 * `subscribe` callback, not a component lifecycle hook. Both are allowed outside
 * `src/lib/effects/` because neither is a React effect.
 */
import { createStore } from '@xstate/store'
import { useSyncExternalStore } from 'react'
import * as v from 'valibot'

export const THEME_VALUES = ['system', 'light', 'dark'] as const
export type Theme = (typeof THEME_VALUES)[number]

const STORAGE_KEY = 'oxe-theme'

/** Rung-2 valibot codec: an unrecognized/legacy stored value fails closed to `'system'`. */
const themeSchema = v.fallback(v.picklist(THEME_VALUES), 'system')

function decodeTheme(raw: string | null): Theme {
  if (raw === null) {
    return 'system'
  }
  return v.parse(themeSchema, raw)
}

function readInitialTheme(): Theme {
  if (typeof localStorage === 'undefined') {
    return 'system'
  }
  try {
    return decodeTheme(localStorage.getItem(STORAGE_KEY))
  } catch {
    return 'system'
  }
}

/** Applies the theme to the document so daisyUI's `data-theme` selector picks it up.
 * `'system'` clears the attribute entirely, deferring to the `dark --prefersdark` /
 * `light --default` pair declared in `src/index.css` via the OS `prefers-color-scheme`. */
function applyThemeToDocument(theme: Theme): void {
  if (typeof document === 'undefined') {
    return
  }
  if (theme === 'system') {
    document.documentElement.removeAttribute('data-theme')
  } else {
    document.documentElement.setAttribute('data-theme', theme)
  }
}

export const themeStore = createStore({
  context: { theme: readInitialTheme() },
  on: {
    setTheme: (_context, event: { theme: Theme }) => ({ theme: event.theme }),
  },
})

// Persist on every change and keep <html data-theme> in sync. Runs at module scope via the
// store's own subscription mechanism, not a React effect.
themeStore.subscribe((snapshot) => {
  applyThemeToDocument(snapshot.context.theme)
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(STORAGE_KEY, snapshot.context.theme)
  }
})

// Apply the hydrated theme immediately, before first paint of any component.
applyThemeToDocument(themeStore.getSnapshot().context.theme)

export function setTheme(theme: Theme): void {
  themeStore.trigger.setTheme({ theme })
}

/**
 * React binding for `themeStore`. `@xstate/store` 4.2.3 (the pinned version) ships no
 * `/react` subpath with a `useSelector` hook, so this uses React's own
 * `useSyncExternalStore` -- the built-in primitive for subscribing a component to an
 * external store's snapshot. This is not a `useEffect` fetch/subscription: it's the
 * dedicated external-store-subscription hook, so it stays outside `src/lib/effects/`.
 */
export function useTheme(): Theme {
  return useSyncExternalStore(
    (onStoreChange) => themeStore.subscribe(onStoreChange).unsubscribe,
    () => themeStore.getSnapshot().context.theme,
  )
}
