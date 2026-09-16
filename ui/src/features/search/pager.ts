import type { Mode } from "../../components/ModeSegments";

/** Rebuild /search url from params, preserving anything not derived.
 * The `p` (page) param is deprecated: continuous scroll owns pagination,
 * generated links never carry it, and deep links that do are ignored. */
export function searchUrl(params: {
  q: string;
  mode?: Mode;
  extra?: Record<string, string>;
}): string {
  const sp = new URLSearchParams();
  sp.set("q", params.q);
  if (params.mode === "ai") sp.set("mode", "ai");
  for (const [k, v] of Object.entries(params.extra ?? {})) sp.set(k, v);
  return `/search?${sp.toString()}`;
}
