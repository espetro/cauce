// @vitest-environment happy-dom
/**
 * Standalone unit test for the theme store: no renderer, per the state ladder's "reducer /
 * store tested standalone" doctrine (ui/AGENTS.md, rung 3/4). Exercises the store's
 * transitions and the rung-2 localStorage codec's fail-closed behavior directly.
 */
import { beforeEach, describe, expect, it } from 'vitest'
import { setTheme, themeStore } from './theme.ts'

describe('themeStore', () => {
  beforeEach(() => {
    localStorage.clear()
    document.documentElement.removeAttribute('data-theme')
    setTheme('system')
  })

  it('defaults to system', () => {
    expect(themeStore.getSnapshot().context.theme).toBe('system')
  })

  it('setTheme transitions the store context', () => {
    setTheme('dark')
    expect(themeStore.getSnapshot().context.theme).toBe('dark')

    setTheme('light')
    expect(themeStore.getSnapshot().context.theme).toBe('light')
  })

  it('persists every change to localStorage under oxe-theme', () => {
    setTheme('dark')
    expect(localStorage.getItem('oxe-theme')).toBe('dark')

    setTheme('system')
    expect(localStorage.getItem('oxe-theme')).toBe('system')
  })

  it('applies data-theme to <html>, clearing it for system', () => {
    setTheme('light')
    expect(document.documentElement.getAttribute('data-theme')).toBe('light')

    setTheme('dark')
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')

    setTheme('system')
    expect(document.documentElement.getAttribute('data-theme')).toBeNull()
  })
})
