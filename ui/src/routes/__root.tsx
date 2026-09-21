import { Trans } from '@lingui/react/macro'
import { createRootRoute, Link, Outlet, useLocation } from '@tanstack/react-router'
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
import { APP_VERSION } from '../lib/version.ts'

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
            aria-label={value}
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

type NavTo = '/search' | '/history' | '/dashboard'

/** Bracket-marks the current page's link per landing.md; the landing route `/` marks `search`. */
function NavLink({ to, children }: { to: NavTo; children: React.ReactNode }) {
  const pathname = useLocation({ select: (l) => l.pathname })
  const active = pathname.startsWith(to) || (to === '/search' && pathname === '/')
  return (
    <Link to={to} className="flex items-center gap-1" aria-current={active ? 'page' : undefined}>
      {active && <span aria-hidden="true">[</span>}
      {children}
      {active && <span aria-hidden="true">]</span>}
    </Link>
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
      <header className="navbar flex-wrap gap-y-2 border-b border-base-300 px-4">
        <div className="flex-1 gap-4">
          <Link to="/" className="text-lg font-semibold">
            oxe
          </Link>
          <nav className="flex items-center gap-3 text-sm">
            <NavLink to="/search">
              <IconSearch aria-hidden="true" />
              <Trans>search</Trans>
            </NavLink>
            <NavLink to="/history">
              <IconHistory aria-hidden="true" />
              <Trans>history</Trans>
            </NavLink>
            <NavLink to="/dashboard">
              <IconLayoutDashboard aria-hidden="true" />
              <Trans>dashboard</Trans>
            </NavLink>
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
          <span className="hidden text-xs opacity-70 sm:inline">v{APP_VERSION}</span>
        </div>
      </header>
      <Outlet />
      <SettingsDialog open={settingsOpen} onClose={() => { void navigate({ search: stripSettings }) }} />
    </>
  )
}
