/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * Backend reachability store + `/health` probe. The SPA must stay
 * useful when the API is absent — on the CF Pages split deploy the
 * assets can outlive (or predate) the backend entirely — so this
 * module owns the single source of truth for "is the API up" and
 * surfaces transitions through the sonner toaster (ui/toaster).
 *
 * Two feeds keep `health` current: `probeBackend()` polls `/health`
 * (rate-limit exempt, works through the Pages proxy function), and
 * `noteApiCall()` piggybacks on every `/api/*` outcome via api.ts's
 * `apiFetch` — a failed call degrades faster than the next poll would.
 */

import { toast } from "svelte-sonner";
import { spa } from "./i18n.js";

export type BackendHealth = "unknown" | "ok" | "degraded" | "offline";

export interface BackendState {
  health: BackendHealth;
  /** `cauce --version` as `/health` reports it — informational. */
  version: string;
}

export const backend = $state<BackendState>({ health: "unknown", version: "" });

const TOAST_ID = "backend-health";
const PROBE_TIMEOUT_MS = 4_000;
/** Poll cadence while healthy — cheap insurance for deploys/migrations. */
const POLL_HEALTHY_MS = 60_000;
/** Retry cadence while down or degraded. */
const POLL_DOWN_MS = 15_000;

let timer: ReturnType<typeof setTimeout> | null = null;
let started = false;

/**
 * Record the outcome of one `/api/*` call. `null` = the fetch itself
 * threw (offline); a JSON 5xx leaves the API reachable but degraded
 * while a non-JSON 5xx is an edge/proxy page — offline; any `res.ok`
 * call proves the backend is up even mid-"degraded" probe (the health
 * endpoint checks store connectivity, not API health).
 */
export function noteApiCall(res: Response | null): void {
  if (res === null) setHealth("offline");
  else if (res.status >= 500) {
    // A JSON 5xx is the API's own error body — reachable but degraded;
    // a non-JSON 5xx is an edge/proxy error page, i.e. the origin is
    // unreachable and "offline" describes it better.
    setHealth(isJson(res) ? "degraded" : "offline");
  } else if (res.ok) setHealth("ok");
}

function isJson(res: Response): boolean {
  return (res.headers.get("content-type") ?? "").includes("json");
}

/**
 * One `/health` probe. 200 `{status:"ok"}` → ok; a 503 carrying the
 * health JSON → degraded (the API reports store trouble); any other
 * non-ok → offline (the split deploy's proxy only 5xxs when the origin
 * itself is unreachable) — including a 200 that isn't the health JSON
 * (the Pages SPA fallback answering `/health` means the proxy
 * functions are not deployed, i.e. no backend path exists).
 */
export async function probeBackend(): Promise<BackendHealth> {
  let next: BackendHealth;
  try {
    const res = await fetch("/health", {
      headers: { Accept: "application/json" },
      signal: AbortSignal.timeout(PROBE_TIMEOUT_MS),
    });
    if (res.ok) {
      const body = (await res.json().catch(() => null)) as {
        status?: string;
        version?: string;
      } | null;
      if (body?.status === "ok") setHealth("ok", body.version ?? "");
      else setHealth("offline");
      return backend.health;
    }
    // A 503 carrying the health JSON is the API reporting store trouble
    // (degraded); a non-JSON 5xx is an edge/proxy page — offline.
    next = res.status === 503 && isJson(res) ? "degraded" : "offline";
  } catch {
    next = "offline";
  }
  setHealth(next);
  return next;
}

/**
 * Apply a transition and toast on change. First-ever probe results
 * toast only on failure — a healthy boot stays silent; recovery after
 * a bad state gets a brief confirmation.
 */
function setHealth(next: BackendHealth, version = ""): void {
  const prev = backend.health;
  backend.health = next;
  if (version) backend.version = version;
  if (next === prev) return;

  if (next === "ok") {
    toast.dismiss(TOAST_ID);
    if (prev === "offline" || prev === "degraded") {
      toast.success(spa.app.backend_back, { duration: 4_000 });
    }
    return;
  }
  if (next === "degraded" || next === "offline") {
    const notify = next === "offline" ? toast.error : toast.warning;
    notify(
      next === "offline" ? spa.app.backend_down_title : spa.app.backend_degraded_title,
      {
        id: TOAST_ID,
        description:
          next === "offline" ? spa.app.backend_down_desc : spa.app.backend_degraded_desc,
        duration: Number.POSITIVE_INFINITY,
        action: { label: spa.app.backend_retry, onClick: () => void probeBackend() },
        dismissible: true,
      },
    );
  }
}

/**
 * Start the probe loop: immediate check, then a cadence that slows to
 * a background heartbeat while healthy and retries quickly while down.
 * Browser `online`/`offline`/visibility events re-probe early. Returns
 * a teardown for tests/hmr; idempotent for the app mount.
 */
export function startBackendProbe(): () => void {
  if (started || typeof window === "undefined") return () => {};
  started = true;

  const tick = async () => {
    await probeBackend();
    timer = setTimeout(tick, backend.health === "ok" ? POLL_HEALTHY_MS : POLL_DOWN_MS);
  };
  void tick();

  const onOnline = () => void probeBackend();
  const onOffline = () => setHealth("offline");
  const onVisible = () => {
    if (document.visibilityState === "visible") void probeBackend();
  };
  window.addEventListener("online", onOnline);
  window.addEventListener("offline", onOffline);
  document.addEventListener("visibilitychange", onVisible);

  return () => {
    started = false;
    if (timer !== null) clearTimeout(timer);
    timer = null;
    window.removeEventListener("online", onOnline);
    window.removeEventListener("offline", onOffline);
    document.removeEventListener("visibilitychange", onVisible);
  };
}
