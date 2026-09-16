import prerender, { locationStub } from "preact-iso/prerender";
import { ErrorBoundary, LocationProvider, Route, Router, lazy, useLocation } from "preact-iso";
import { Component, toChildArray } from "preact";
import { useCallback, useEffect, useId, useRef, useState } from "preact/hooks";
import * as v from "valibot";
import { Fragment, jsx, jsxs } from "preact/jsx-runtime";
import { WindowVirtualizer } from "virtua";
//#region \0rolldown/runtime.js
var __defProp = Object.defineProperty;
var __esmMin = (fn, res, err) => () => {
	if (err) throw err[0];
	try {
		return fn && (res = fn(fn = 0)), res;
	} catch (e) {
		throw err = [e], e;
	}
};
var __exportAll = (all, no_symbols) => {
	let target = {};
	for (var name in all) __defProp(target, name, {
		get: all[name],
		enumerable: true
	});
	if (!no_symbols) __defProp(target, Symbol.toStringTag, { value: "Module" });
	return target;
};
/** Time an async fetch; reports duration_ms + outcome. */
async function devTimed(event, extra, fn) {
	return fn();
}
var init_devlog = __esmMin((() => {})), str, num, nullishStr, nullishNum, SearchResultSchema, SearchResponseSchema, HistoryItemSchema, HistoryResponseSchema, CacheStatsSchema, ModelsResponseSchema, SettingsGetSchema, ApiStatsSchema, SettingsTestSchema, SettingsPutSchema, AiSourceSchema, AnswerEventSchema;
var init_schemas = __esmMin((() => {
	str = v.string();
	num = v.number();
	nullishStr = v.nullish(str);
	nullishNum = v.nullish(num);
	SearchResultSchema = v.object({
		title: v.nullish(str),
		url: v.nullish(str),
		id: v.nullish(str),
		text: v.nullish(str),
		highlights: v.nullish(v.array(v.unknown())),
		favicon: v.nullish(str),
		publishedDate: v.nullish(str),
		author: v.nullish(str),
		image: v.nullish(str)
	});
	SearchResponseSchema = v.object({
		requestId: v.nullish(str),
		searchType: v.nullish(str),
		results: v.array(SearchResultSchema),
		costDollars: v.nullish(v.object({ total: v.optional(num) })),
		_source: nullishStr,
		_q_hash: nullishStr,
		_q: nullishStr,
		_backend: nullishStr,
		_duration_ms: nullishNum,
		_cached_at: nullishNum,
		_error: nullishStr,
		_error_kind: nullishStr
	});
	HistoryItemSchema = v.variant("kind", [v.object({
		kind: v.literal("click"),
		clicked_at: num,
		query_hash: str,
		query: str,
		result_id: str,
		url: str,
		title: str,
		source: str
	}), v.object({
		kind: v.literal("cache"),
		created_at: num,
		expires_at: num,
		query_hash: str,
		query: str,
		hits: num,
		size_bytes: num
	})]);
	HistoryResponseSchema = v.object({
		items: v.array(HistoryItemSchema),
		clicks: num,
		cache_rows: num,
		limit: num,
		since: str
	});
	CacheStatsSchema = v.object({
		rows: num,
		unexpired_rows: num,
		db_size_bytes: num,
		total_hits: num,
		oldest_unexpired: nullishNum,
		newest: nullishNum
	});
	v.object({
		status: str,
		service: str,
		cache_size: num,
		version: str,
		pid: num
	});
	ModelsResponseSchema = v.object({
		object: str,
		data: v.array(v.object({ id: str })),
		ai_available: v.boolean(),
		error: nullishStr
	});
	SettingsGetSchema = v.object({
		configured: v.boolean(),
		config_path: str,
		ai: v.nullish(v.object({
			provider: str,
			model: str,
			base_url: nullishStr,
			enabled: v.boolean(),
			api_key_set: v.boolean(),
			api_key_env: nullishStr
		}))
	});
	ApiStatsSchema = v.object({
		days: num,
		searches_per_day: v.optional(v.array(v.object({
			day: str,
			cache: num,
			network: num,
			total: num
		})), []),
		hit_rate: v.optional(v.object({
			total: num,
			cache_hits: num,
			rate: nullishNum
		}), {
			total: 0,
			cache_hits: 0,
			rate: null
		}),
		latency_ms: v.optional(v.object({
			p50: nullishNum,
			p90: nullishNum,
			p99: nullishNum
		}), {
			p50: null,
			p90: null,
			p99: null
		}),
		top_queries: v.optional(v.array(v.object({
			query: str,
			count: num
		})), []),
		zero_result_queries: v.optional(v.array(v.object({
			query: str,
			last_seen: num
		})), []),
		client_split: v.optional(v.array(v.object({
			client: str,
			count: num
		})), []),
		cache: v.optional(CacheStatsSchema, {
			rows: 0,
			unexpired_rows: 0,
			db_size_bytes: 0,
			total_hits: 0,
			oldest_unexpired: null,
			newest: null
		})
	});
	SettingsTestSchema = v.object({
		ok: v.boolean(),
		detail: str
	});
	SettingsPutSchema = v.object({
		ok: v.boolean(),
		config_path: str
	});
	AiSourceSchema = v.object({
		title: v.nullish(str),
		url: str,
		favicon: nullishStr
	});
	AnswerEventSchema = v.variant("type", [
		v.object({
			type: v.literal("step"),
			tool: str,
			query: str,
			label: str
		}),
		v.object({
			type: v.literal("delta"),
			text: str
		}),
		v.object({
			type: v.literal("sources"),
			sources: v.array(AiSourceSchema)
		}),
		v.object({
			type: v.literal("done"),
			answer: str,
			related_questions: v.array(str),
			confidence: num,
			model: v.optional(str),
			cached: v.optional(v.boolean()),
			error: v.optional(str),
			sources: v.optional(v.array(AiSourceSchema))
		})
	]);
}));
//#endregion
//#region src/lib/api.ts
async function request(url, schema, opts = {}) {
	let res;
	try {
		res = await fetch(`${BASE}${url}`, {
			method: opts.method ?? "GET",
			headers: {
				...opts.body !== void 0 ? { "Content-Type": "application/json" } : {},
				Accept: opts.accept ?? "application/json"
			},
			...opts.body !== void 0 ? { body: JSON.stringify(opts.body) } : {},
			signal: opts.signal
		});
	} catch (e) {
		if (e?.name === "AbortError") throw e;
		throw new ApiError("network_error", e?.message ?? "network error", 0);
	}
	if (!res.ok) {
		let code = "http_error";
		let message = `HTTP ${res.status}`;
		try {
			const envelope = v.parse(ErrorEnvelopeSchema, await res.json());
			code = envelope.error.code;
			message = envelope.error.message;
		} catch {}
		throw new ApiError(code, message, res.status);
	}
	return v.parse(schema, await res.json());
}
async function search(req, signal) {
	const out = await devTimed("search", {
		q: req.query,
		page: req.page ?? 1
	}, () => request("/search", SearchResponseSchema, {
		method: "POST",
		body: {
			numResults: 10,
			contents: {
				text: true,
				highlights: true
			},
			...req,
			...req.page != null && req.page > 1 ? { page: req.page } : {}
		},
		signal
	}));
	req.query, out._source, out.results?.length, out._duration_ms;
	return out;
}
async function recordClick(payload) {
	try {
		const body = JSON.stringify({
			source: "web-ui",
			...payload
		});
		if (navigator.sendBeacon) {
			navigator.sendBeacon(`${BASE}/click`, new Blob([body], { type: "application/json" }));
			return;
		}
		await fetch(`${BASE}/click`, {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body,
			keepalive: true
		});
	} catch {}
}
/** DDG autocomplete via the backend proxy: JSON array of phrases, max 6. */
async function ddgAc(q, signal) {
	const res = await devTimed("ac", { q }, () => fetch(`${BASE}/ac?q=${encodeURIComponent(q)}`, { signal }));
	if (!res.ok) return [];
	const out = await res.json().catch(() => []);
	const items = Array.isArray(out) ? out : [];
	items.length;
	return items;
}
/** OpenSearch suggestions: ["prefix", ["s1", ...], [], []] */
async function suggest(q, signal) {
	const res = await devTimed("suggest", { q }, () => fetch(`${BASE}/suggest?q=${encodeURIComponent(q)}`, { signal }));
	if (!res.ok) return [];
	const out = (await res.json().catch(() => null))?.[1] ?? [];
	out.length;
	return out;
}
/** Delete a single cache row by its query hash (POST /row/{key}/delete).
* Note: the backend route redirects (303) on success; we only check status. */
async function deleteCacheRow(key) {
	try {
		return (await fetch(`${BASE}/row/${encodeURIComponent(key)}/delete`, {
			method: "POST",
			headers: { Accept: "application/json" }
		})).ok;
	} catch {
		return false;
	}
}
async function fetchApiHistory(since, qText, signal) {
	const p = new URLSearchParams({ since: SINCE_TO_BACKEND[since] });
	if (qText) p.set("q", qText);
	return request(`/api/history?${p.toString()}`, HistoryResponseSchema, { signal });
}
/** POST /history/delete: prune click history by scope.
* Backend replies {"ok": true, "deleted": n} for JSON clients. */
async function deleteHistory(scope) {
	return (await request("/history/delete", v.object({
		ok: v.optional(v.boolean()),
		deleted: v.number()
	}), {
		method: "POST",
		body: { scope }
	})).deleted;
}
/** GET /api/stats: search-log aggregates for the dashboard plus cache
* stats. Params: days=1..90 (default 14). */
function apiStats(signal) {
	return request("/api/stats", ApiStatsSchema, { signal });
}
var BASE, ApiError, ErrorEnvelopeSchema, SINCE_TO_BACKEND;
var init_api = __esmMin((() => {
	init_devlog();
	init_schemas();
	BASE = "";
	ApiError = class extends Error {
		code;
		status;
		constructor(code, message, status) {
			super(message);
			this.name = "ApiError";
			this.code = code;
			this.status = status;
		}
	};
	ErrorEnvelopeSchema = v.object({ error: v.object({
		code: v.string(),
		message: v.string()
	}) });
	SINCE_TO_BACKEND = {
		"24": "24h",
		"168": "7d",
		"720": "30d",
		all: "all"
	};
}));
//#endregion
//#region src/lib/ai.ts
/** AI-mode endpoint clients: /v1/models, /settings/test, /answer (SSE).
* Response schemas + AnswerEvent live in ./schemas.ts (bound to the
* generated OpenAPI types). */
/** Verify AI config with POST /settings/test (provider + key + model). */
async function testConnection(body) {
	return request("/settings/test", SettingsTestSchema, {
		method: "POST",
		body: { ai: body }
	});
}
async function listModels(signal) {
	return request("/v1/models", ModelsResponseSchema, { signal });
}
/** Consume the /answer SSE stream via chunked fetch. Calls `on` per event.
* Malformed frames are skipped (try/catch), well-formed frames are
* type-narrowed through AnswerEventSchema. */
async function streamAnswer(query, on, signal) {
	const res = await fetch(`/answer`, {
		method: "POST",
		headers: {
			"Content-Type": "application/json",
			Accept: "text/event-stream"
		},
		body: JSON.stringify({ query }),
		signal
	});
	if (!res.ok || !res.body) {
		let code = "http_error";
		let detail = `HTTP ${res.status}`;
		try {
			const envelope = v.parse(v.object({ error: v.object({
				code: v.string(),
				message: v.string()
			}) }), await res.json());
			code = envelope.error.code;
			detail = envelope.error.message;
		} catch {}
		throw new ApiError(code, detail, res.status);
	}
	const reader = res.body.getReader();
	const decoder = new TextDecoder();
	let buf = "";
	for (;;) {
		const { done, value } = await reader.read();
		if (done) break;
		buf += decoder.decode(value, { stream: true });
		let idx;
		while ((idx = buf.indexOf("\n\n")) !== -1) {
			const frame = buf.slice(0, idx);
			buf = buf.slice(idx + 2);
			for (const line of frame.split("\n")) {
				if (!line.startsWith("data: ")) continue;
				try {
					const ev = v.parse(AnswerEventSchema, JSON.parse(line.slice(6)));
					ev.type;
					on(ev);
				} catch {}
			}
		}
	}
}
var init_ai = __esmMin((() => {
	init_api();
	init_schemas();
}));
//#endregion
//#region src/lib/useMountEffect.ts
/** Escape hatch for one-time external sync on mount (setup + cleanup).
* Wraps useEffect with an empty dependency array to make intent explicit. */
function useMountEffect(effect) {
	useEffect(effect, []);
}
var init_useMountEffect = __esmMin((() => {}));
//#endregion
//#region src/components/ModelPicker.tsx
function bumpModels() {
	modelsVersion += 1;
	document.dispatchEvent(new CustomEvent("oxe-models-bump"));
}
function onModelsBump(cb) {
	const handler = () => cb();
	document.addEventListener("oxe-models-bump", handler);
	return () => document.removeEventListener("oxe-models-bump", handler);
}
function currentModelsVersion() {
	return modelsVersion;
}
/** Filterable model combobox (SRP: pick one model from a large list).
* Text input filters, click/focus opens, Enter selects, Escape closes;
* aria-combobox semantics; list is max-height + scroll (446-model lists). */
function ModelPicker({ models, value, onChange, disabled, id, size = "sm", label = "AI model", modelsError }) {
	const [open, setOpen] = useState(false);
	const [filter, setFilter] = useState("");
	const [active, setActive] = useState(0);
	const rootRef = useRef(null);
	const inputRef = useRef(null);
	const listId = `model-picker-list-${useId()}`;
	const filterLc = filter.trim().toLowerCase();
	const filtered = (filterLc ? models.filter((m) => m.toLowerCase().includes(filterLc)) : models).slice(0, 50);
	useMountEffect(function closeOnOutsideClick() {
		const onDocClick = (e) => {
			if (rootRef.current && !rootRef.current.contains(e.target)) setOpen(false);
		};
		document.addEventListener("mousedown", onDocClick);
		return () => document.removeEventListener("mousedown", onDocClick);
	});
	const pick = (m) => {
		onChange(m);
		setOpen(false);
		setFilter("");
		inputRef.current?.blur();
	};
	/** Commit typed text on blur: exact (case-insensitive) match against the
	* model list selects that model; anything else is a custom model id. */
	const commit = () => {
		const t = filter.trim();
		if (!t || t === value) {
			setOpen(false);
			setFilter("");
			return;
		}
		const exact = models.find((m) => m.toLowerCase() === t.toLowerCase());
		pick(exact ?? t);
	};
	return /* @__PURE__ */ jsxs("div", {
		class: "relative",
		ref: rootRef,
		children: [
			/* @__PURE__ */ jsxs("div", {
				role: "combobox",
				"aria-expanded": open,
				"aria-haspopup": "listbox",
				"aria-owns": listId,
				children: [/* @__PURE__ */ jsx("input", {
					ref: inputRef,
					id,
					type: "text",
					role: "searchbox",
					"aria-label": label,
					"aria-autocomplete": "list",
					"aria-controls": listId,
					autocomplete: "off",
					class: `input ${size === "xs" ? "select-xs" : "select-sm"} w-full pr-6 min-w-0`,
					value: open ? filter : value,
					placeholder: value || "filter models…",
					disabled,
					onInput: (e) => {
						const v = e.target.value;
						setFilter(v);
						setOpen(true);
						setActive(0);
					},
					onFocus: () => {
						setFilter("");
						setOpen(true);
						setActive(0);
					},
					onBlur: commit,
					onKeyDown: (e) => {
						const ke = e;
						if (ke.key === "ArrowDown") {
							e.preventDefault();
							setOpen(true);
							setActive((i) => Math.min(i + 1, filtered.length - 1));
						} else if (ke.key === "ArrowUp") {
							e.preventDefault();
							setActive((i) => Math.max(i - 1, 0));
						} else if (ke.key === "Enter" && open && filtered[active]) {
							e.preventDefault();
							pick(filtered[active]);
						} else if (ke.key === "Escape") {
							setOpen(false);
							setFilter("");
						}
					}
				}), /* @__PURE__ */ jsx("span", {
					class: "absolute right-2 top-1/2 -translate-y-1/2 text-[10px] opacity-40 pointer-events-none select-none",
					"aria-hidden": "true",
					children: "▾"
				})]
			}),
			open && filtered.length === 0 && /* @__PURE__ */ jsx("ul", {
				id: listId,
				role: "listbox",
				"aria-label": label,
				class: "absolute left-0 right-0 top-full mt-1 z-50 bg-base-100 border border-base-300 rounded-md shadow-sm py-2 text-xs m-0 list-none p-0",
				children: /* @__PURE__ */ jsx("li", {
					role: "option",
					"aria-selected": false,
					"aria-disabled": "true",
					class: "px-3 opacity-60",
					children: modelsError ? `model listing failed: ${modelsError}` : "no models - check provider / API key in settings"
				})
			}),
			open && filtered.length > 0 && /* @__PURE__ */ jsx("ul", {
				id: listId,
				role: "listbox",
				"aria-label": label,
				class: "absolute left-0 right-0 top-full mt-1 z-50 bg-base-100 border border-base-300 rounded-md shadow-sm py-1 text-xs m-0 list-none p-0 max-h-64 overflow-y-auto",
				children: filtered.map((m, i) => /* @__PURE__ */ jsx("li", {
					role: "option",
					"aria-selected": m === value,
					class: `px-3 py-1.5 cursor-pointer truncate ${i === active ? "bg-base-200" : ""}`,
					onMouseDown: (e) => {
						e.preventDefault();
						pick(m);
					},
					onMouseEnter: () => setActive(i),
					children: m
				}, m))
			})
		]
	});
}
var modelsVersion;
var init_ModelPicker = __esmMin((() => {
	init_useMountEffect();
	modelsVersion = 0;
}));
//#endregion
//#region src/components/Toasts.tsx
function emit() {
	for (const fn of subs) fn(toasts);
}
/** Push a toast; auto-dismissed after 4s (timer cleared on dismiss). */
function toast(type, msg) {
	const t = {
		id: nextId++,
		type,
		msg
	};
	toasts = [...toasts, t];
	emit();
	timers.set(t.id, setTimeout(() => dismiss(t.id), 4e3));
}
function dismiss(id) {
	const timer = timers.get(id);
	if (timer) {
		clearTimeout(timer);
		timers.delete(id);
	}
	toasts = toasts.filter((t) => t.id !== id);
	emit();
}
/** Fixed daisyUI toast stack (bottom-end). Mount once, next to the Header. */
function Toasts() {
	const [list, setList] = useState(toasts);
	useMountEffect(function subscribeToToasts() {
		subs.add(setList);
		return () => {
			subs.delete(setList);
		};
	});
	const alertClass = (t) => t.type === "success" ? "alert-success" : t.type === "error" ? "alert-error" : "alert-info";
	return /* @__PURE__ */ jsx("div", {
		class: "toast toast-end toast-bottom z-50",
		children: list.map((t) => /* @__PURE__ */ jsx("div", {
			role: "status",
			class: `alert ${alertClass(t)} text-sm py-2 animate-in fade-in slide-in-from-bottom-2 duration-300`,
			onClick: () => dismiss(t.id),
			children: /* @__PURE__ */ jsx("span", { children: t.msg })
		}, t.id))
	});
}
var toasts, nextId, subs, timers;
var init_Toasts = __esmMin((() => {
	init_useMountEffect();
	toasts = [];
	nextId = 1;
	subs = /* @__PURE__ */ new Set();
	timers = /* @__PURE__ */ new Map();
}));
//#endregion
//#region src/lib/theme.ts
/** Stored theme choice (default system). */
function getTheme() {
	const v = localStorage.getItem(THEME_KEY);
	return v === "light" || v === "dark" ? v : "system";
}
function setTheme(choice) {
	localStorage.setItem(THEME_KEY, choice);
	applyTheme(choice);
}
/** Apply via daisyUI `data-theme`; system removes the attr so the
* `color-scheme: light dark` media behavior takes over (prefersdark). */
function applyTheme(choice) {
	if (choice === "system") document.documentElement.removeAttribute("data-theme");
	else document.documentElement.setAttribute("data-theme", choice);
}
var THEME_KEY, THEMES;
var init_theme = __esmMin((() => {
	THEME_KEY = "oxe-theme";
	THEMES = [
		"system",
		"light",
		"dark"
	];
}));
//#endregion
//#region src/features/settings/schema.ts
async function getSettings(signal) {
	return request("/settings", SettingsGetSchema, { signal });
}
async function putSettings(body) {
	await request("/settings", SettingsPutSchema, {
		method: "PUT",
		body: { ai: body }
	});
}
var SettingsSchema, PROVIDERS;
var init_schema = __esmMin((() => {
	init_api();
	init_schemas();
	SettingsSchema = v.object({
		provider: v.picklist([
			"openai",
			"anthropic",
			"groq",
			"mistral"
		], "pick a provider"),
		model: v.pipe(v.string(), v.trim(), v.nonEmpty("model is required")),
		api_key: v.optional(v.pipe(v.string(), v.trim())),
		base_url: v.optional(v.pipe(v.string(), v.trim(), v.url("must be a valid url"))),
		enabled: v.boolean()
	});
	PROVIDERS = [
		"openai",
		"anthropic",
		"groq",
		"mistral"
	];
}));
//#endregion
//#region src/features/settings/SettingsDialog.tsx
/** Settings dialog: native <dialog class="modal"> + <form method="dialog">.
* Reads/writes the backend [ai] config via GET/PUT /settings; parse-on-submit
* via valibot (save PUTs then closes the dialog). api_key is redacted
* server-side so it stays blank ("unchanged if blank"). Esc and backdrop
* clicks close the native dialog for free; the close event notifies the
* parent so it strips the ?settings param. */
function SettingsDialog({ onClose }) {
	const [models, setModels] = useState([]);
	const [modelsError, setModelsError] = useState(null);
	const [saveError, setSaveError] = useState(null);
	const [loadError, setLoadError] = useState(null);
	const [fieldErrors, setFieldErrors] = useState({});
	const [saving, setSaving] = useState(false);
	const [testing, setTesting] = useState(false);
	const [testResult, setTestResult] = useState(null);
	const formRef = useRef(null);
	const dialogRef = useRef(null);
	const [model, setModel] = useState("");
	const [provider, setProvider] = useState("openai");
	const [theme, setThemeState] = useState(getTheme);
	useEffect(() => {
		const ctl = new AbortController();
		const setField = (name, value) => {
			const el = formRef.current?.elements?.namedItem(name);
			if (el instanceof HTMLInputElement || el instanceof HTMLSelectElement) el.value = value;
		};
		getSettings(ctl.signal).then((s) => {
			const ai = s.ai;
			if (!ai) return;
			if (PROVIDERS.includes(ai.provider)) {
				setProvider(ai.provider);
				setField("provider", ai.provider);
			}
			if (ai.model) setModel(ai.model);
			if (ai.base_url) setField("base_url", ai.base_url);
			const enabled = formRef.current?.elements?.namedItem("enabled");
			if (enabled instanceof HTMLInputElement) enabled.checked = ai.enabled;
		}).catch((e) => {
			if (e?.name !== "AbortError") setLoadError(e?.message ?? "load failed");
		});
		listModels(ctl.signal).then((m) => {
			setModels(m.data.map((d) => d.id));
			setModelsError(m.error ?? null);
		}).catch(() => setModels([]));
		return () => ctl.abort();
	}, []);
	useEffect(function wireDialogOnMount() {
		const dlg = dialogRef.current;
		if (!dlg) return;
		dlg.showModal();
		dlg.addEventListener("close", onClose);
		dlg.querySelector("select, input")?.focus();
		return () => dlg.removeEventListener("close", onClose);
	}, [onClose]);
	const submit = (e) => {
		e.preventDefault();
		setSaveError(null);
		setTestResult(null);
		const fd = new FormData(formRef.current);
		const raw = {
			provider: String(fd.get("provider") ?? ""),
			model: model.trim(),
			api_key: String(fd.get("api_key") ?? "").trim() || void 0,
			base_url: String(fd.get("base_url") ?? "").trim() || void 0,
			enabled: fd.get("enabled") === "on"
		};
		const parsed = v.safeParse(SettingsSchema, raw);
		if (!parsed.success) {
			const errs = {};
			for (const issue of parsed.issues) {
				const key = issue.path?.[0]?.key;
				if (key && !errs[key]) errs[key] = issue.message;
			}
			setFieldErrors(errs);
			return;
		}
		setFieldErrors({});
		setSaving(true);
		putSettings({
			provider: parsed.output.provider,
			model: parsed.output.model,
			api_key: parsed.output.api_key ?? null,
			base_url: parsed.output.base_url ?? null,
			enabled: parsed.output.enabled
		}).then(() => {
			setSaving(false);
			bumpModels();
			toast("success", "settings saved");
			dialogRef.current?.close();
		}).catch((err) => {
			setSaving(false);
			const msg = err.message ?? "save failed";
			setSaveError(msg);
			toast("error", `settings save failed: ${msg}`);
		});
	};
	/** Verify the form's provider/model/key/base_url with POST /settings/test. */
	const runTest = () => {
		if (testing) return;
		setTestResult(null);
		setSaveError(null);
		const fd = new FormData(formRef.current);
		const provider = String(fd.get("provider") ?? "");
		const modelTrimmed = model.trim();
		const baseUrl = String(fd.get("base_url") ?? "").trim();
		const apiKey = String(fd.get("api_key") ?? "").trim();
		if (!modelTrimmed) {
			setTestResult({
				ok: false,
				detail: "pick a model first"
			});
			return;
		}
		setTesting(true);
		testConnection({
			provider,
			model: modelTrimmed,
			base_url: baseUrl || void 0,
			api_key: apiKey || void 0
		}).then((r) => {
			setTestResult({
				ok: Boolean(r.ok),
				detail: r.detail
			});
			toast(r.ok ? "success" : "error", r.detail || (r.ok ? "connection ok" : "connection failed"));
		}).catch((err) => {
			const detail = err.message ?? "test failed";
			setTestResult({
				ok: false,
				detail
			});
			toast("error", detail);
		}).finally(() => setTesting(false));
	};
	const err = (k) => fieldErrors[k] ? /* @__PURE__ */ jsx("p", {
		class: "text-error text-xs mt-1",
		children: fieldErrors[k]
	}) : null;
	return /* @__PURE__ */ jsxs("dialog", {
		ref: dialogRef,
		class: "modal",
		"aria-label": "settings",
		children: [/* @__PURE__ */ jsxs("div", {
			class: "modal-box w-full max-w-md animate-in fade-in zoom-in-95 duration-150",
			children: [
				/* @__PURE__ */ jsx("h2", {
					class: "text-base font-semibold mb-3",
					children: "settings"
				}),
				loadError && /* @__PURE__ */ jsxs("div", {
					role: "alert",
					class: "alert alert-error text-sm mb-3",
					children: [
						"backend settings endpoints not available (",
						loadError,
						") - the server needs GET/PUT /settings support"
					]
				}),
				/* @__PURE__ */ jsxs("form", {
					ref: formRef,
					onSubmit: submit,
					noValidate: true,
					children: [
						/* @__PURE__ */ jsxs("fieldset", {
							class: "fieldset gap-2.5",
							children: [
								/* @__PURE__ */ jsx("legend", {
									class: "fieldset-legend text-sm",
									children: "ai"
								}),
								/* @__PURE__ */ jsx("label", {
									class: "label text-xs",
									for: "set-provider",
									children: "provider"
								}),
								/* @__PURE__ */ jsx("select", {
									id: "set-provider",
									name: "provider",
									class: "select select-sm w-full",
									value: provider,
									onInput: (e) => setProvider(e.target.value),
									children: PROVIDERS.map((p) => /* @__PURE__ */ jsx("option", {
										value: p,
										children: p
									}, p))
								}),
								err("provider"),
								/* @__PURE__ */ jsx("label", {
									class: "label text-xs",
									for: "set-model",
									children: "model"
								}),
								/* @__PURE__ */ jsx(ModelPicker, {
									id: "set-model",
									models,
									value: model,
									onChange: setModel,
									size: "sm",
									modelsError
								}),
								err("model"),
								modelsError && models.length === 0 && /* @__PURE__ */ jsxs("p", {
									class: "text-warning text-xs mt-1",
									role: "note",
									children: ["model listing failed: ", modelsError]
								}),
								/* @__PURE__ */ jsx("label", {
									class: "label text-xs",
									for: "set-api-key",
									children: "api key"
								}),
								/* @__PURE__ */ jsx("input", {
									id: "set-api-key",
									name: "api_key",
									type: "password",
									class: "input input-sm w-full",
									placeholder: "(unchanged if blank)",
									autocomplete: "off"
								}),
								err("api_key"),
								/* @__PURE__ */ jsx("label", {
									class: "label text-xs",
									for: "set-base-url",
									children: "base url"
								}),
								/* @__PURE__ */ jsx("input", {
									id: "set-base-url",
									name: "base_url",
									type: "url",
									class: "input input-sm w-full",
									placeholder: "https://api.openai.com/v1",
									autocomplete: "off"
								}),
								err("base_url"),
								/* @__PURE__ */ jsxs("div", {
									class: "flex items-center gap-2 mt-1",
									children: [/* @__PURE__ */ jsx("button", {
										type: "button",
										class: "btn btn-outline btn-sm",
										disabled: testing,
										onClick: runTest,
										children: testing ? /* @__PURE__ */ jsx("span", { class: "loading loading-spinner loading-xs" }) : "Test connection"
									}), testResult && /* @__PURE__ */ jsx("span", {
										class: `text-xs ${testResult.ok ? "text-success" : "text-error"}`,
										role: "status",
										children: testResult.detail
									})]
								}),
								/* @__PURE__ */ jsxs("label", {
									class: "label cursor-pointer gap-2 text-xs justify-start",
									children: [/* @__PURE__ */ jsx("input", {
										type: "checkbox",
										name: "enabled",
										class: "toggle toggle-sm",
										defaultChecked: true
									}), "enabled"]
								})
							]
						}),
						/* @__PURE__ */ jsxs("fieldset", {
							class: "fieldset gap-2.5 mt-2",
							children: [
								/* @__PURE__ */ jsx("legend", {
									class: "fieldset-legend text-sm",
									children: "theme"
								}),
								/* @__PURE__ */ jsx("div", {
									role: "radiogroup",
									"aria-label": "theme",
									class: "join",
									children: THEMES.map((t) => /* @__PURE__ */ jsx("button", {
										type: "button",
										role: "radio",
										"aria-checked": theme === t,
										tabIndex: theme === t ? 0 : -1,
										class: `btn join-item btn-sm ${theme === t ? "btn-primary" : "btn-ghost"}`,
										onClick: () => {
											setThemeState(t);
											setTheme(t);
										},
										children: t === "system" ? "System" : t === "light" ? "Light" : "Dark"
									}, t))
								}),
								theme === "system" && /* @__PURE__ */ jsx("p", {
									class: "text-xs opacity-50",
									children: "follows your OS light/dark preference"
								})
							]
						}),
						saveError && /* @__PURE__ */ jsx("div", {
							role: "alert",
							class: "alert alert-error text-xs mt-2",
							children: /* @__PURE__ */ jsx("span", { children: saveError })
						}),
						/* @__PURE__ */ jsxs("div", {
							class: "modal-action",
							children: [/* @__PURE__ */ jsx("button", {
								type: "button",
								class: "btn btn-ghost btn-sm",
								onClick: () => dialogRef.current?.close(),
								children: "cancel"
							}), /* @__PURE__ */ jsx("button", {
								type: "submit",
								class: "btn btn-primary btn-sm",
								disabled: saving,
								children: saving ? /* @__PURE__ */ jsx("span", { class: "loading loading-dots loading-xs" }) : "save"
							})]
						})
					]
				})
			]
		}), /* @__PURE__ */ jsx("form", {
			method: "dialog",
			class: "modal-backdrop",
			children: /* @__PURE__ */ jsx("button", {
				"aria-label": "close settings",
				children: "close"
			})
		})]
	});
}
var init_SettingsDialog = __esmMin((() => {
	init_ai();
	init_ModelPicker();
	init_Toasts();
	init_theme();
	init_schema();
}));
//#endregion
//#region src/components/Header.tsx
/** AI-mode availability from GET /v1/models (`ai_available`).
* Tri-state: null = still querying (never demote AI mode on null). */
function useAiAvailable() {
	const [ai, setAi] = useState(null);
	useEffect(() => {
		const ctl = new AbortController();
		listModels(ctl.signal).then((m) => setAi(Boolean(m.ai_available))).catch(() => setAi(false));
		return () => ctl.abort();
	}, []);
	return ai;
}
function Header() {
	const { path, query, route } = useLocation();
	const settingsOpen = query?.settings === "open";
	const closeSettings = () => {
		const sp = new URLSearchParams(window.location.search);
		sp.delete("settings");
		const qs = sp.toString();
		route(`${window.location.pathname}${qs ? `?${qs}` : ""}`, true);
	};
	const isActive = (item) => item.exact ? path === item.href : path === item.href || path.startsWith(`${item.href}/`);
	return /* @__PURE__ */ jsxs("header", {
		class: "navbar bg-base-100 border-b border-base-300 px-4 h-12 min-h-12 flex items-center gap-4",
		children: [
			/* @__PURE__ */ jsx("a", {
				href: "/",
				class: "font-logo font-semibold tracking-tight text-base",
				children: "oxe"
			}),
			/* @__PURE__ */ jsx("nav", {
				class: "flex items-center gap-1 flex-wrap text-sm",
				"aria-label": "main",
				children: NAV.map((item) => /* @__PURE__ */ jsx("a", {
					href: item.href,
					class: `px-2 py-1 rounded ${isActive(item) ? "font-semibold" : "opacity-70 hover:opacity-100"}`,
					"aria-current": isActive(item) ? "page" : void 0,
					children: isActive(item) ? `[${item.label}]` : item.label
				}, item.href))
			}),
			/* @__PURE__ */ jsxs("span", {
				class: "ml-auto flex items-center gap-1",
				children: [
					/* @__PURE__ */ jsx("button", {
						type: "button",
						class: "btn btn-ghost btn-xs",
						"aria-label": "Settings",
						onClick: () => route(`${window.location.pathname}?settings=open`),
						children: "Settings"
					}),
					/* @__PURE__ */ jsx("a", {
						href: "https://github.com/espetro/oxe",
						target: "_blank",
						rel: "noopener noreferrer",
						class: "btn btn-ghost btn-sm btn-circle",
						"aria-label": "GitHub repository",
						tabIndex: 0,
						children: /* @__PURE__ */ jsx(GitHubIcon, {})
					}),
					/* @__PURE__ */ jsx("span", {
						class: "text-xs opacity-50 hidden sm:inline",
						children: "v0.4.0"
					})
				]
			}),
			settingsOpen && /* @__PURE__ */ jsx(SettingsDialog, { onClose: closeSettings })
		]
	});
}
function usePageTitle(title) {
	useEffect(() => {
		document.title = title ? `${title} · oxe` : "oxe";
	}, [title]);
}
function Center({ children, vh = false }) {
	return /* @__PURE__ */ jsx("div", {
		class: `flex w-full flex-col items-center ${vh ? "justify-center min-h-[75vh] -mt-[30vh]" : ""}`,
		children
	});
}
function Empty$1({ children }) {
	return /* @__PURE__ */ jsx("p", {
		class: "opacity-60 text-sm py-8 text-center",
		children: toChildArray(children)
	});
}
var NAV, GitHubIcon;
var init_Header = __esmMin((() => {
	init_ai();
	init_SettingsDialog();
	NAV = [
		{
			href: "/",
			label: "Search",
			exact: true
		},
		{
			href: "/history",
			label: "History"
		},
		{
			href: "/dashboard",
			label: "Dashboard"
		}
	];
	GitHubIcon = () => /* @__PURE__ */ jsx("svg", {
		width: "16",
		height: "16",
		viewBox: "0 0 16 16",
		fill: "currentColor",
		"aria-hidden": "true",
		children: /* @__PURE__ */ jsx("path", { d: "M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82a7.4 7.4 0 0 1 2-.27c.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A7.995 7.995 0 0 0 16 8c0-4.42-3.58-8-8-8z" })
	});
}));
//#endregion
//#region src/components/AboutHint.tsx
init_Header();
/** (?) about affordance, fixed to the bottom-left corner of the page
* (moved out of the navbar; lives in the root layout). daisyUI dropdown
* opens on hover/focus or toggles on click; dropdown-end keeps the panel
* inside the viewport down to ≈390px and clear of the content column
* (toasts are top-right, so no collision). */
function AboutHint() {
	return /* @__PURE__ */ jsxs("div", {
		class: "dropdown dropdown-end dropdown-top dropdown-hover dropdown-focus fixed bottom-3 left-3 z-40",
		children: [/* @__PURE__ */ jsx("button", {
			type: "button",
			class: "btn btn-ghost btn-xs btn-circle opacity-30 hover:opacity-70",
			"aria-label": "about oxe: caching, MCP API, search modes",
			children: "?"
		}), /* @__PURE__ */ jsxs("div", {
			class: "dropdown-content w-72 max-w-[min(288px,68vw)] bg-base-100 border border-base-300 rounded-md shadow-sm p-3 text-xs z-50",
			role: "note",
			children: [/* @__PURE__ */ jsx("p", {
				class: "mb-1.5",
				children: "search once, share with your agents - cached, MCP-ready · REST + MCP API on :4479"
			}), /* @__PURE__ */ jsxs("p", {
				class: "opacity-60",
				children: [
					/* @__PURE__ */ jsx("span", {
						class: "font-medium opacity-80",
						children: "search:"
					}),
					" classic link results with cache metadata. ",
					/* @__PURE__ */ jsx("span", {
						class: "font-medium opacity-80",
						children: "AI:"
					}),
					" streaming answer with cited sources."
				]
			})]
		})]
	});
}
//#endregion
//#region src/components/ErrorBoundary.tsx
/** @jsxImportSource preact */
/** Route-level class boundary (mounted in routes/_layout.tsx around the
* routed content): on a render crash it replaces the page area with a
* 500-ish hero while keeping the app shell (header, toasts) alive. */
var ErrorBoundary$1 = class extends Component {
	state = { error: null };
	static getDerivedStateFromError(error) {
		return { error };
	}
	componentDidCatch(error) {
		console.error("ui error boundary:", error);
	}
	render() {
		if (this.state.error) return /* @__PURE__ */ jsx("div", {
			class: "flex-1 flex flex-col items-center justify-center min-h-[60vh] px-4 text-center",
			children: /* @__PURE__ */ jsxs("div", {
				class: "max-w-md",
				children: [
					/* @__PURE__ */ jsx("p", {
						class: "text-5xl font-semibold font-mono tracking-tight opacity-30",
						children: "500"
					}),
					/* @__PURE__ */ jsx("h1", {
						class: "mt-2 text-lg font-medium",
						children: "something broke"
					}),
					/* @__PURE__ */ jsx("p", {
						class: "py-3 text-sm opacity-60",
						children: this.state.error.message
					}),
					/* @__PURE__ */ jsxs("div", {
						class: "mt-3 flex items-center justify-center gap-2",
						children: [/* @__PURE__ */ jsx("button", {
							type: "button",
							class: "btn btn-primary btn-sm",
							onClick: () => this.setState({ error: null }),
							children: "try again"
						}), /* @__PURE__ */ jsx("a", {
							href: "/",
							class: "btn btn-ghost btn-sm",
							children: "back to search"
						})]
					})
				]
			})
		});
		return this.props.children;
	}
};
//#endregion
//#region src/routes/_layout.tsx
init_Toasts();
/** Root layout, applied to every route via import.meta.glob in main.tsx.
* Hoists the persistent <Header/> (owns the global ?settings= dialog),
* the toast stack, and the fixed bottom-left (?) hint. The boundary only
* covers routed content, so a render crash keeps the shell alive. */
function Layout({ children }) {
	return /* @__PURE__ */ jsxs("div", {
		class: "min-h-screen flex flex-col",
		children: [
			/* @__PURE__ */ jsx(Header, {}),
			/* @__PURE__ */ jsx("main", {
				class: "flex-1 flex flex-col",
				children: /* @__PURE__ */ jsx(ErrorBoundary$1, { children })
			}),
			/* @__PURE__ */ jsx(Toasts, {}),
			/* @__PURE__ */ jsx(AboutHint, {})
		]
	});
}
//#endregion
//#region src/routes/404.tsx
var _404_exports = /* @__PURE__ */ __exportAll({ default: () => NotFound });
function NotFound() {
	return /* @__PURE__ */ jsxs("div", {
		class: "flex-1 flex flex-col items-center justify-center min-h-[60vh] px-4 text-center animate-in fade-in zoom-in-95 duration-300",
		children: [
			/* @__PURE__ */ jsx("p", {
				class: "text-5xl font-logo font-semibold tracking-tight opacity-30",
				children: "404"
			}),
			/* @__PURE__ */ jsx("h1", {
				class: "mt-2 text-lg font-medium",
				children: "nothing here"
			}),
			/* @__PURE__ */ jsx("p", {
				class: "mt-1 text-sm opacity-60",
				children: "this page does not exist — check the address or head back to search"
			}),
			/* @__PURE__ */ jsxs("div", {
				class: "mt-6 flex items-center gap-2",
				children: [/* @__PURE__ */ jsx("a", {
					href: "/",
					class: "btn btn-primary btn-sm",
					children: "back to search"
				}), /* @__PURE__ */ jsx("a", {
					href: "/history",
					class: "btn btn-ghost btn-sm",
					children: "history"
				})]
			})
		]
	});
}
var init__404 = __esmMin((() => {}));
//#endregion
//#region src/lib/format.ts
function domainOf(url) {
	try {
		return new URL(url).hostname.replace(/^www\./, "") || url;
	} catch {
		return url;
	}
}
function faviconFor(url) {
	const d = domainOf(url);
	return `https://icons.duckduckgo.com/ip3/${encodeURIComponent(d)}.ico`;
}
function fmtDur(s) {
	if (s == null || s < 0) return "0s";
	if (s < 60) return `${Math.floor(s)}s`;
	if (s < 3600) return `${Math.floor(s / 60)}m`;
	if (s < 86400) return `${Math.floor(s / 3600)}h`;
	return `${Math.floor(s / 86400)}d`;
}
function fmtBytes(n) {
	if (n < 1024) return `${n} B`;
	if (n < 1048576) return `${(n / 1024).toFixed(1)} KB`;
	return `${(n / 1048576).toFixed(1)} MB`;
}
function fmtTs(epoch) {
	if (!epoch) return "—";
	return (/* @__PURE__ */ new Date(epoch * 1e3)).toISOString().replace("T", " ").slice(0, 19) + " UTC";
}
function truncate(s, n) {
	return s.length > n ? `${s.slice(0, n - 1)}…` : s;
}
var init_format = __esmMin((() => {}));
//#endregion
//#region src/routes/dashboard.tsx
var dashboard_exports = /* @__PURE__ */ __exportAll({ default: () => DashboardRoute });
function Panel(props) {
	return /* @__PURE__ */ jsxs("section", {
		class: `border border-base-300 rounded-lg p-4 min-w-0 ${props.wide ? "md:col-span-2" : ""}`,
		children: [/* @__PURE__ */ jsx("h2", {
			class: "text-sm font-medium mb-3",
			children: props.title
		}), props.children]
	});
}
function Empty() {
	return /* @__PURE__ */ jsx("p", {
		class: "opacity-50 text-sm",
		children: "no data yet"
	});
}
/** Token-based inline SVG sparkline: total searches per day. */
function Sparkline({ days }) {
	const data = days.slice(-30);
	const max = Math.max(...data.map((d) => d.total), 1);
	const W = 240;
	const H = 48;
	const step = data.length > 1 ? W / (data.length - 1) : W;
	const pts = data.map((d, i) => `${(i * step).toFixed(1)},${(H - d.total / max * 44 - 2).toFixed(1)}`);
	return /* @__PURE__ */ jsx("svg", {
		viewBox: `0 0 ${W} ${H}`,
		class: "w-full h-12",
		role: "img",
		"aria-label": "searches per day",
		children: /* @__PURE__ */ jsx("polyline", {
			points: pts.join(" "),
			fill: "none",
			stroke: "currentColor",
			"stroke-width": "1.5",
			class: "text-primary"
		})
	});
}
function Bar({ value, max, label }) {
	const pct = max > 0 ? Math.round(value / max * 100) : 0;
	return /* @__PURE__ */ jsxs("div", {
		class: "mb-1",
		children: [/* @__PURE__ */ jsxs("div", {
			class: "flex justify-between text-[13px]",
			children: [/* @__PURE__ */ jsx("span", {
				class: "truncate max-w-[70%]",
				children: label
			}), /* @__PURE__ */ jsx("span", {
				class: "opacity-60",
				children: value
			})]
		}), /* @__PURE__ */ jsx("progress", {
			class: "progress progress-primary h-1",
			value: pct,
			max: 100,
			"aria-label": `${label}: ${value}`
		})]
	});
}
function DashboardRoute() {
	usePageTitle("dashboard");
	const [stats, setStats] = useState(null);
	const [error, setError] = useState(null);
	const seq = useRef(0);
	useEffect(() => {
		const id = ++seq.current;
		apiStats().then((s) => seq.current === id && (setStats(s), setError(null))).catch((e) => seq.current === id && setError(e.message));
	}, []);
	return /* @__PURE__ */ jsxs("div", {
		class: "w-full max-w-[960px] mx-auto px-4 pb-16",
		children: [
			/* @__PURE__ */ jsx("h1", {
				class: "text-xl font-semibold mt-6 mb-1",
				children: "oxe stats"
			}),
			/* @__PURE__ */ jsxs("p", {
				class: "text-[13px] opacity-60 mb-4",
				children: [
					"window: last ",
					stats?.days ?? 14,
					" days"
				]
			}),
			error && /* @__PURE__ */ jsxs("p", {
				class: "text-error py-6 text-sm",
				children: ["error: ", error]
			}),
			!stats && !error && /* @__PURE__ */ jsx("div", {
				class: "py-10 flex justify-center",
				"aria-busy": "true",
				children: /* @__PURE__ */ jsx("span", { class: "loading loading-dots loading-md" })
			}),
			stats && /* @__PURE__ */ jsxs("div", {
				class: "grid gap-4 md:grid-cols-2",
				children: [
					/* @__PURE__ */ jsx(Panel, {
						title: "searches per day",
						children: stats.searches_per_day.some((d) => d.total > 0) ? /* @__PURE__ */ jsxs(Fragment, { children: [/* @__PURE__ */ jsx(Sparkline, { days: stats.searches_per_day }), /* @__PURE__ */ jsxs("p", {
							class: "text-[13px] opacity-60 mt-1",
							children: [stats.searches_per_day.reduce((a, d) => a + d.total, 0), " searches in window"]
						})] }) : /* @__PURE__ */ jsx(Empty, {})
					}),
					/* @__PURE__ */ jsx(Panel, {
						title: "cache hit rate",
						children: stats.hit_rate.rate != null ? /* @__PURE__ */ jsxs(Fragment, { children: [/* @__PURE__ */ jsxs("div", {
							class: "text-3xl font-bold",
							children: [stats.hit_rate.rate, "%"]
						}), /* @__PURE__ */ jsxs("p", {
							class: "text-[13px] opacity-60",
							children: [
								stats.hit_rate.cache_hits,
								" of ",
								stats.hit_rate.total,
								" served from cache"
							]
						})] }) : /* @__PURE__ */ jsx(Empty, {})
					}),
					/* @__PURE__ */ jsx(Panel, {
						title: "network latency",
						children: stats.latency_ms.p50 != null ? /* @__PURE__ */ jsx("div", {
							class: "grid grid-cols-3 gap-2 text-center",
							children: [
								"p50",
								"p90",
								"p99"
							].map((p) => /* @__PURE__ */ jsxs("div", { children: [/* @__PURE__ */ jsx("div", {
								class: "text-xl font-semibold",
								children: Math.round(stats.latency_ms[p] ?? 0)
							}), /* @__PURE__ */ jsxs("div", {
								class: "text-[13px] opacity-60",
								children: [p, " ms"]
							})] }, p))
						}) : /* @__PURE__ */ jsx(Empty, {})
					}),
					/* @__PURE__ */ jsx(Panel, {
						title: "client split",
						children: stats.client_split.length > 0 ? /* @__PURE__ */ jsx("div", { children: stats.client_split.map((c) => /* @__PURE__ */ jsx(Bar, {
							value: c.count,
							label: c.client,
							max: Math.max(...stats.client_split.map((x) => x.count))
						}, c.client)) }) : /* @__PURE__ */ jsx(Empty, {})
					}),
					/* @__PURE__ */ jsx(Panel, {
						title: "top queries",
						wide: true,
						children: stats.top_queries.length > 0 ? /* @__PURE__ */ jsx("div", { children: stats.top_queries.slice(0, 10).map((q) => /* @__PURE__ */ jsx(Bar, {
							value: q.count,
							label: q.query,
							max: Math.max(...stats.top_queries.slice(0, 10).map((x) => x.count))
						}, q.query)) }) : /* @__PURE__ */ jsx(Empty, {})
					}),
					/* @__PURE__ */ jsx(Panel, {
						title: "zero-result queries",
						wide: true,
						children: stats.zero_result_queries.length > 0 ? /* @__PURE__ */ jsx("ul", {
							class: "text-[13px] space-y-1",
							children: stats.zero_result_queries.slice(0, 10).map((q) => /* @__PURE__ */ jsxs("li", {
								class: "flex justify-between gap-4",
								children: [/* @__PURE__ */ jsx("span", {
									class: "truncate",
									children: q.query
								}), /* @__PURE__ */ jsx("span", {
									class: "opacity-60 whitespace-nowrap",
									children: fmtTs(q.last_seen)
								})]
							}, q.query))
						}) : /* @__PURE__ */ jsx("p", {
							class: "opacity-50 text-sm",
							children: "none 🎉"
						})
					}),
					/* @__PURE__ */ jsx(Panel, {
						title: "cache",
						wide: true,
						children: /* @__PURE__ */ jsx("table", {
							class: "table table-sm text-[13px]",
							children: /* @__PURE__ */ jsxs("tbody", { children: [
								/* @__PURE__ */ jsxs("tr", { children: [
									/* @__PURE__ */ jsx("td", {
										class: "opacity-60",
										children: "rows"
									}),
									/* @__PURE__ */ jsx("td", {
										class: "text-right",
										children: stats.cache.rows
									}),
									/* @__PURE__ */ jsx("td", {
										class: "opacity-60",
										children: "unexpired"
									}),
									/* @__PURE__ */ jsx("td", {
										class: "text-right",
										children: stats.cache.unexpired_rows
									})
								] }),
								/* @__PURE__ */ jsxs("tr", { children: [
									/* @__PURE__ */ jsx("td", {
										class: "opacity-60",
										children: "db size"
									}),
									/* @__PURE__ */ jsx("td", {
										class: "text-right",
										children: fmtBytes(stats.cache.db_size_bytes)
									}),
									/* @__PURE__ */ jsx("td", {
										class: "opacity-60",
										children: "total hits"
									}),
									/* @__PURE__ */ jsx("td", {
										class: "text-right",
										children: stats.cache.total_hits
									})
								] }),
								/* @__PURE__ */ jsxs("tr", { children: [
									/* @__PURE__ */ jsx("td", {
										class: "opacity-60",
										children: "newest"
									}),
									/* @__PURE__ */ jsx("td", {
										class: "text-right",
										children: fmtTs(stats.cache.newest)
									}),
									/* @__PURE__ */ jsx("td", {
										class: "opacity-60",
										children: "oldest"
									}),
									/* @__PURE__ */ jsx("td", {
										class: "text-right",
										children: fmtTs(stats.cache.oldest_unexpired)
									})
								] })
							] })
						})
					})
				]
			})
		]
	});
}
var init_dashboard = __esmMin((() => {
	init_Header();
	init_api();
	init_format();
}));
//#endregion
//#region src/routes/history.tsx
var history_exports = /* @__PURE__ */ __exportAll({ default: () => HistoryRoute });
function parseSince(v) {
	return SINCE_VALUES.includes(v) ? v : "all";
}
function fmtLocal(epoch) {
	if (!epoch) return "—";
	const d = /* @__PURE__ */ new Date(epoch * 1e3);
	const p = (n) => String(n).padStart(2, "0");
	return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
/** Two-step delete: pick a scope, then confirm. `all` requires a second
* click on the same button (double-click confirm). */
function DeleteControls({ onDeleted, onError }) {
	const [scope, setScope] = useState("24h");
	const [armed, setArmed] = useState(false);
	const [busy, setBusy] = useState(false);
	const run = () => {
		setBusy(true);
		deleteHistory(scope).then(() => {
			setArmed(false);
			onDeleted();
		}).catch((e) => onError(e.message)).finally(() => setBusy(false));
	};
	return /* @__PURE__ */ jsxs("div", {
		class: "flex items-center gap-2",
		children: [/* @__PURE__ */ jsxs("select", {
			class: "select select-sm w-40",
			value: scope,
			onChange: (e) => {
				setScope(e.target.value);
				setArmed(false);
			},
			"aria-label": "delete scope",
			children: [
				/* @__PURE__ */ jsx("option", {
					value: "24h",
					children: "older than 24h"
				}),
				/* @__PURE__ */ jsx("option", {
					value: "7d",
					children: "older than 7d"
				}),
				/* @__PURE__ */ jsx("option", {
					value: "30d",
					children: "older than 30d"
				}),
				/* @__PURE__ */ jsx("option", {
					value: "all",
					children: "all history"
				})
			]
		}), /* @__PURE__ */ jsx("button", {
			type: "button",
			class: `btn btn-sm ${armed ? "btn-error" : "btn-ghost text-error"}`,
			disabled: busy,
			onClick: () => armed ? run() : setArmed(true),
			onBlur: () => setArmed(false),
			children: armed ? scope === "all" ? "really delete all?" : "confirm delete" : "delete…"
		})]
	});
}
function HistoryRoute() {
	usePageTitle("history");
	const { query, route } = useLocation();
	const since = parseSince(query?.since);
	const qf = String(query?.qf ?? "");
	const [items, setItems] = useState([]);
	const [counts, setCounts] = useState({
		clicks: 0,
		cache_rows: 0
	});
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState(null);
	const seq = useRef(0);
	const load = (s, q) => {
		const id = ++seq.current;
		setLoading(true);
		fetchApiHistory(s, q).then((r) => {
			if (seq.current !== id) return;
			setItems(r.items.map((it) => ({
				...it,
				sort_at: it.kind === "click" ? it.clicked_at : it.created_at
			})));
			setCounts({
				clicks: r.clicks,
				cache_rows: r.cache_rows
			});
			setError(null);
		}).catch((e) => seq.current === id && setError(e.message)).finally(() => seq.current === id && setLoading(false));
	};
	useEffect(() => {
		load(since, qf);
	}, [since, qf]);
	const setParam = (key, value) => {
		const sp = new URLSearchParams(window.location.search);
		if (value && !(key === "since" && value === "all")) sp.set(key, value);
		else sp.delete(key);
		const qs = sp.toString();
		route(`${window.location.pathname}${qs ? `?${qs}` : ""}`, true);
	};
	const clearFilters = () => {
		const sp = new URLSearchParams(window.location.search);
		sp.delete("since");
		sp.delete("qf");
		const qs = sp.toString();
		route(`${window.location.pathname}${qs ? `?${qs}` : ""}`, true);
	};
	return /* @__PURE__ */ jsxs("div", {
		class: "w-full max-w-[960px] mx-auto px-4 pb-16",
		children: [
			/* @__PURE__ */ jsx("h1", {
				class: "text-xl font-semibold mt-6 mb-1",
				children: "History"
			}),
			/* @__PURE__ */ jsxs("p", {
				class: "text-[13px] opacity-60 mb-4",
				children: [
					counts.clicks,
					" clicks · ",
					counts.cache_rows,
					" cached searches · newest first"
				]
			}),
			/* @__PURE__ */ jsxs("div", {
				class: "flex flex-wrap items-center gap-2 mb-4",
				children: [
					/* @__PURE__ */ jsxs("select", {
						class: "select select-sm w-32",
						value: since,
						onChange: (e) => setParam("since", e.target.value),
						"aria-label": "time filter",
						children: [
							/* @__PURE__ */ jsx("option", {
								value: "all",
								children: "all time"
							}),
							/* @__PURE__ */ jsx("option", {
								value: "24",
								children: "last 24h"
							}),
							/* @__PURE__ */ jsx("option", {
								value: "168",
								children: "last week"
							}),
							/* @__PURE__ */ jsx("option", {
								value: "720",
								children: "last month"
							})
						]
					}),
					/* @__PURE__ */ jsx("input", {
						type: "search",
						class: "input input-sm w-56",
						placeholder: "filter by query text…",
						value: qf,
						onInput: (e) => setParam("qf", e.target.value),
						"aria-label": "query filter"
					}),
					(since !== "all" || qf) && /* @__PURE__ */ jsx("button", {
						type: "button",
						class: "btn btn-ghost btn-sm",
						onClick: clearFilters,
						children: "clear"
					}),
					/* @__PURE__ */ jsx("div", {
						class: "ml-auto",
						children: /* @__PURE__ */ jsx(DeleteControls, {
							onDeleted: () => load(since, qf),
							onError: setError
						})
					})
				]
			}),
			loading && /* @__PURE__ */ jsx("div", {
				class: "py-10 flex justify-center",
				"aria-busy": "true",
				children: /* @__PURE__ */ jsx("span", { class: "loading loading-dots loading-md" })
			}),
			!loading && error && /* @__PURE__ */ jsxs("p", {
				class: "text-error py-6 text-sm",
				children: ["error: ", error]
			}),
			!loading && !error && items.length === 0 && /* @__PURE__ */ jsxs("p", {
				class: "opacity-60 py-8 text-sm",
				children: [
					"nothing here yet — open a result from the",
					" ",
					/* @__PURE__ */ jsx("a", {
						href: "/",
						class: "link link-primary",
						children: "search"
					}),
					" ",
					"page."
				]
			}),
			!loading && items.length > 0 && /* @__PURE__ */ jsx("div", {
				class: "overflow-x-auto",
				children: /* @__PURE__ */ jsxs("table", {
					class: "table table-sm",
					children: [/* @__PURE__ */ jsx("thead", { children: /* @__PURE__ */ jsxs("tr", {
						class: "text-[13px] opacity-60",
						children: [
							/* @__PURE__ */ jsx("th", { children: "when" }),
							/* @__PURE__ */ jsx("th", { children: "kind" }),
							/* @__PURE__ */ jsx("th", { children: "query" }),
							/* @__PURE__ */ jsx("th", {
								class: "hidden md:table-cell",
								children: "detail"
							}),
							/* @__PURE__ */ jsx("th", {})
						]
					}) }), /* @__PURE__ */ jsx("tbody", { children: items.map((r) => /* @__PURE__ */ jsxs("tr", {
						class: "align-top",
						children: [
							/* @__PURE__ */ jsx("td", {
								class: "whitespace-nowrap text-[13px]",
								children: fmtLocal(r.sort_at)
							}),
							/* @__PURE__ */ jsx("td", {
								class: "text-[13px]",
								children: /* @__PURE__ */ jsx("span", {
									class: `badge badge-sm ${r.kind === "click" ? "badge-primary" : "badge-ghost"}`,
									children: r.kind === "click" ? `click · ${r.source ?? "web"}` : "search"
								})
							}),
							/* @__PURE__ */ jsx("td", {
								class: "text-[13px]",
								children: /* @__PURE__ */ jsx("a", {
									href: `/row/${r.query_hash}`,
									class: "link link-primary",
									children: r.query || "(no query)"
								})
							}),
							/* @__PURE__ */ jsx("td", {
								class: "hidden md:table-cell text-[13px] opacity-60 max-w-[300px] truncate",
								children: r.kind === "click" ? /* @__PURE__ */ jsx("a", {
									href: r.url,
									target: "_blank",
									rel: "noopener noreferrer",
									class: "link link-hover",
									children: truncate(r.url ?? "", 80)
								}) : /* @__PURE__ */ jsxs(Fragment, { children: [
									r.hits ?? 0,
									" hits · expires ",
									fmtLocal(r.expires_at ?? 0)
								] })
							}),
							/* @__PURE__ */ jsx("td", { children: r.kind === "click" && /* @__PURE__ */ jsx("button", {
								type: "button",
								class: "btn btn-ghost btn-xs",
								onClick: () => fetch(`/search?q=${encodeURIComponent(r.query || "")}`, { headers: { Accept: "application/json" } }).then((res) => res.text()).then((t) => navigator.clipboard?.writeText(t)),
								children: "copy json"
							}) })
						]
					}, `${r.kind}-${r.query_hash}-${r.sort_at}`)) })]
				})
			})
		]
	});
}
var SINCE_VALUES;
var init_history = __esmMin((() => {
	init_Header();
	init_api();
	init_format();
	SINCE_VALUES = [
		"24",
		"168",
		"720",
		"all"
	];
}));
//#endregion
//#region src/components/ModeSegments.tsx
/** Models + AI availability from GET /v1/models.
* Refetches when settings saves bump the models version (bumpModels()).
* `available` is tri-state: null = still querying (never demote AI on null). */
function useModels() {
	const [available, setAvailable] = useState(null);
	const [models, setModels] = useState([]);
	const [error, setError] = useState(null);
	const [version, setVersion] = useState(currentModelsVersion);
	useEffect(() => onModelsBump(() => setVersion(currentModelsVersion())), []);
	useEffect(() => {
		const ctl = new AbortController();
		listModels(ctl.signal).then((m) => {
			setAvailable(Boolean(m.ai_available));
			setModels(m.data.map((d) => d.id));
			setError(m.error ?? null);
		}).catch(() => setAvailable(false));
		return () => ctl.abort();
	}, [version]);
	return {
		available,
		models,
		error
	};
}
/** DDG-style inline segmented mode toggle at the right end of the pill:
* light track, active segment is a white pill with a small shadow.
* radiogroup semantics, arrow keys switch segments. */
function ModeSegments({ mode, onChange, aiAvailable }) {
	const aiDisabled = aiAvailable === false;
	const move = (dir) => {
		const i = SEGMENTS.findIndex((s) => s.v === mode);
		const next = SEGMENTS[(i + dir + SEGMENTS.length) % SEGMENTS.length];
		if (!(next.v === "ai" && aiDisabled)) onChange(next.v);
	};
	return /* @__PURE__ */ jsx("div", {
		role: "radiogroup",
		"aria-label": "search mode",
		class: "join bg-base-200 rounded-full p-0.5 shrink-0",
		children: SEGMENTS.map((s) => {
			const disabled = s.v === "ai" && aiDisabled;
			const active = mode === s.v;
			return /* @__PURE__ */ jsx("span", {
				class: "tooltip tooltip-bottom",
				"data-tip": disabled ? "configure a model in settings" : void 0,
				children: /* @__PURE__ */ jsxs("button", {
					type: "button",
					role: "radio",
					"aria-checked": active,
					"aria-disabled": disabled || void 0,
					disabled,
					tabIndex: active ? 0 : -1,
					class: `btn join-item btn-xs rounded-full border-0 ${active ? "bg-base-100 shadow-sm font-medium" : "bg-transparent opacity-60 hover:opacity-100"} ${disabled ? "btn-disabled opacity-30" : ""}`,
					onClick: () => {
						if (!disabled) onChange(s.v);
					},
					onKeyDown: (e) => {
						if (e.key === "ArrowRight" || e.key === "ArrowDown") {
							e.preventDefault();
							move(1);
						} else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
							e.preventDefault();
							move(-1);
						}
					},
					children: [s.v === "traditional" ? /* @__PURE__ */ jsx(Magnifier, {}) : /* @__PURE__ */ jsx(Sparkle, {}), s.label]
				})
			}, s.v);
		})
	});
}
/** AI second-row controls: model picker + reasoning toggle chip.
* Pure UI state (localStorage); request wiring is a backend concern. */
function AiControls({ available, models, modelsError }) {
	const ls = () => typeof localStorage === "undefined" ? null : localStorage;
	const [storedModel, setStoredModel] = useState(() => ls()?.getItem(STORE_KEY) ?? "");
	const [reasoning, setReasoningState] = useState(() => ls()?.getItem(REASONING_KEY) === "1");
	const model = storedModel && models.includes(storedModel) ? storedModel : models[0] ?? "";
	const setModel = (m) => {
		setStoredModel(m);
		ls()?.setItem(STORE_KEY, m);
	};
	const setReasoning = (r) => {
		setReasoningState(Boolean(r));
		ls()?.setItem(REASONING_KEY, r ? "1" : "0");
	};
	return /* @__PURE__ */ jsxs("div", {
		class: "flex flex-col sm:flex-row sm:items-center gap-1.5 sm:gap-2 w-full text-xs min-w-0",
		children: [/* @__PURE__ */ jsxs("div", {
			class: "flex items-center gap-1.5 min-w-0 flex-1",
			children: [/* @__PURE__ */ jsx("span", {
				class: "shrink-0 opacity-70",
				children: "model"
			}), /* @__PURE__ */ jsx(ModelPicker, {
				models,
				value: model,
				onChange: setModel,
				disabled: available === false || models.length === 0,
				size: "xs",
				modelsError
			})]
		}), /* @__PURE__ */ jsx("button", {
			type: "button",
			role: "switch",
			"aria-checked": reasoning,
			class: `btn btn-xs rounded-full shrink-0 self-start sm:self-auto ${reasoning ? "btn-primary btn-soft" : "btn-ghost"}`,
			onClick: () => setReasoning(!reasoning),
			children: "reasoning"
		})]
	});
}
var SEGMENTS, Magnifier, Sparkle, STORE_KEY, REASONING_KEY;
var init_ModeSegments = __esmMin((() => {
	init_ai();
	init_ModelPicker();
	SEGMENTS = [{
		v: "traditional",
		label: "Search"
	}, {
		v: "ai",
		label: "AI"
	}];
	Magnifier = () => /* @__PURE__ */ jsxs("svg", {
		width: "13",
		height: "13",
		viewBox: "0 0 24 24",
		fill: "none",
		stroke: "currentColor",
		"stroke-width": "2",
		"stroke-linecap": "round",
		"aria-hidden": "true",
		children: [/* @__PURE__ */ jsx("circle", {
			cx: "11",
			cy: "11",
			r: "7"
		}), /* @__PURE__ */ jsx("path", { d: "m20 20-3.5-3.5" })]
	});
	Sparkle = () => /* @__PURE__ */ jsx("svg", {
		width: "13",
		height: "13",
		viewBox: "0 0 24 24",
		fill: "none",
		stroke: "currentColor",
		"stroke-width": "2",
		"stroke-linejoin": "round",
		"aria-hidden": "true",
		children: /* @__PURE__ */ jsx("path", { d: "M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9L12 3z" })
	});
	STORE_KEY = "oxe-ai-model";
	REASONING_KEY = "oxe-ai-reasoning";
}));
//#endregion
//#region src/features/suggests/useSuggests.ts
/** History-first suggestions from the local GET /suggest endpoint,
* followed by DDG web suggestions via the backend GET /ac proxy
* (gated by the oxe-ac localStorage toggle). */
function useSuggests(query, open) {
	const [history, setHistory] = useState([]);
	const [web, setWeb] = useState([]);
	const [acOn, setAcOnState] = useState(() => typeof localStorage === "undefined" || localStorage.getItem("oxe-ac") !== "off");
	useEffect(function fetchSuggestions() {
		const v = query.trim().toLowerCase();
		if (!open || v.length < 2) {
			setHistory([]);
			setWeb([]);
			return;
		}
		const webCtlRef = { current: null };
		const t = setTimeout(() => {
			const ctl = new AbortController();
			webCtlRef.current = ctl;
			if (acOn) ddgAc(v, ctl.signal).then(setWeb).catch(() => {});
			else setWeb([]);
		}, 300);
		const ctl = new AbortController();
		suggest(v, ctl.signal).then(setHistory).catch(() => {});
		return function cancelSuggestions() {
			clearTimeout(t);
			ctl.abort();
			webCtlRef.current?.abort();
		};
	}, [
		query,
		open,
		acOn
	]);
	const setAcOn = (v) => {
		setAcOnState(v);
		localStorage.setItem(AC_KEY, v ? "on" : "off");
	};
	return {
		items: mergeSuggests(history, web),
		acOn,
		setAcOn
	};
}
/** Merge history entries, dedup case-insensitively, most recent first. */
function mergeSuggests(history, web = []) {
	const seen = /* @__PURE__ */ new Set();
	const items = [];
	for (const text of history.slice(0, 3)) {
		const k = text.trim().toLowerCase();
		if (k && !seen.has(k)) {
			seen.add(k);
			items.push({
				text,
				group: "history"
			});
		}
	}
	for (const text of web.slice(0, 4)) {
		const k = text.trim().toLowerCase();
		if (k && !seen.has(k)) {
			seen.add(k);
			items.push({
				text,
				group: "web"
			});
		}
	}
	return items;
}
function useListNav(count, onPick, onFill, onClose) {
	const [activeIndex, setActiveIndex] = useState(null);
	const ref = useRef({
		count,
		onPick,
		onFill,
		onClose
	});
	ref.current = {
		count,
		onPick,
		onFill,
		onClose
	};
	const handleKey = (e) => {
		const { count: n, onPick: pick, onFill: fill, onClose: close } = ref.current;
		if (!n) return;
		if (e.key === "ArrowDown") {
			e.preventDefault();
			setActiveIndex((i) => i == null ? 0 : (i + 1) % n);
		} else if (e.key === "ArrowUp") {
			e.preventDefault();
			setActiveIndex((i) => i == null ? n - 1 : (i - 1 + n) % n);
		} else if (e.key === "Enter" && activeIndex != null && activeIndex < n) {
			e.preventDefault();
			pick(activeIndex);
		} else if ((e.key === "Tab" || e.key === "ArrowRight") && activeIndex != null && activeIndex < n) {
			e.preventDefault();
			fill(activeIndex);
		} else if (e.key === "Escape") close();
	};
	return {
		activeIndex,
		handleKey,
		setActiveIndex
	};
}
var AC_KEY, GROUP_LABEL;
var init_useSuggests = __esmMin((() => {
	init_api();
	AC_KEY = "oxe-ac";
	GROUP_LABEL = {
		history: "your history",
		web: "web suggestions"
	};
}));
//#endregion
//#region src/features/suggests/SuggestionsDropdown.tsx
/** Zero-weight suggestions dropdown: canvas background, hairline border,
* muted caps group labels (not options). Does not submit. */
function SuggestionsDropdown({ items, activeIndex, onPick, onHover, acOn, setAcOn }) {
	if (!items.length) return null;
	let idx = -1;
	let lastGroup = null;
	return /* @__PURE__ */ jsxs("ul", {
		role: "listbox",
		class: "absolute left-0 right-0 top-full mt-1 z-50 bg-base-100 border border-base-300 rounded-md shadow-sm py-1 text-sm m-0 list-none p-0",
		children: [items.map((s) => {
			idx += 1;
			const i = idx;
			const showLabel = s.group !== lastGroup;
			lastGroup = s.group;
			return /* @__PURE__ */ jsxs("li", {
				role: "none",
				children: [showLabel && /* @__PURE__ */ jsx("div", {
					class: "px-3 pt-2 pb-1 text-[11px] uppercase tracking-wide opacity-40 select-none",
					role: "presentation",
					children: GROUP_LABEL[s.group]
				}), /* @__PURE__ */ jsx("div", {
					role: "option",
					"aria-selected": activeIndex === i,
					class: `px-3 py-1.5 cursor-pointer ${activeIndex === i ? "bg-base-200" : ""}`,
					onMouseDown: (e) => {
						e.preventDefault();
						onPick(s.text);
					},
					onMouseEnter: () => onHover(i),
					onMouseLeave: () => onHover(null),
					children: s.text
				})]
			}, `${s.group}-${s.text}`);
		}), setAcOn && /* @__PURE__ */ jsx("li", {
			role: "none",
			class: "border-t border-base-300 mt-1",
			children: /* @__PURE__ */ jsxs("label", {
				class: "flex items-center justify-between gap-2 px-3 py-1.5 text-[11px] uppercase tracking-wide opacity-60 cursor-pointer select-none",
				role: "presentation",
				children: ["web suggestions", /* @__PURE__ */ jsx("input", {
					type: "checkbox",
					class: "toggle toggle-xs",
					checked: acOn !== false,
					onChange: (e) => setAcOn(e.target.checked),
					"aria-label": "toggle web suggestions"
				})]
			})
		})]
	});
}
var init_SuggestionsDropdown = __esmMin((() => {
	init_useSuggests();
}));
//#endregion
//#region src/features/suggests/SearchBox.tsx
/** DDG-style pill search bar: rounded-full container, inline segmented
* mode toggle at the right end, and (AI mode) a second action row that
* reveals via a smooth morphism. Suggestions stay anchored to the pill.
* Does not fetch (the suggests hook owns that) and does not navigate. */
function SearchBox({ value, onInput, onSubmit, placeholder, autoFocus, busy, size = "lg", ariaLabel = "search", mode, onModeChange, aiAvailable, models = [], modelsError }) {
	const [open, setOpen] = useState(false);
	const [focused, setFocused] = useState(false);
	const boxRef = useRef(null);
	const inputRef = useRef(null);
	const typedRef = useRef(value);
	typedRef.current = value;
	const aiMode = mode === "ai";
	const ph = placeholder ?? (aiMode ? "Ask anything privately" : "Search privately");
	const { items, acOn, setAcOn } = useSuggests(aiMode ? "" : value, open);
	const { activeIndex, handleKey, setActiveIndex } = useListNav(items.length, (i) => {
		const t = items[i]?.text;
		if (t) {
			setOpen(false);
			setActiveIndex(null);
			onInput(t);
			onSubmit(t);
		}
	}, (i) => {
		const t = items[i]?.text;
		if (t) {
			onInput(t);
			inputRef.current?.focus();
		}
	}, () => {
		setOpen(false);
		setActiveIndex(null);
		onInput(typedRef.current);
	});
	useMountEffect(function closeOnOutsideClick() {
		const onDocClick = (e) => {
			if (boxRef.current && !boxRef.current.contains(e.target)) setOpen(false);
		};
		document.addEventListener("mousedown", onDocClick);
		return () => document.removeEventListener("mousedown", onDocClick);
	});
	const showDropdown = open && value.trim().length >= 2 && items.length > 0;
	return /* @__PURE__ */ jsxs("div", {
		class: "relative w-full min-w-0 max-w-[min(672px,calc(100vw-48px))]",
		ref: boxRef,
		children: [/* @__PURE__ */ jsx("form", {
			role: "search",
			onSubmit: (e) => {
				e.preventDefault();
				setOpen(false);
				const q = value.trim();
				if (q) onSubmit(q);
			},
			children: /* @__PURE__ */ jsxs("div", {
				class: `w-full bg-base-100 border border-base-300 rounded-[28px] overflow-hidden
            transition-[box-shadow,border-color] duration-200 ease-out
            ${focused ? "oxe-pill-focus" : "shadow-none"}
            ${size === "lg" ? "px-4 py-2" : "px-3 py-1.5"}`,
				children: [/* @__PURE__ */ jsxs("div", {
					class: `flex items-center gap-1.5 ${size === "lg" ? "min-h-10" : "min-h-8"}`,
					children: [
						/* @__PURE__ */ jsx("input", {
							ref: inputRef,
							type: "search",
							name: "q",
							enterkeyhint: "search",
							autofocus: autoFocus,
							class: "grow bg-transparent outline-none min-w-0",
							placeholder: ph,
							"aria-label": ariaLabel,
							"aria-autocomplete": "list",
							"aria-expanded": showDropdown,
							"aria-controls": "suggest-listbox",
							autocomplete: "off",
							value,
							onInput: (e) => {
								onInput(e.target.value);
								setOpen(true);
								setActiveIndex(null);
							},
							onFocus: () => {
								setOpen(true);
								setFocused(true);
							},
							onBlur: () => setFocused(false),
							onKeyDown: (e) => {
								if (showDropdown) handleKey(e);
								else if (e.key === "Escape") e.target.blur();
							}
						}),
						mode && onModeChange ? /* @__PURE__ */ jsx(ModeSegments, {
							mode,
							onChange: onModeChange,
							aiAvailable: aiAvailable ?? null
						}) : null,
						/* @__PURE__ */ jsx("button", {
							type: "submit",
							class: "btn btn-ghost btn-sm btn-circle shrink-0",
							"aria-label": "submit search",
							disabled: busy,
							children: busy ? /* @__PURE__ */ jsx("span", { class: "loading loading-dots loading-xs" }) : /* @__PURE__ */ jsxs("svg", {
								width: "16",
								height: "16",
								viewBox: "0 0 24 24",
								fill: "none",
								stroke: "currentColor",
								"stroke-width": "2",
								"stroke-linecap": "round",
								"aria-hidden": "true",
								children: [/* @__PURE__ */ jsx("circle", {
									cx: "11",
									cy: "11",
									r: "7"
								}), /* @__PURE__ */ jsx("path", { d: "m20 20-3.5-3.5" })]
							})
						})
					]
				}), mode === "ai" && /* @__PURE__ */ jsx("div", {
					class: "ai-row-in border-t border-base-200 mt-1.5 pt-1.5",
					children: /* @__PURE__ */ jsx(AiControls, {
						available: aiAvailable ?? null,
						models,
						modelsError
					})
				})]
			})
		}), showDropdown && /* @__PURE__ */ jsx("div", {
			id: "suggest-listbox",
			children: /* @__PURE__ */ jsx(SuggestionsDropdown, {
				items,
				activeIndex,
				onPick: (text) => {
					setOpen(false);
					setActiveIndex(null);
					onInput(text);
					onSubmit(text);
				},
				onHover: (i) => setActiveIndex(i),
				acOn,
				setAcOn
			})
		})]
	});
}
var init_SearchBox = __esmMin((() => {
	init_SuggestionsDropdown();
	init_useSuggests();
	init_ModeSegments();
	init_useMountEffect();
}));
//#endregion
//#region src/routes/index.tsx
var routes_exports = /* @__PURE__ */ __exportAll({ default: () => Home });
/** Persist at event time and keep the tri-state demote as a derived value
* (never write the demoted value back into state, so aiAvailable recovering
* restores the user's AI choice). */
function useMode$1() {
	const [mode, setMode] = useState(() => typeof localStorage === "undefined" ? "traditional" : localStorage.getItem(MODE_KEY$1) === "ai" ? "ai" : "traditional");
	const setModeAndStore = (m) => {
		setMode(m);
		localStorage.setItem(MODE_KEY$1, m);
	};
	return [mode, setModeAndStore];
}
function Home() {
	usePageTitle("");
	const { route } = useLocation();
	const [q, setQ] = useState("");
	const [mode, setMode] = useMode$1();
	const { available: aiAvailable, models, error: modelsError } = useModels();
	const effectiveMode = mode === "ai" && aiAvailable === false ? "traditional" : mode;
	const submit = (query) => {
		const trimmed = query.trim();
		if (!trimmed) return;
		route(effectiveMode === "ai" ? `/search?q=${encodeURIComponent(trimmed)}&mode=ai` : `/search?q=${encodeURIComponent(trimmed)}`);
	};
	return /* @__PURE__ */ jsxs(Center, {
		vh: true,
		children: [
			/* @__PURE__ */ jsx("h1", {
				class: "text-5xl font-semibold tracking-tight mb-4",
				children: "oxe"
			}),
			/* @__PURE__ */ jsx("p", {
				class: "opacity-50 text-sm mb-6 max-md:mb-4 md:mb-10",
				children: "your local web intel layer"
			}),
			/* @__PURE__ */ jsx("div", {
				class: "self-stretch flex justify-center px-3 min-w-0 mb-8",
				children: /* @__PURE__ */ jsx(SearchBox, {
					value: q,
					onInput: setQ,
					onSubmit: submit,
					autoFocus: true,
					size: "lg",
					mode: effectiveMode,
					onModeChange: setMode,
					aiAvailable,
					models,
					modelsError
				})
			})
		]
	});
}
var MODE_KEY$1;
var init_routes = __esmMin((() => {
	init_Header();
	init_ModeSegments();
	init_SearchBox();
	MODE_KEY$1 = "oxe-mode";
}));
//#endregion
//#region src/features/search/ResultCard.tsx
/** Card-less Google-anatomy result: favicon + domain, blue title,
* two-line snippet, collapsed cached text preview. */
function ResultCard({ result, onOpen }) {
	const url = result.url ?? "";
	const domain = domainOf(url);
	const snippet = (result.text || result.highlights?.join(" ") || "").trim();
	const title = result.title || "(untitled)";
	return /* @__PURE__ */ jsxs("article", {
		class: "py-3",
		children: [
			/* @__PURE__ */ jsxs("div", {
				class: "flex items-center gap-2 text-[13px] opacity-70",
				children: [/* @__PURE__ */ jsx("img", {
					src: faviconFor(url),
					alt: "",
					width: 16,
					height: 16,
					loading: "lazy",
					class: "inline-block",
					onError: (e) => e.target.style.display = "none"
				}), /* @__PURE__ */ jsx("span", {
					class: "truncate",
					children: domain
				})]
			}),
			/* @__PURE__ */ jsx("h3", {
				class: "text-lg leading-snug my-0.5",
				children: /* @__PURE__ */ jsx("a", {
					href: url,
					target: "_blank",
					rel: "noopener noreferrer",
					class: "link link-primary no-underline font-medium",
					onClick: () => onOpen(result),
					"data-result-id": result.id || url,
					children: truncate(title, 120)
				})
			}),
			snippet && /* @__PURE__ */ jsx("p", {
				class: "text-sm opacity-80 line-clamp-2 m-0",
				children: truncate(snippet, 200)
			}),
			snippet && /* @__PURE__ */ jsxs("details", {
				class: "text-sm",
				children: [/* @__PURE__ */ jsx("summary", {
					class: "opacity-50 cursor-pointer select-none text-[13px]",
					children: "cached page text preview"
				}), /* @__PURE__ */ jsx("p", {
					class: "opacity-70 m-1 whitespace-pre-wrap",
					children: truncate(snippet, 400)
				})]
			})
		]
	});
}
var init_ResultCard = __esmMin((() => {
	init_format();
}));
//#endregion
//#region src/features/search/useSearch.ts
/** Cache-hit when the server served the payload from the SQLite TTL cache. */
function isCacheHit(payload) {
	return payload?._source === "cache";
}
/** Age (seconds) of a cached payload, from the server-injected `_cached_at`.
* Returns null when absent or in the future (clock skew). */
function cachedAgeOf(payload, now = Date.now() / 1e3) {
	const at = payload?._cached_at;
	if (typeof at !== "number" || at <= 0) return null;
	const age = Math.floor(now - at);
	return age >= 0 ? age : null;
}
/** Continuous scroll: `run` loads page 1, `loadMore` appends the next page
* as the user nears the end. `refresh` bypasses the cache by deleting the
* row first (POST /row/{key}/delete) then re-searching. The `p` URL param
* is deprecated: deep links still resolve but page state is not restored. */
function useSearch() {
	const [state, setState] = useState(initial);
	const abortRef = useRef(null);
	const reqIdRef = useRef(0);
	const qRef = useRef("");
	const fetchPage = (q, p, mode) => {
		const reqId = ++reqIdRef.current;
		qRef.current = q;
		abortRef.current?.abort();
		const ctl = new AbortController();
		abortRef.current = ctl;
		if (mode !== "more") setState((s) => ({
			...s,
			status: mode === "refresh" ? "refreshing" : "loading",
			loading: true,
			error: null
		}));
		else setState((s) => ({
			...s,
			loadingMore: true,
			moreError: false
		}));
		search({
			query: q,
			numResults: PAGE_SIZE,
			page: p
		}, ctl.signal).then((payload) => {
			if (reqId !== reqIdRef.current) return;
			if (mode === "more") setState((s) => {
				if (qRef.current !== q) return s;
				const seen = new Set(s.results.map((r) => r.id || r.url));
				const fresh = payload.results.filter((r) => !seen.has(r.id || r.url));
				return {
					...s,
					results: [...s.results, ...fresh],
					page: p,
					hasNext: payload.results.length > 0 && p < 10,
					loadingMore: false,
					moreError: false
				};
			});
			else setState({
				payload,
				results: payload.results,
				status: nextStatus(payload),
				loading: false,
				error: nextError(payload),
				page: 1,
				hasNext: payload.results.length > 0 && true,
				loadingMore: false,
				moreError: false
			});
		}).catch((e) => {
			if (reqId !== reqIdRef.current) return;
			if (e?.name === "AbortError") return;
			const message = e?.message ?? "search failed";
			if (mode === "more") setState((s) => ({
				...s,
				loadingMore: false,
				moreError: true
			}));
			else setState((s) => ({
				...s,
				status: "error",
				loading: false,
				error: { message }
			}));
		});
	};
	const run = (q) => {
		setState(initial());
		fetchPage(q, 1, "initial");
	};
	const loadMore = useCallback((q) => {
		if (qRef.current !== q) return;
		if (!state.hasNext || state.loadingMore) return;
		fetchPage(q, state.page + 1, "more");
	}, [
		state.hasNext,
		state.loadingMore,
		state.moreError,
		state.page
	]);
	const refresh = (q) => {
		const key = state.payload?._q_hash;
		if (key) deleteCacheRow(key).catch(() => void 0).finally(() => fetchPage(q, 1, "refresh"));
		else fetchPage(q, 1, "refresh");
	};
	return {
		state,
		run,
		loadMore,
		refresh
	};
}
function metaLine(payload, total) {
	if (!payload && total === 0) return "";
	return `${total} result${total === 1 ? "" : "s"}`;
}
/** Terminal status derived from a successful search response: an empty
* page carrying `_error` is an error, not a clean empty. */
function nextStatus(payload) {
	if (payload.results.length === 0 && payload._error) return "error";
	return payload.results.length === 0 ? "empty" : "success";
}
function nextError(payload) {
	if (!payload._error) return null;
	const kind = payload._error_kind;
	return {
		message: payload._error,
		kind: kind === "rate_limited" || kind === "timeout" || kind === "backend_error" ? kind : void 0
	};
}
var PAGE_SIZE, initial;
var init_useSearch = __esmMin((() => {
	init_api();
	PAGE_SIZE = 10;
	initial = () => ({
		payload: null,
		results: [],
		status: "idle",
		loading: false,
		error: null,
		page: 0,
		hasNext: false,
		loadingMore: false,
		moreError: false
	});
}));
//#endregion
//#region src/features/search/index.ts
var init_search$1 = __esmMin((() => {
	init_useSearch();
}));
//#endregion
//#region src/features/search/pager.ts
/** Rebuild /search url from params, preserving anything not derived.
* The `p` (page) param is deprecated: continuous scroll owns pagination,
* generated links never carry it, and deep links that do are ignored. */
function searchUrl(params) {
	const sp = new URLSearchParams();
	sp.set("q", params.q);
	if (params.mode === "ai") sp.set("mode", "ai");
	for (const [k, v] of Object.entries(params.extra ?? {})) sp.set(k, v);
	return `/search?${sp.toString()}`;
}
var init_pager = __esmMin((() => {}));
//#endregion
//#region src/features/answer/MarkdownLite.tsx
/** Tiny markdown-lite inline renderer: `code`, **bold**, *italic*, [n] citations. */
function renderInline(text, keyBase) {
	const out = [];
	const re = /(`[^`]+`|\*\*[^*]+\*\*|\*[^*]+\*|\[\d+\])/g;
	let last = 0;
	let m;
	let i = 0;
	while ((m = re.exec(text)) !== null) {
		if (m.index > last) out.push(/* @__PURE__ */ jsx("span", { children: text.slice(last, m.index) }, `${keyBase}-t${i++}`));
		const tok = m[0];
		if (tok.startsWith("`")) out.push(/* @__PURE__ */ jsx("code", {
			class: "bg-base-200 px-1 rounded text-[0.9em]",
			children: tok.slice(1, -1)
		}, `${keyBase}-c${i++}`));
		else if (tok.startsWith("**")) out.push(/* @__PURE__ */ jsx("strong", { children: tok.slice(2, -2) }, `${keyBase}-b${i++}`));
		else if (tok.startsWith("*")) out.push(/* @__PURE__ */ jsx("em", { children: tok.slice(1, -1) }, `${keyBase}-i${i++}`));
		else {
			const n = Number(tok.slice(1, -1));
			out.push(/* @__PURE__ */ jsx("a", {
				href: `#src-${n}`,
				class: "citation text-primary text-[0.75em] align-super ml-0.5",
				onClick: (e) => {
					e.preventDefault();
					const el = document.getElementById(`src-${n}`);
					if (!el) return;
					el.scrollIntoView({
						behavior: "smooth",
						block: "nearest",
						inline: "center"
					});
					el.classList.add("outline", "outline-primary");
					setTimeout(() => {
						if (el.isConnected) el.classList.remove("outline", "outline-primary");
					}, 1200);
				},
				children: n
			}, `${keyBase}-r${i++}`));
		}
		last = m.index + tok.length;
	}
	if (last < text.length) out.push(/* @__PURE__ */ jsx("span", { children: text.slice(last) }, `${keyBase}-t${i++}`));
	return out;
}
/** Minimal block-level markdown: paragraphs, bullet lists, headings. */
function MarkdownLite({ text }) {
	const blocks = [];
	const lines = text.split("\n");
	let para = [];
	let list = [];
	let k = 0;
	const flushPara = () => {
		if (para.length) {
			blocks.push(/* @__PURE__ */ jsx("p", {
				class: "leading-relaxed whitespace-pre-wrap",
				children: renderInline(para.join(" "), `p${k}`)
			}, `p${k++}`));
			para = [];
		}
	};
	const flushList = () => {
		if (list.length) {
			blocks.push(/* @__PURE__ */ jsx("ul", {
				class: "list-disc pl-5 space-y-1 my-2",
				children: list.map((li, j) => /* @__PURE__ */ jsx("li", { children: renderInline(li, `li${k}-${j}`) }, j))
			}, `ul${k++}`));
			list = [];
		}
	};
	for (const line of lines) {
		const t = line.trim();
		if (!t) {
			flushPara();
			flushList();
		} else if (/^[-*]\s+/.test(t)) {
			flushPara();
			list.push(t.replace(/^[-*]\s+/, ""));
		} else if (/^#{1,3}\s+/.test(t)) {
			flushPara();
			flushList();
			blocks.push(/* @__PURE__ */ jsx("h3", {
				class: "font-semibold mt-3 mb-1 text-base",
				children: renderInline(t.replace(/^#{1,3}\s+/, ""), `h${k}`)
			}, `h${k++}`));
		} else {
			if (list.length) flushList();
			para.push(t);
		}
	}
	flushPara();
	flushList();
	return /* @__PURE__ */ jsx("div", {
		class: "text-[15px] space-y-2",
		children: blocks
	});
}
var init_MarkdownLite = __esmMin((() => {}));
//#endregion
//#region src/features/answer/SourceCard.tsx
/** Compact horizontal tile: number badge, favicon, domain, truncated title. */
function SourceCard({ source, n, queryHash }) {
	let domain = source.url;
	try {
		domain = new URL(source.url).hostname;
	} catch {}
	return /* @__PURE__ */ jsx("a", {
		id: `src-${n}`,
		href: source.url,
		target: "_blank",
		rel: "noopener noreferrer",
		onClick: () => recordClick({
			query_hash: queryHash,
			result_id: `src-${n}`,
			url: source.url,
			title: source.title ?? ""
		}),
		class: "card card-compact bg-base-200 border border-base-300 w-[150px] shrink-0 snap-start hover:opacity-90 transition-opacity",
		children: /* @__PURE__ */ jsxs("div", {
			class: "card-body p-2.5 gap-1",
			children: [/* @__PURE__ */ jsxs("div", {
				class: "flex items-center gap-1.5 text-[11px] opacity-70",
				children: [
					/* @__PURE__ */ jsx("span", {
						class: "badge badge-xs badge-primary font-mono",
						children: n
					}),
					/* @__PURE__ */ jsx("img", {
						src: `https://icons.duckduckgo.com/ip3/${domain}.ico`,
						alt: "",
						width: 12,
						height: 12,
						loading: "lazy",
						onError: (e) => e.currentTarget.style.display = "none"
					}),
					/* @__PURE__ */ jsx("span", {
						class: "truncate",
						children: domain
					})
				]
			}), /* @__PURE__ */ jsx("p", {
				class: "text-[12px] leading-snug line-clamp-2",
				children: source.title
			})]
		})
	});
}
var init_SourceCard = __esmMin((() => {
	init_api();
}));
//#endregion
//#region src/features/answer/AnswerView.tsx
/** AI mode surface: answer, tool steps, sources row, related questions. */
function AnswerView({ query, state, onStop, onRetry, onAskRelated, onViewClassic }) {
	const { text, steps, sources, status, cached, error, relatedQuestions } = state;
	const streaming = status === "idle" || status === "streaming";
	const done = status === "done" || status === "stopped" || status === "error";
	const stopped = status === "stopped";
	const emptySources = done && !error && sources.length === 0 && !text;
	return /* @__PURE__ */ jsxs("div", {
		class: "pt-2 flex flex-col gap-5 animate-in fade-in slide-in-from-bottom-2 duration-300",
		children: [
			/* @__PURE__ */ jsxs("div", {
				class: "flex flex-wrap items-baseline gap-x-3 gap-y-1",
				children: [
					/* @__PURE__ */ jsx("h2", {
						class: "text-xs font-semibold tracking-widest uppercase opacity-60",
						children: "answer"
					}),
					cached && /* @__PURE__ */ jsx("span", {
						class: "badge badge-ghost badge-xs",
						children: "from cache"
					}),
					streaming && /* @__PURE__ */ jsx("span", {
						class: "text-xs opacity-50",
						children: "streaming…"
					}),
					/* @__PURE__ */ jsxs("span", {
						class: "ml-auto flex gap-2",
						children: [streaming && /* @__PURE__ */ jsx("button", {
							type: "button",
							class: "btn btn-ghost btn-xs",
							onClick: onStop,
							children: "stop"
						}), done && /* @__PURE__ */ jsx("button", {
							type: "button",
							class: "btn btn-ghost btn-xs",
							onClick: onViewClassic,
							children: "view classic"
						})]
					})
				]
			}),
			steps.length > 0 && /* @__PURE__ */ jsx("ul", {
				class: "text-[13px] opacity-70 space-y-1",
				"aria-live": "polite",
				children: steps.map((s, i) => /* @__PURE__ */ jsxs("li", {
					class: "flex items-center gap-2 animate-in fade-in slide-in-from-bottom-2 duration-300",
					children: [!done || i < steps.length - 1 ? /* @__PURE__ */ jsx("span", { class: "loading loading-spinner loading-xs" }) : /* @__PURE__ */ jsx("span", {
						"aria-hidden": "true",
						children: "·"
					}), /* @__PURE__ */ jsx("span", { children: s })]
				}, i))
			}),
			error ? /* @__PURE__ */ jsxs("div", {
				class: "text-sm flex flex-col gap-2",
				children: [
					text && /* @__PURE__ */ jsx("div", {
						class: "mb-1 opacity-80",
						children: /* @__PURE__ */ jsx(MarkdownLite, { text })
					}),
					/* @__PURE__ */ jsx("div", {
						role: "alert",
						class: "alert alert-error animate-in fade-in zoom-in-95 duration-300",
						children: /* @__PURE__ */ jsxs("span", { children: ["stream interrupted - ", error] })
					}),
					/* @__PURE__ */ jsxs("div", {
						class: "flex gap-2",
						children: [/* @__PURE__ */ jsx("button", {
							type: "button",
							class: "btn btn-sm",
							onClick: onRetry,
							children: "retry"
						}), /* @__PURE__ */ jsx("button", {
							type: "button",
							class: "btn btn-ghost btn-sm",
							onClick: onViewClassic,
							children: "switch to classic results"
						})]
					})
				]
			}) : emptySources ? /* @__PURE__ */ jsxs("div", {
				class: "text-sm animate-in fade-in zoom-in-95 duration-300",
				children: [/* @__PURE__ */ jsx("p", {
					class: "opacity-60 mb-2",
					children: "no sources found for this query - try fewer words, or"
				}), /* @__PURE__ */ jsx("button", {
					type: "button",
					class: "btn btn-sm",
					onClick: onViewClassic,
					children: "switch to classic results"
				})]
			}) : text ? /* @__PURE__ */ jsxs("div", { children: [
				/* @__PURE__ */ jsx(MarkdownLite, { text }),
				streaming && /* @__PURE__ */ jsx("span", {
					class: "animate-pulse font-mono",
					"aria-hidden": "true",
					children: "▌"
				}),
				stopped && /* @__PURE__ */ jsx("p", {
					class: "text-xs opacity-50 mt-1",
					children: "stopped"
				})
			] }) : streaming && steps.length === 0 ? /* @__PURE__ */ jsxs("div", {
				class: "flex flex-col gap-3 skeleton-shimmer",
				"aria-busy": "true",
				children: [
					/* @__PURE__ */ jsx("div", { class: "skeleton h-4 w-11/12" }),
					/* @__PURE__ */ jsx("div", { class: "skeleton h-4 w-full" }),
					/* @__PURE__ */ jsx("div", { class: "skeleton h-4 w-3/4" }),
					/* @__PURE__ */ jsx("div", {
						class: "flex gap-2 overflow-hidden",
						children: [
							0,
							1,
							2
						].map((i) => /* @__PURE__ */ jsx("div", { class: "skeleton h-16 w-44 shrink-0" }, i))
					})
				]
			}) : null,
			sources.length > 0 && /* @__PURE__ */ jsxs("div", { children: [/* @__PURE__ */ jsx("h2", {
				class: "text-xs font-semibold tracking-widest uppercase opacity-60 mb-2",
				children: "sources"
			}), /* @__PURE__ */ jsx("div", {
				class: "flex gap-2 overflow-x-auto pb-2 snap-x -mx-4 px-4",
				children: sources.map((s, i) => /* @__PURE__ */ jsx("div", {
					class: "animate-in fade-in slide-in-from-bottom-2 duration-300",
					style: { animationDelay: `${i * 40}ms` },
					children: /* @__PURE__ */ jsx(SourceCard, {
						source: s,
						n: i + 1,
						queryHash: query
					})
				}, s.url))
			})] }),
			done && !error && relatedQuestions.length > 0 && /* @__PURE__ */ jsxs("div", {
				class: "animate-in fade-in duration-300",
				children: [/* @__PURE__ */ jsx("h2", {
					class: "text-xs font-semibold tracking-widest uppercase opacity-60 mb-2",
					children: "related"
				}), /* @__PURE__ */ jsx("ul", {
					class: "space-y-1.5",
					children: relatedQuestions.map((rq) => /* @__PURE__ */ jsx("li", { children: /* @__PURE__ */ jsx("button", {
						type: "button",
						class: "text-left text-sm hover:text-primary hover:underline",
						onClick: () => onAskRelated(rq),
						children: rq
					}) }, rq))
				})]
			}),
			!text && !error && !emptySources && done && /* @__PURE__ */ jsx(Empty$1, { children: "no answer produced" })
		]
	});
}
var init_AnswerView = __esmMin((() => {
	init_MarkdownLite();
	init_SourceCard();
	init_Header();
}));
//#endregion
//#region src/features/answer/useAnswer.ts
/** Pure SSE event reducer so event handling is testable without a stream.
* The first event transitions idle -> streaming; done/error terminalize. */
function applyAnswerEvent(state, ev) {
	if (ev.type === "step") return {
		...state,
		status: "streaming",
		steps: [...state.steps, ev.label]
	};
	if (ev.type === "delta") return {
		...state,
		status: "streaming",
		text: state.text + ev.text
	};
	if (ev.type === "sources") return {
		...state,
		status: "streaming",
		sources: ev.sources
	};
	if (ev.type === "done") return {
		...state,
		text: ev.answer || state.text,
		status: state.status === "stopped" ? "stopped" : ev.error ? "error" : "done",
		cached: ev.cached ?? false,
		confidence: ev.confidence,
		relatedQuestions: ev.related_questions ?? [],
		error: ev.error ?? null
	};
	return ev;
}
/** Owns the SSE answer stream lifecycle for one query run. */
function useAnswer() {
	const [state, setState] = useState(INITIAL);
	const abortRef = useRef(null);
	const stoppedRef = useRef(false);
	useMountEffect(function abortStreamOnUnmount() {
		return () => abortRef.current?.abort();
	});
	return {
		state,
		run: useCallback(function runAnswerStream(query) {
			abortRef.current?.abort();
			const ctl = new AbortController();
			abortRef.current = ctl;
			stoppedRef.current = false;
			setState(INITIAL);
			const on = (ev) => {
				setState((s) => applyAnswerEvent(s, ev));
			};
			streamAnswer(query, on, ctl.signal).catch((e) => {
				if (e?.name === "AbortError") {
					setState((s) => stoppedRef.current ? s : {
						...s,
						status: "stopped"
					});
					return;
				}
				setState((s) => ({
					...s,
					status: "error",
					error: e?.message ?? "answer failed"
				}));
			});
		}, []),
		stop: useCallback(function stopAnswerStream() {
			stoppedRef.current = true;
			abortRef.current?.abort();
			setState((s) => ({
				...s,
				status: "stopped"
			}));
		}, [])
	};
}
var INITIAL;
var init_useAnswer = __esmMin((() => {
	init_ai();
	init_useMountEffect();
	INITIAL = {
		status: "idle",
		text: "",
		steps: [],
		sources: [],
		cached: false,
		confidence: 0,
		relatedQuestions: [],
		error: null
	};
}));
//#endregion
//#region src/routes/search.tsx
var search_exports = /* @__PURE__ */ __exportAll({ default: () => SearchRoute });
/** Set mode and persist it at event time (no sync effect). */
function useMode(initial) {
	const [mode, setMode] = useState(initial);
	const setModeAndStore = (m) => {
		setMode(m);
		localStorage.setItem(MODE_KEY, m);
	};
	return [mode, setModeAndStore];
}
function SearchRoute() {
	const { query, route } = useLocation();
	const q = String(query?.q ?? "");
	const urlMode = query?.mode === "ai" ? "ai" : "traditional";
	usePageTitle(q || "search");
	const aiAvailable = useAiAvailable();
	const { models, error: modelsError } = useModels();
	const [input, setInput] = useState(q);
	const [mode, setMode] = useMode(urlMode);
	const { state, run, loadMore, refresh } = useSearch();
	const answer = useAnswer();
	const virtuaRef = useRef(null);
	const aiModeBlocked = mode === "ai" && aiAvailable === false;
	const effectiveMode = aiModeBlocked ? "traditional" : mode;
	useEffect(function stripDeprecatedPageParam() {
		if (typeof query?.p === "string") {
			const sp = new URLSearchParams(window.location.search);
			sp.delete("p");
			const qs = sp.toString();
			route(`${window.location.pathname}${qs ? `?${qs}` : ""}`, true);
		}
	}, [query?.p]);
	useEffect(function rerunOnQueryChange() {
		setInput(q);
		if (q && effectiveMode === "traditional") run(q);
	}, [q]);
	const maybeLoadMore = () => {
		const v = virtuaRef.current;
		const n = state.results.length;
		if (!v || n === 0) return;
		const last = n - 1;
		const itemH = v.getItemSize(last) || 140;
		if (v.getItemOffset(last) + itemH - (window.scrollY + v.viewportSize) < 2 * itemH) loadMore(q);
	};
	useEffect(function checkLoadMoreAfterResults() {
		if (state.results.length > 0 && !state.loadingMore) {
			const t = setTimeout(maybeLoadMore, 0);
			return () => clearTimeout(t);
		}
	}, [state.results.length, state.loadingMore]);
	const changeMode = (m) => {
		setMode(m);
		if (urlMode !== m) {
			const extra = {};
			if (typeof query?.settings === "string") extra.settings = query.settings;
			route(searchUrl({
				q,
				mode: m,
				extra
			}), true);
		}
	};
	useEffect(function rerunOnQueryChange() {
		setInput(q);
		if (q && effectiveMode === "traditional") run(q);
	}, [q]);
	useEffect(function runAnswerOnQueryOrModeChange() {
		if (q && effectiveMode === "ai") answer.run(q);
	}, [q, effectiveMode]);
	const submit = (raw) => {
		const t = raw.trim();
		if (!t) return;
		const extra = {};
		if (typeof query?.settings === "string") extra.settings = query.settings;
		route(searchUrl({
			q: t,
			mode,
			extra
		}));
	};
	const askAi = (query) => {
		setMode("ai");
		route(`/search?q=${encodeURIComponent(query)}&mode=ai`);
	};
	const viewClassic = () => {
		setMode("traditional");
		route(`/search?q=${encodeURIComponent(q)}`);
	};
	const { payload, loading, error, results } = state;
	const qHash = payload?._q_hash ?? "";
	return /* @__PURE__ */ jsxs("div", {
		class: "w-full max-w-[652px] mx-auto px-4 pb-16",
		children: [/* @__PURE__ */ jsxs("div", {
			class: "pt-4 flex flex-col gap-3",
			children: [
				/* @__PURE__ */ jsx(SearchBox, {
					value: input,
					onInput: setInput,
					onSubmit: submit,
					busy: effectiveMode === "ai" ? answer.state.status === "idle" || answer.state.status === "streaming" : loading,
					size: "md",
					mode,
					onModeChange: changeMode,
					aiAvailable,
					models,
					modelsError
				}),
				aiModeBlocked && /* @__PURE__ */ jsx("p", {
					class: "text-xs opacity-60 mt-1",
					role: "note",
					children: "AI mode is not configured - set a model in settings"
				}),
				effectiveMode === "traditional" && results.length > 0 && payload && /* @__PURE__ */ jsxs("div", {
					class: "flex flex-wrap items-center gap-x-3 gap-y-1 text-[13px]",
					children: [
						/* @__PURE__ */ jsx("span", {
							class: "opacity-60",
							children: metaLine(payload, results.length) || (loading ? "searching…" : "")
						}),
						isCacheHit(payload) && /* @__PURE__ */ jsx("span", {
							class: "tooltip",
							"data-tip": "Actually search the web (refreshes this cache entry)",
							children: /* @__PURE__ */ jsxs("button", {
								type: "button",
								class: "badge badge-sm badge-ghost cursor-pointer",
								"aria-label": "cached result: click to refresh from the web",
								onClick: () => {
									refresh(q);
									toast("success", "refreshed from the web");
								},
								children: ["cached", (() => {
									const age = cachedAgeOf(payload);
									return age != null ? ` · ${fmtDur(age)} old` : "";
								})()]
							})
						}),
						payload && /* @__PURE__ */ jsxs("span", {
							class: "flex gap-2 ml-auto",
							children: [/* @__PURE__ */ jsx("button", {
								type: "button",
								class: "btn btn-ghost btn-xs",
								onClick: () => navigator.clipboard?.writeText(window.location.href),
								children: "copy link"
							}), /* @__PURE__ */ jsx("button", {
								type: "button",
								class: "btn btn-ghost btn-xs",
								onClick: () => navigator.clipboard?.writeText(JSON.stringify({
									requestId: payload.requestId,
									results,
									costDollars: payload.costDollars
								})),
								children: "copy json"
							})]
						})
					]
				})
			]
		}), effectiveMode === "ai" ? /* @__PURE__ */ jsx(AnswerView, {
			query: q,
			state: answer.state,
			onStop: answer.stop,
			onRetry: () => answer.run(q),
			onAskRelated: askAi,
			onViewClassic: viewClassic
		}) : /* @__PURE__ */ jsxs(Fragment, { children: [
			loading && /* @__PURE__ */ jsx("div", {
				class: "py-6 flex flex-col divide-y divide-base-300",
				"aria-busy": "true",
				children: [
					0,
					1,
					2,
					3
				].map((i) => /* @__PURE__ */ jsxs("div", {
					class: "py-3 flex flex-col gap-1.5",
					children: [
						/* @__PURE__ */ jsxs("div", {
							class: "flex items-center gap-2",
							children: [/* @__PURE__ */ jsx("div", { class: "skeleton size-4 rounded-sm" }), /* @__PURE__ */ jsx("div", { class: "skeleton h-3 w-28" })]
						}),
						/* @__PURE__ */ jsx("div", { class: "skeleton h-5 w-2/3" }),
						/* @__PURE__ */ jsx("div", { class: "skeleton h-3.5 w-full" }),
						/* @__PURE__ */ jsx("div", { class: "skeleton h-3.5 w-11/12" })
					]
				}, i))
			}),
			!loading && error && /* @__PURE__ */ jsxs("div", {
				class: "py-6 flex flex-col gap-3 animate-in fade-in zoom-in-95 duration-300",
				children: [error.kind === "rate_limited" ? /* @__PURE__ */ jsx("div", {
					role: "alert",
					class: "alert alert-warning text-sm",
					children: /* @__PURE__ */ jsx("span", { children: "search backend rate-limited, retry shortly" })
				}) : /* @__PURE__ */ jsx("div", {
					role: "alert",
					class: "alert alert-error text-sm",
					children: /* @__PURE__ */ jsx("span", { children: "search backend failed" })
				}), /* @__PURE__ */ jsxs("div", {
					class: "flex gap-2",
					children: [/* @__PURE__ */ jsx("button", {
						type: "button",
						class: "btn btn-sm",
						onClick: () => run(q),
						children: "retry"
					}), aiAvailable === true && /* @__PURE__ */ jsx("button", {
						type: "button",
						class: "btn btn-ghost btn-sm",
						onClick: () => askAi(q),
						children: "ask AI instead"
					})]
				})]
			}),
			!loading && !error && payload && results.length === 0 && /* @__PURE__ */ jsxs("div", {
				class: "py-10 text-sm animate-in fade-in zoom-in-95 duration-300",
				children: [/* @__PURE__ */ jsx("p", {
					class: "opacity-60 mb-3",
					children: "no results"
				}), aiAvailable === true && /* @__PURE__ */ jsx("button", {
					type: "button",
					class: "btn btn-sm",
					onClick: () => askAi(q),
					children: "ask AI instead"
				})]
			}),
			!loading && results.length > 0 && /* @__PURE__ */ jsxs(Fragment, { children: [
				/* @__PURE__ */ jsx(WindowVirtualizer, {
					ref: virtuaRef,
					data: results,
					onScroll: maybeLoadMore,
					children: (r, i) => /* @__PURE__ */ jsx("div", {
						class: "animate-in fade-in slide-in-from-bottom-2 duration-300",
						style: {
							"--i": i,
							animationDelay: `calc(var(--i) * 40ms)`
						},
						children: /* @__PURE__ */ jsx(ResultCard, {
							result: r,
							queryHash: qHash,
							onOpen: (res) => recordClick({
								query_hash: qHash,
								result_id: res.id || res.url || "",
								url: res.url ?? "",
								title: res.title ?? ""
							})
						})
					}, r.id || r.url)
				}),
				state.loadingMore && /* @__PURE__ */ jsx("div", {
					class: "py-6 flex justify-center",
					"aria-busy": "true",
					role: "status",
					children: /* @__PURE__ */ jsx("span", {
						class: "loading loading-dots loading-sm opacity-50",
						"aria-label": "loading"
					})
				}),
				!state.hasNext && !state.loadingMore && !state.moreError && /* @__PURE__ */ jsx("p", {
					class: "py-6 text-center text-sm opacity-40",
					children: "end of results"
				}),
				state.moreError && /* @__PURE__ */ jsxs("div", {
					class: "py-6 flex flex-col items-center gap-2 text-sm",
					children: [/* @__PURE__ */ jsx("p", {
						class: "opacity-60",
						children: "couldn’t load more results"
					}), /* @__PURE__ */ jsx("button", {
						type: "button",
						class: "btn btn-ghost btn-sm",
						onClick: () => loadMore(q),
						children: "retry"
					})]
				}),
				state.hasNext && !state.loadingMore && !state.moreError && /* @__PURE__ */ jsx("div", {
					class: "py-6 flex justify-center",
					children: /* @__PURE__ */ jsx("button", {
						type: "button",
						class: "btn btn-ghost btn-sm opacity-60",
						onClick: () => loadMore(q),
						children: "more results"
					})
				})
			] })
		] })]
	});
}
var MODE_KEY;
var init_search = __esmMin((() => {
	init_Header();
	init_ModeSegments();
	init_api();
	init_Toasts();
	init_ResultCard();
	init_search$1();
	init_pager();
	init_format();
	init_AnswerView();
	init_useAnswer();
	init_SearchBox();
	MODE_KEY = "oxe-mode";
}));
//#endregion
//#region src/app.tsx
var pages = /* #__PURE__ */ Object.assign({
	"./routes/404.tsx": () => Promise.resolve().then(() => (init__404(), _404_exports)),
	"./routes/dashboard.tsx": () => Promise.resolve().then(() => (init_dashboard(), dashboard_exports)),
	"./routes/history.tsx": () => Promise.resolve().then(() => (init_history(), history_exports)),
	"./routes/index.tsx": () => Promise.resolve().then(() => (init_routes(), routes_exports)),
	"./routes/search.tsx": () => Promise.resolve().then(() => (init_search(), search_exports))
});
var isPage = (file) => !file.split("/").pop().startsWith("_");
function routePath(file) {
	return file.replace("./routes", "").replace(/\.tsx$/, "").replace(/\/index$/, "").replace(/\[(\w+)\]/g, ":$1") || "/";
}
/** Layout resolution: the statically imported root _layout.tsx applies to
* every page. If segment layouts (routes/<seg>/_layout.tsx) are ever added,
* restore an eager glob here (excluding the root file) with
* longest-prefix-match resolution (glob "routes/<seg>/_layout.tsx").
*/
function layoutFor(_pagePath) {
	return Layout;
}
var pageRoutes = Object.entries(pages).filter(([file]) => isPage(file)).map(([file, load]) => {
	const path = routePath(file);
	const Layout = layoutFor(file);
	return {
		path,
		Component: lazy(async () => {
			const Page = (await load()).default;
			const Wrapped = (props) => /* @__PURE__ */ jsx(Layout, { children: /* @__PURE__ */ jsx(Page, { ...props }) });
			return { default: Wrapped };
		}),
		isDefault: path === "/404"
	};
});
function App({ url }) {
	const regular = pageRoutes.filter((r) => !r.isDefault);
	const fallback = pageRoutes.filter((r) => r.isDefault);
	const wrap = (Component) => {
		const C = Component;
		return (props) => /* @__PURE__ */ jsx(ErrorBoundary, { children: /* @__PURE__ */ jsx(C, { ...props }) });
	};
	return /* @__PURE__ */ jsx(LocationProvider, {
		...url ? { url } : {},
		children: /* @__PURE__ */ jsxs(Router, { children: [regular.map(({ path, Component }) => /* @__PURE__ */ jsx(Route, {
			path,
			component: wrap(Component)
		}, path)), fallback.map(({ Component }) => /* @__PURE__ */ jsx(Route, {
			default: true,
			component: wrap(Component)
		}, "404"))] })
	});
}
//#endregion
//#region src/entry-prerender.tsx
/** Render the App for `url` to { html, links }. Routes with dynamic content
* (search results, history) prerender only the shell; data loads client-side
* on hydration. */
async function prerenderApp(url) {
	locationStub(url);
	return await prerender(/* @__PURE__ */ jsx(App, { url }));
}
//#endregion
export { prerenderApp };
