/** Hand-written valibot schemas for the JSON endpoints the FE calls.
 * Each response schema is bound via `satisfies v.GenericSchema<T>` to the
 * generated OpenAPI type (./generated/types.gen.ts, regenerated from the
 * repo-root openapi.json with `bun run gen:types`) so wire drift breaks tsc.
 * Wire honesty: fresh cache misses now serialize explicit nulls, so
 * `_`-fields are `nullish()` (undefined OR null), not just optional. */
import * as v from "valibot";
import type { components } from "./generated/types.gen";

type SearchResponseDto = components["schemas"]["SearchResponse"];
type ApiHistoryResponseDto = components["schemas"]["ApiHistoryResponse"];
type CacheStatsDto = components["schemas"]["CacheStatsResponse"];
type HealthDto = components["schemas"]["HealthResponse"];
type ModelsResponseDto = components["schemas"]["ModelsResponse"];
type SettingsResponseDto = components["schemas"]["SettingsResponse"];
type ApiStatsDto = components["schemas"]["ApiStatsResponse"];
type SettingsTestDto = components["schemas"]["SettingsTestResponse"];

// ---- shared building blocks ----

const str = v.string();
const num = v.number();
const nullishStr = v.nullish(str);
const nullishNum = v.nullish(num);

/** result row: title/url stay non-null (the app renders them), the rest
 * follow the wire (string | null | absent). */
const SearchResultSchema = v.object({
  title: v.nullish(str),
  url: v.nullish(str),
  id: v.nullish(str),
  text: v.nullish(str),
  highlights: v.nullish(v.array(v.unknown())),
  favicon: v.nullish(str),
  publishedDate: v.nullish(str),
  author: v.nullish(str),
  image: v.nullish(str),
});

// ---- endpoint response schemas ----

export const SearchResponseSchema = v.object({
  requestId: v.nullish(str),
  searchType: v.nullish(str),
  results: v.array(SearchResultSchema),
  costDollars: v.nullish(v.object({ total: v.optional(num) })),
  // cache-transparency `_` fields: explicit nulls on fresh misses
  _source: nullishStr,
  _q_hash: nullishStr,
  _q: nullishStr,
  _backend: nullishStr,
  _duration_ms: nullishNum,
  _cached_at: nullishNum,
  _error: nullishStr,
  _error_kind: nullishStr,
}) satisfies v.GenericSchema<SearchResponseDto>;

export type SearchResponse = v.InferOutput<typeof SearchResponseSchema>;
export type SearchResult = v.InferOutput<typeof SearchResponseSchema>["results"][number];

export const HistoryItemSchema = v.variant("kind", [
  v.object({
    kind: v.literal("click"),
    clicked_at: num,
    query_hash: str,
    query: str,
    result_id: str,
    url: str,
    title: str,
    source: str,
  }),
  v.object({
    kind: v.literal("cache"),
    created_at: num,
    expires_at: num,
    query_hash: str,
    query: str,
    hits: num,
    size_bytes: num,
  }),
]);

export const HistoryResponseSchema = v.object({
  items: v.array(HistoryItemSchema),
  clicks: num,
  cache_rows: num,
  limit: num,
  since: str,
}) satisfies v.GenericSchema<ApiHistoryResponseDto>;

export type HistoryResponse = v.InferOutput<typeof HistoryResponseSchema>;
export type HistoryItem = HistoryResponse["items"][number];
/** FE display shape: `sort_at` derived client-side (backend pops it pre-wire). */
export type HistoryRow = HistoryItem & { sort_at: number };

/** CacheStats as FE-consumed; also reused (loosely) inside ApiStats. */
export const CacheStatsSchema = v.object({
  rows: num,
  unexpired_rows: num,
  db_size_bytes: num,
  total_hits: num,
  oldest_unexpired: nullishNum,
  newest: nullishNum,
}) satisfies v.GenericSchema<CacheStatsDto>;

export type CacheStats = v.InferOutput<typeof CacheStatsSchema>;

export const HealthSchema = v.object({
  status: str,
  service: str,
  cache_size: num,
  version: str,
  pid: num,
}) satisfies v.GenericSchema<HealthDto>;

export type Health = v.InferOutput<typeof HealthSchema>;

export const ModelsResponseSchema = v.object({
  object: str,
  data: v.array(v.object({ id: str })),
  ai_available: v.boolean(),
  error: nullishStr,
}) satisfies v.GenericSchema<ModelsResponseDto>;

export type ModelsResponse = v.InferOutput<typeof ModelsResponseSchema>;

export const SettingsGetSchema = v.object({
  configured: v.boolean(),
  config_path: str,
  ai: v.nullish(
    v.object({
      provider: str,
      model: str,
      base_url: nullishStr,
      enabled: v.boolean(),
      api_key_set: v.boolean(),
      api_key_env: nullishStr,
    }),
  ),
}) satisfies v.GenericSchema<SettingsResponseDto>;

export type SettingsGet = v.InferOutput<typeof SettingsGetSchema>;

export const ApiStatsSchema = v.object({
  days: num,
  searches_per_day: v.optional(
    v.array(v.object({ day: str, cache: num, network: num, total: num })),
    [],
  ),
  hit_rate: v.optional(v.object({ total: num, cache_hits: num, rate: nullishNum }), {
    total: 0,
    cache_hits: 0,
    rate: null,
  }),
  latency_ms: v.optional(v.object({ p50: nullishNum, p90: nullishNum, p99: nullishNum }), {
    p50: null,
    p90: null,
    p99: null,
  }),
  top_queries: v.optional(v.array(v.object({ query: str, count: num })), []),
  zero_result_queries: v.optional(v.array(v.object({ query: str, last_seen: num })), []),
  client_split: v.optional(v.array(v.object({ client: str, count: num })), []),
  cache: v.optional(CacheStatsSchema, {
    rows: 0,
    unexpired_rows: 0,
    db_size_bytes: 0,
    total_hits: 0,
    oldest_unexpired: null,
    newest: null,
  }),
}) satisfies v.GenericSchema<ApiStatsDto>;

export type ApiStats = v.InferOutput<typeof ApiStatsSchema>;

/** POST /settings/test reply. */
export const SettingsTestSchema = v.object({
  ok: v.boolean(),
  detail: str,
}) satisfies v.GenericSchema<SettingsTestDto>;

export type SettingsTest = v.InferOutput<typeof SettingsTestSchema>;

/** PUT /settings reply ({ok, config_path}). */
export const SettingsPutSchema = v.object({ ok: v.boolean(), config_path: str });

// ---- SSE answer events (mirror oxe/ai.py emit sites) ----

export const AiSourceSchema = v.object({
  title: v.nullish(str),
  url: str,
  favicon: nullishStr,
});

export type AiSource = v.InferOutput<typeof AiSourceSchema>;

export const AnswerEventSchema = v.variant("type", [
  v.object({ type: v.literal("step"), tool: str, query: str, label: str }),
  v.object({ type: v.literal("delta"), text: str }),
  v.object({ type: v.literal("sources"), sources: v.array(AiSourceSchema) }),
  v.object({
    type: v.literal("done"),
    answer: str,
    related_questions: v.array(str),
    confidence: num,
    model: v.optional(str),
    cached: v.optional(v.boolean()),
    error: v.optional(str),
    sources: v.optional(v.array(AiSourceSchema)),
  }),
]);

export type AnswerEvent = v.InferOutput<typeof AnswerEventSchema>;
