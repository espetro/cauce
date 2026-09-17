/**
 * One-shot settings load on dialog open, as a `useEffect` in the single directory where
 * effects are legal (ui/AGENTS.md Enforced rule). The dialog is an imperative island:
 * the result is non-opaque component state (state ladder rung 1), fetched once per open.
 */
import { useEffect, useState } from 'react'
import { getSettings, type SettingsPayload } from '../settingsApi.ts'

export interface SettingsLoad {
  /** Current settings, null while loading or after a load failure. */
  current: SettingsPayload | null
  /** True once a load attempt failed (the form falls back to blank defaults). */
  failed: boolean
}

/** Load `GET /settings` whenever `open` becomes true; resets when it closes. */
export function useSettingsLoad(open: boolean): SettingsLoad {
  const [state, setState] = useState<SettingsLoad>({ current: null, failed: false })

  useEffect(() => {
    let cancelled = false
    if (!open) {
      // Reset after close, deferred so the effect body never sets state synchronously.
      const id = setTimeout(() => setState({ current: null, failed: false }), 0)
      return () => { cancelled = true; clearTimeout(id) }
    }
    void getSettings()
      .then((current) => { if (!cancelled) setState({ current, failed: false }) })
      .catch(() => { if (!cancelled) setState({ current: null, failed: true }) })
    return () => { cancelled = true }
  }, [open])

  return state
}
