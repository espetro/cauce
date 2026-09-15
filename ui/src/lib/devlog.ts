/** Dev-only client-side structured logging for the QA loop.
 * Every call is gated behind import.meta.env.DEV so the production build
 * dead-code-eliminates it entirely (built bundle stays byte-identical). */

const DEV: boolean = import.meta.env.DEV;

export function devLog(event: string, fields: Record<string, unknown> = {}): void {
  if (!DEV) return;
  console.debug(`[oxe] ${event}`, fields);
}

/** Time an async fetch; reports duration_ms + outcome. */
export async function devTimed<T>(
  event: string,
  extra: Record<string, unknown> | undefined,
  fn: () => Promise<T>,
): Promise<T> {
  if (!DEV) return fn();
  const t0 = performance.now();
  try {
    const out = await fn();
    devLog(event, { ...extra, duration_ms: Math.round(performance.now() - t0), ok: true });
    return out;
  } catch (e) {
    devLog(event, {
      ...extra,
      duration_ms: Math.round(performance.now() - t0),
      ok: false,
      error: (e as Error)?.message,
    });
    throw e;
  }
}
