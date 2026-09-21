async page => {
  // Evidence capture for the UI anti-drift loop (~/UILOOP.md). Run via:
  //   playwright-cli --raw run-code --filename=scripts/ui-loop-capture.js
  // run-code has no fs/env: substitute __BASE_URL__, __OUT_DIR__, __AXE_PATH__ with sed, mkdir OUT_DIR first,
  // and redirect stdout (a JSON report) to OUT_DIR/evidence.json.
  const BASE = '__BASE_URL__'
  const OUT = '__OUT_DIR__'
  const AXE_FILE = '__AXE_PATH__'
  const SCREENS = {
    'landing-idle': { path: '/' },
    'search-classic-results': { path: '/search?q=playwright' },
    'search-classic-zero': { path: '/search?q=zzzz&force=empty' },
    'search-classic-error': { path: '/search?q=x&force=error', expect5xx: ['/search'] },
    'ai-unavailable': { path: '/search?q=x&mode=ai&force=ai-off' },
    'ai-stream-failed': { path: '/search?q=x&mode=ai&force=error' },
    'ai-empty-sources': { path: '/search?q=x&mode=ai&force=empty' },
    history: { path: '/history' },
    dashboard: { path: '/dashboard' },
  }
  const VIEWPORTS = { desktop: [1280, 720], tablet: [768, 1024], mobile: [390, 844] }
  const report = []
  const ctx = page.context()
  for (const [name, s] of Object.entries(SCREENS)) {
    for (const [vp, [w, h]] of Object.entries(VIEWPORTS)) {
      const p = await ctx.newPage()
      await p.setViewportSize({ width: w, height: h })
      const errors = []
      const warnings = []
      const fivexx = []
      p.on('console', m => {
        if (m.type() === 'error') errors.push(m.text())
        if (m.type() === 'warning') warnings.push(m.text())
      })
      p.on('response', r => {
        if (r.status() >= 500) fivexx.push(`${r.status()} ${new URL(r.url()).pathname}`)
      })
      await p.goto(BASE + s.path)
      await p.waitForLoadState('load')
      await p.waitForTimeout(2500)
      const shot = `${OUT}/${name}--${vp}.png`
      await p.screenshot({ path: shot })
      await p.addScriptTag({ path: AXE_FILE })
      const axe = await p.evaluate(async () => {
        const r = await window.axe.run()
        return r.violations.map(v => ({ id: v.id, impact: v.impact, nodes: v.nodes.length, help: v.help }))
      })
      const layout = await p.evaluate(() => ({
        overflowX: document.documentElement.scrollWidth - window.innerWidth,
        bodyHeight: document.body.getBoundingClientRect().height,
      }))
      const expected = s.expect5xx ?? []
      const unexpected5xx = fivexx.filter(x => !expected.some(e => x.endsWith(e)))
      const errs = expected.length ? errors.filter(e => !e.includes('status of 5')) : errors
      report.push({
        screen: name, viewport: vp, shot,
        consoleErrors: errs, consoleWarnings: warnings, network5xx: unexpected5xx,
        axeCriticalSerious: axe.filter(v => v.impact === 'critical' || v.impact === 'serious'),
        axeOther: axe.filter(v => v.impact !== 'critical' && v.impact !== 'serious'),
        layoutCollapse: layout.overflowX > 1 || layout.bodyHeight < 50, layout,
      })
      await p.close()
    }
  }
  return JSON.stringify(report)
}
