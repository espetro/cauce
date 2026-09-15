import type { Mode } from "../../components/ModeSegments";

/** Rebuild /search url from params, preserving anything not derived. */
export function searchUrl(params: {
  q: string;
  page?: number;
  mode?: Mode;
  extra?: Record<string, string>;
}): string {
  const sp = new URLSearchParams();
  sp.set("q", params.q);
  if ((params.page ?? 1) > 1) sp.set("p", String(params.page));
  if (params.mode === "ai") sp.set("mode", "ai");
  for (const [k, v] of Object.entries(params.extra ?? {})) sp.set(k, v);
  return `/search?${sp.toString()}`;
}
