import { Trans } from '@lingui/react/macro'
import { createRootRoute, Link, Outlet } from '@tanstack/react-router'
import IconGithub from '~icons/lucide/github'
import IconHelpCircle from '~icons/lucide/circle-question-mark'
import IconHistory from '~icons/lucide/history'
import IconLayoutDashboard from '~icons/lucide/layout-dashboard'
import IconMonitor from '~icons/lucide/monitor'
import IconMoon from '~icons/lucide/moon'
import IconSearch from '~icons/lucide/search'
import IconSettings from '~icons/lucide/settings'
import IconSun from '~icons/lucide/sun'
import { setTheme, THEME_VALUES, useTheme, type Theme } from '../lib/theme.ts'
import * as v from 'valibot'
import { settingsSchema } from '../lib/routeSearch.ts'
import { SettingsDialog } from '../components/SettingsDialog.tsx'
import { stripSettings } from '../lib/settingsUrl.ts'

export const Route = createRootRoute({
  validateSearch: v.object({ settings: settingsSchema }),
  component: RootComponent,
})

const THEME_ICONS: Record<Theme, typeof IconSun> = {
  system: IconMonitor,
  light: IconSun,
  dark: IconMoon,
}

/**
 * Theme toggle: the header-reachable control the plan's screen specs imply is present on
 * every screen (see `.agents/docs/screens/userflow-checkpoints.md` checkpoint 18's `theme`
 * fieldset, and landing.md's persistent header). Reads/writes `themeStore` (rung 4), which
 * itself persists to `localStorage` (rung 2) behind a valibot codec -- see `src/lib/theme.ts`.
 */
function ThemeToggle() {
  const theme = useTheme()
  return (
    <div className="join" role="radiogroup" aria-label="Theme">
      {THEME_VALUES.map((value) => {
        const ThemeIcon = THEME_ICONS[value]
        return (
          <button
            key={value}
            type="button"
            role="radio"
            aria-checked={theme === value}
            className={`btn btn-xs join-item ${theme === value ? 'btn-active' : ''}`}
            onClick={() => {
              setTheme(value)
            }}
          >
            <ThemeIcon aria-hidden="true" />
          </button>
        )
      })}
    </div>
  )
}

/**
 * Root layout: the persistent header/nav every screen spec assumes is present (landing.md,
 * search.md, history.md, dashboard.md all show the same `oxe / search / history / dashboard
 * / (?) / settings / GitHub / version` bar). This lays out the structural slot only --
 * per-screen active-link styling and the settings dialog's real content are screen-task work,
 * not this foundation task's.
 */
function RootComponent() {
  const search = Route.useSearch()
  const navigate = Route.useNavigate()
  const settingsOpen = search.settings === 'open'
  return (
    <>
      <header className="navbar border-b border-base-300 px-4">
        <div className="flex-1 gap-4">
          <Link to="/" className="text-lg font-semibold">
            oxe
          </Link>
          <nav className="flex items-center gap-3 text-sm">
            <Link to="/" className="flex items-center gap-1" activeOptions={{ exact: true }}>
              <IconSearch aria-hidden="true" />
              <Trans>search</Trans>
            </Link>
            <Link to="/history" className="flex items-center gap-1">
              <IconHistory aria-hidden="true" />
              <Trans>history</Trans>
            </Link>
            <Link to="/dashboard" className="flex items-center gap-1">
              <IconLayoutDashboard aria-hidden="true" />
              <Trans>dashboard</Trans>
            </Link>
          </nav>
        </div>
        <div className="flex items-center gap-3">
          <ThemeToggle />
          <button type="button" className="btn btn-ghost btn-sm btn-circle" aria-label="About">
            <IconHelpCircle aria-hidden="true" />
          </button>
          {/* Settings is a dialog overlay reachable via the `?settings=open` search param on
           * any route (userflow-checkpoints.md checkpoint 18), not its own route file. */}
          <Link
            to="."
            search={{ settings: 'open' }}
            className="btn btn-ghost btn-sm btn-circle"
            aria-label="Settings"
          >
            <IconSettings aria-hidden="true" />
          </Link>
          <a
            href="https://github.com/"
            target="_blank"
            rel="noreferrer"
            className="btn btn-ghost btn-sm btn-circle"
            aria-label="GitHub"
          >
            <IconGithub aria-hidden="true" />
          </a>
        </div>
      </header>
      <Outlet />
      <SettingsDialog open={settingsOpen} onClose={() => { void navigate({ search: stripSettings }) }} />
    </>
  )
}
