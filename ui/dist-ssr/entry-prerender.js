import prerender, { locationStub } from "preact-iso/prerender";
import { ErrorBoundary, lazy } from "preact-iso";
import { Component, toChildArray } from "preact";
import { useCallback, useEffect, useId, useRef, useState } from "preact/hooks";
import * as v from "valibot";
import { createRouter, getPagePath, openPage, redirectPage } from "@nanostores/router";
import { useStore } from "@nanostores/preact";
import { Fragment, jsx, jsxs } from "preact/jsx-runtime";
import { atom } from "nanostores";
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
//#endregion
//#region src/paraglide/runtime.js
/**
* @param {typeof strategy} strategyToUse
* @param {string | undefined} urlForUrlStrategy
* @returns {Locale | undefined}
*/
function resolveLocaleWithStrategies(strategyToUse, urlForUrlStrategy) {
	/** @type {string | undefined} */
	let locale;
	for (const strat of strategyToUse) {
		if (strat === "baseLocale") locale = "en";
		else if (isCustomStrategy(strat) && customClientStrategies.has(strat)) {
			const handler = customClientStrategies.get(strat);
			if (handler) {
				const result = handler.getLocale();
				if (result instanceof Promise) continue;
				if (result !== void 0) return assertIsLocale(result);
			}
		}
		const matchedLocale = toLocale(locale);
		if (matchedLocale) return matchedLocale;
	}
}
/**
* Coerces a locale-like string to the canonical locale value used by the runtime.
*
* @param {unknown} value
* @returns {Locale | undefined}
*/
function toLocale(value) {
	if (typeof value !== "string") return;
	const lowerValue = value.toLowerCase();
	for (const locale of locales) if (locale.toLowerCase() === lowerValue) return locale;
}
/**
* Asserts that the input can be normalized to a locale.
*
* @param {unknown} input - The input to check.
* @returns {Locale} The input normalized to a Locale.
* @throws {Error} If the input is not a locale.
*/
function assertIsLocale(input) {
	const locale = toLocale(input);
	if (locale) return locale;
	throw new Error(`Invalid locale: ${input}. Expected one of: ${locales.join(", ")}`);
}
/**
* Applies the configured trailing slash policy to a URL.
*
* The root pathname always remains `/`. Query parameters and hashes are not
* modified.
*
* @param {URL} url
* @returns {URL}
*/
function normalizeTrailingSlash(url) {
	return url;
}
/**
* Matches a canonical URL while allowing configured patterns to retain their
* existing trailing slash style.
*
* @param {URLPattern} pattern
* @param {URL} url
* @returns {any}
*/
function execUrlPattern(pattern, url) {
	return pattern.exec(url.href);
}
/**
* Low-level URL de-localization function, primarily used in server contexts.
*
* This function is designed for server-side usage where you need precise control
* over URL de-localization, such as in middleware or request handlers. It works with
* URL objects and always returns absolute URLs.
*
* For client-side UI components, use `deLocalizeHref()` instead, which provides
* a more convenient API with relative paths.
*
* @see https://paraglidejs.com/i18n-routing
*
* @example
* ```typescript
* // Server middleware example
* app.use((req, res, next) => {
*   const url = new URL(req.url, `${req.protocol}://${req.headers.host}`);
*   const baseUrl = deLocalizeUrl(url);
*
*   // Store the base URL for later use
*   req.baseUrl = baseUrl;
*   next();
* });
* ```
*
* @example
* ```typescript
* // Using with URL patterns
* const url = new URL("https://example.com/de/about");
* deLocalizeUrl(url); // => URL("https://example.com/about")
*
* // Using with domain-based localization
* const url = new URL("https://de.example.com/store");
* deLocalizeUrl(url); // => URL("https://example.com/store")
* ```
*
* @param {string | URL} url - The URL to de-localize. If string, must be absolute.
* @returns {URL} The de-localized URL, always absolute
*/
function deLocalizeUrl(url) {
	return deLocalizeUrlDefaultPattern(url);
}
/**
* De-localizes a URL using the default pattern (/:locale/*)
* @param {string|URL} url
* @returns {URL}
*/
function deLocalizeUrlDefaultPattern(url) {
	const urlObj = normalizeTrailingSlash(typeof url === "string" ? new URL(url, getUrlOrigin()) : new URL(url));
	const pathSegments = urlObj.pathname.split("/").filter(Boolean);
	if (pathSegments.length > 0 && toLocale(pathSegments[0])) urlObj.pathname = "/" + pathSegments.slice(1).join("/");
	return normalizeTrailingSlash(urlObj);
}
/**
* Match route policy against both the public URL and its canonical URL.
*
* The function is deliberately separate from variables.js: configuration is
* inert data, while canonicalization and route selection form a routing layer.
*
* @param {string | URL} url
* @returns {{ match: string; strategy?: typeof strategy; exclude?: boolean } | undefined}
*/
function findMatchingRouteStrategy(url) {
	if (routeStrategies.length === 0) return;
	const urlString = typeof url === "string" ? url : url.href;
	if (cachedRouteStrategyUrl === urlString) return cachedRouteStrategy;
	const publicUrl = normalizeTrailingSlash(new URL(urlString, "http://example.com"));
	const canonicalUrl = deLocalizeUrl(publicUrl);
	const candidateUrls = canonicalUrl.href === publicUrl.href ? [publicUrl] : [publicUrl, canonicalUrl];
	let match;
	for (const candidateUrl of candidateUrls) {
		for (const routeStrategy of routeStrategies) if (execUrlPattern(new URLPattern(routeStrategy.match, candidateUrl.href), candidateUrl)) {
			match = routeStrategy;
			break;
		}
		if (match) break;
	}
	cachedRouteStrategyUrl = urlString;
	cachedRouteStrategy = match;
	return match;
}
/**
* Returns the strategy to use for a specific URL.
*
* If no route strategy matches (or the matching rule is `exclude: true`),
* the global strategy is returned.
*
* @param {string | URL} url
* @returns {typeof strategy}
*/
function getStrategyForUrl(url) {
	const routeStrategy = findMatchingRouteStrategy(url);
	if (routeStrategy && routeStrategy.exclude !== true && Array.isArray(routeStrategy.strategy)) return routeStrategy.strategy;
	return strategy;
}
/**
* Checks if the given strategy is a custom strategy.
*
* @param {unknown} strategy The name of the custom strategy to validate.
* Must be a string that starts with "custom-" followed by alphanumeric characters, hyphens, or underscores.
* @returns {boolean} Returns true if it is a custom strategy, false otherwise.
*/
function isCustomStrategy(strategy) {
	return typeof strategy === "string" && /^custom-[A-Za-z0-9_-]+$/.test(strategy);
}
var URLPattern, locales, cookieName, strategy, routeStrategies, serverAsyncLocalStorage, isServer, experimentalStaticLocale, localeInitiallySet, getLocale, navigateOrReload, setLocale, getUrlOrigin, cookieNamePattern, cachedRouteStrategyUrl, cachedRouteStrategy, customClientStrategies;
var init_runtime = __esmMin((() => {
	URLPattern = {};
	locales = ["en"];
	cookieName = "PARAGLIDE_LOCALE";
	strategy = ["baseLocale"];
	routeStrategies = [];
	serverAsyncLocalStorage = void 0;
	isServer = typeof window === "undefined";
	experimentalStaticLocale = assertIsLocale("en");
	/** @type {any} */ globalThis.__paraglide = globalThis.__paraglide ?? {};
	/** @type {any} */ globalThis.__paraglide.ssr = globalThis.__paraglide.ssr ?? {};
	localeInitiallySet = false;
	getLocale = () => {
		if (experimentalStaticLocale !== void 0) return experimentalStaticLocale;
		if (serverAsyncLocalStorage) {
			const locale = serverAsyncLocalStorage?.getStore()?.locale;
			if (locale) return locale;
		}
		let strategyToUse = strategy;
		if (!isServer && typeof window !== "undefined" && window.location?.href) strategyToUse = getStrategyForUrl(window.location.href);
		const resolved = resolveLocaleWithStrategies(strategyToUse, typeof window !== "undefined" ? window.location?.href : void 0);
		if (resolved) {
			if (!localeInitiallySet) {
				localeInitiallySet = true;
				setLocale(resolved, { reload: false });
			}
			return resolved;
		}
		throw new Error("No locale found. Read the docs https://paraglidejs.com/errors#no-locale-found");
	};
	navigateOrReload = (newLocation) => {
		if (newLocation) window.location.href = newLocation;
		else window.location.reload();
	};
	setLocale = (newLocale, options) => {
		const optionsWithDefaults = {
			reload: true,
			...options
		};
		if (experimentalStaticLocale !== void 0 && newLocale !== experimentalStaticLocale && optionsWithDefaults.reload === false) {
			console.warn(`Paraglide: setLocale(${JSON.stringify(newLocale)}, { reload: false }) cannot switch away from the statically built locale ${JSON.stringify(experimentalStaticLocale)}. A document navigation is required; reload has been forced to true.`);
			optionsWithDefaults.reload = true;
		}
		/** @type {Locale | undefined} */
		let currentLocale;
		try {
			currentLocale = getLocale();
		} catch {}
		/** @type {Array<Promise<void>>} */
		const customSetLocalePromises = [];
		/** @type {string | undefined} */
		let newLocation = void 0;
		let strategyToUse = strategy;
		if (!isServer && typeof window !== "undefined" && window.location?.href) strategyToUse = getStrategyForUrl(window.location.href);
		for (const strat of strategyToUse) if (strat === "baseLocale") continue;
		else if (isCustomStrategy(strat) && customClientStrategies.has(strat)) {
			const handler = customClientStrategies.get(strat);
			if (handler) {
				let result = handler.setLocale(newLocale);
				if (result instanceof Promise) {
					result = result.catch((error) => {
						throw new Error(`Custom strategy "${strat}" setLocale failed.`, { cause: error });
					});
					customSetLocalePromises.push(result);
				}
			}
		}
		const runReload = () => {
			if (!isServer && optionsWithDefaults.reload && window.location && newLocale !== currentLocale) navigateOrReload(newLocation);
		};
		if (customSetLocalePromises.length) return Promise.all(customSetLocalePromises).then(() => {
			runReload();
		});
		runReload();
	};
	getUrlOrigin = () => {
		if (serverAsyncLocalStorage) return serverAsyncLocalStorage.getStore()?.origin ?? "http://fallback.com";
		else if (typeof window !== "undefined") return window.location.origin;
		return "http://fallback.com";
	};
	cookieNamePattern = cookieName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
	new RegExp(`(?:^|;\\s*)${cookieNamePattern}=([^;]*)`);
	customClientStrategies = /* @__PURE__ */ new Map();
}));
/**
* A locale that is available in the project.
*
* @example
*   setLocale(request.locale as Locale)
*
* @typedef {typeof locales[number]} Locale
*/
/**
* A branded type representing a localized string.
*
* Message functions return this type instead of \`string\`, enabling TypeScript
* to distinguish translated strings from regular strings at compile time.
* This allows you to enforce that only properly localized content is used
* in your UI components.
*
* Since \`LocalizedString\` is a branded subtype of \`string\`, it remains fully
* backward compatible—you can pass it anywhere a \`string\` is expected.
*
* @example
*   // Enforce localized strings in your components
*   function PageTitle(props: { title: LocalizedString }) {
*     return <h1>{props.title}</h1>
*   }
*
*   // ✅ Correct: using a message function
*   <PageTitle title={m.welcome_title()} />
*
*   // ❌ Type error: raw strings are not LocalizedString
*   <PageTitle title="Welcome" />
*
* @example
*   // LocalizedString is assignable to string (backward compatible)
*   const localized: LocalizedString = m.greeting()
*   const str: string = localized  // ✅ works fine
*
*   // But string is not assignable to LocalizedString
*   const raw: LocalizedString = "Hello"  // ❌ Type error
*
* @example
*   // Catches accidental string concatenation
*   function showMessage(msg: LocalizedString) { ... }
*
*   showMessage(m.hello())                    // ✅
*   showMessage("Hello " + userName)          // ❌ Type error
*   showMessage(m.hello_user({ name: userName }))  // ✅ use params instead
*
* @typedef {string & { readonly __brand: 'LocalizedString' }} LocalizedString
*/
/**
* A single markup option passed to a tag instance.
*
* @typedef {{
*   name: string;
*   value: unknown;
* }} MessageMarkupOption
*/
/**
* A single static markup attribute attached to a tag instance.
*
* @typedef {{
*   name: string;
*   value: string | true;
* }} MessageMarkupAttribute
*/
/**
* Record of markup options for a tag instance.
*
* @typedef {Record<string, unknown>} MessageMarkupOptions
*/
/**
* Record of markup attributes for a tag instance.
*
* @typedef {Record<string, string | true>} MessageMarkupAttributes
*/
/**
* Type-level schema for a single markup tag.
*
* @typedef {{
*   options: MessageMarkupOptions;
*   attributes: MessageMarkupAttributes;
*   children: boolean;
* }} MessageMarkupTag
*/
/**
* Type-level schema for all markup tags in a message.
*
* @typedef {Record<string, MessageMarkupTag>} MessageMarkupSchema
*/
/**
* Type-only metadata attached to compiled message functions.
*
* @template Inputs
* @template Options
* @template {MessageMarkupSchema} [Markup = MessageMarkupSchema]
* @typedef {{
*   readonly __paraglide?: {
*     inputs: Inputs;
*     options: Options;
*     markup: Markup;
*   };
* }} MessageMetadata
*/
/**
* A compiled, framework-neutral message part.
*
* @typedef {{
*   type: "text";
*   value: string;
* } | {
*   type: "markup-start";
*   name: string;
*   options: MessageMarkupOptions;
*   attributes: MessageMarkupAttributes;
* } | {
*   type: "markup-end";
*   name: string;
*   options: MessageMarkupOptions;
*   attributes: MessageMarkupAttributes;
* } | {
*   type: "markup-standalone";
*   name: string;
*   options: MessageMarkupOptions;
*   attributes: MessageMarkupAttributes;
* }} MessagePart
*/
/**
* A message function is a message for a specific locale.
*
* @example
*   m.hello({ name: 'world' })
*
* @typedef {(inputs?: Record<string, never>) => LocalizedString} MessageFunction
*/
/**
* A message bundle function that selects the message to be returned.
*
* Uses `getLocale()` under the hood to determine the locale with an option.
*
* @template {string} T
*
* @example
*   *   m.hello({ name: 'world' }, { locale: "en" })
*
* @typedef {(params: Record<string, never>, options: { locale: T }) => LocalizedString} MessageBundleFunction
*/
//#endregion
//#region src/paraglide/messages/about_ai_body.js
var en_about_ai_body, about_ai_body;
var init_about_ai_body = __esmMin((() => {
	init_runtime();
	en_about_ai_body = () => {
		return ` streaming answer with cited sources.`;
	};
	about_ai_body = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_about_ai_body(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/about_ai_label.js
var en_about_ai_label, about_ai_label;
var init_about_ai_label = __esmMin((() => {
	init_runtime();
	en_about_ai_label = () => {
		return `AI:`;
	};
	about_ai_label = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_about_ai_label(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/about_aria.js
var en_about_aria, about_aria;
var init_about_aria = __esmMin((() => {
	init_runtime();
	en_about_aria = () => {
		return `about oxe: caching, MCP API, search modes`;
	};
	about_aria = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_about_aria(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/about_line1.js
var en_about_line1, about_line1;
var init_about_line1 = __esmMin((() => {
	init_runtime();
	en_about_line1 = () => {
		return `search once, share with your agents - cached, MCP-ready · REST + MCP API on :4479`;
	};
	about_line1 = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_about_line1(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/about_search_body.js
var en_about_search_body, about_search_body;
var init_about_search_body = __esmMin((() => {
	init_runtime();
	en_about_search_body = () => {
		return ` classic link results with cache metadata. `;
	};
	about_search_body = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_about_search_body(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/about_search_label.js
var en_about_search_label, about_search_label;
var init_about_search_label = __esmMin((() => {
	init_runtime();
	en_about_search_label = () => {
		return `search:`;
	};
	about_search_label = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_about_search_label(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/ai_label_model.js
var en_ai_label_model, ai_label_model;
var init_ai_label_model = __esmMin((() => {
	init_runtime();
	en_ai_label_model = () => {
		return `model`;
	};
	ai_label_model = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_ai_label_model(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/ai_label_reasoning.js
var en_ai_label_reasoning, ai_label_reasoning;
var init_ai_label_reasoning = __esmMin((() => {
	init_runtime();
	en_ai_label_reasoning = () => {
		return `reasoning`;
	};
	ai_label_reasoning = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_ai_label_reasoning(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/answer_aria_generating.js
var en_answer_aria_generating, answer_aria_generating;
var init_answer_aria_generating = __esmMin((() => {
	init_runtime();
	en_answer_aria_generating = () => {
		return `generating answer`;
	};
	answer_aria_generating = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_answer_aria_generating(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/answer_from_cache.js
var en_answer_from_cache, answer_from_cache;
var init_answer_from_cache = __esmMin((() => {
	init_runtime();
	en_answer_from_cache = () => {
		return `from cache`;
	};
	answer_from_cache = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_answer_from_cache(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/answer_heading.js
var en_answer_heading, answer_heading;
var init_answer_heading = __esmMin((() => {
	init_runtime();
	en_answer_heading = () => {
		return `answer`;
	};
	answer_heading = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_answer_heading(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/answer_no_sources.js
var en_answer_no_sources, answer_no_sources;
var init_answer_no_sources = __esmMin((() => {
	init_runtime();
	en_answer_no_sources = () => {
		return `no sources found for this query - try fewer words, or`;
	};
	answer_no_sources = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_answer_no_sources(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/answer_none_produced.js
var en_answer_none_produced, answer_none_produced;
var init_answer_none_produced = __esmMin((() => {
	init_runtime();
	en_answer_none_produced = () => {
		return `no answer produced`;
	};
	answer_none_produced = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_answer_none_produced(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/answer_related_heading.js
var en_answer_related_heading, answer_related_heading;
var init_answer_related_heading = __esmMin((() => {
	init_runtime();
	en_answer_related_heading = () => {
		return `related`;
	};
	answer_related_heading = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_answer_related_heading(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/answer_retry.js
var en_answer_retry, answer_retry;
var init_answer_retry = __esmMin((() => {
	init_runtime();
	en_answer_retry = () => {
		return `retry`;
	};
	answer_retry = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_answer_retry(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/answer_sources_heading.js
var en_answer_sources_heading, answer_sources_heading;
var init_answer_sources_heading = __esmMin((() => {
	init_runtime();
	en_answer_sources_heading = () => {
		return `sources`;
	};
	answer_sources_heading = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_answer_sources_heading(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/answer_stop.js
var en_answer_stop, answer_stop;
var init_answer_stop = __esmMin((() => {
	init_runtime();
	en_answer_stop = () => {
		return `stop`;
	};
	answer_stop = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_answer_stop(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/answer_stopped.js
var en_answer_stopped, answer_stopped;
var init_answer_stopped = __esmMin((() => {
	init_runtime();
	en_answer_stopped = () => {
		return `stopped`;
	};
	answer_stopped = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_answer_stopped(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/answer_stream_interrupted.js
var en_answer_stream_interrupted, answer_stream_interrupted;
var init_answer_stream_interrupted = __esmMin((() => {
	init_runtime();
	en_answer_stream_interrupted = (i) => {
		return `stream interrupted - ${i?.e}`;
	};
	answer_stream_interrupted = ((inputs, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_answer_stream_interrupted(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/answer_switch_classic.js
var en_answer_switch_classic, answer_switch_classic;
var init_answer_switch_classic = __esmMin((() => {
	init_runtime();
	en_answer_switch_classic = () => {
		return `switch to classic results`;
	};
	answer_switch_classic = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_answer_switch_classic(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/answer_view_classic.js
var en_answer_view_classic, answer_view_classic;
var init_answer_view_classic = __esmMin((() => {
	init_runtime();
	en_answer_view_classic = () => {
		return `view classic`;
	};
	answer_view_classic = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_answer_view_classic(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/api_network_error.js
var en_api_network_error, api_network_error;
var init_api_network_error = __esmMin((() => {
	init_runtime();
	en_api_network_error = () => {
		return `network error`;
	};
	api_network_error = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_api_network_error(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_aria_sparkline.js
var en_dashboard_aria_sparkline, dashboard_aria_sparkline;
var init_dashboard_aria_sparkline = __esmMin((() => {
	init_runtime();
	en_dashboard_aria_sparkline = () => {
		return `searches per day`;
	};
	dashboard_aria_sparkline = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_aria_sparkline(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_cache_db_size.js
var en_dashboard_cache_db_size, dashboard_cache_db_size;
var init_dashboard_cache_db_size = __esmMin((() => {
	init_runtime();
	en_dashboard_cache_db_size = () => {
		return `db size`;
	};
	dashboard_cache_db_size = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_cache_db_size(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_cache_newest.js
var en_dashboard_cache_newest, dashboard_cache_newest;
var init_dashboard_cache_newest = __esmMin((() => {
	init_runtime();
	en_dashboard_cache_newest = () => {
		return `newest`;
	};
	dashboard_cache_newest = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_cache_newest(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_cache_oldest.js
var en_dashboard_cache_oldest, dashboard_cache_oldest;
var init_dashboard_cache_oldest = __esmMin((() => {
	init_runtime();
	en_dashboard_cache_oldest = () => {
		return `oldest`;
	};
	dashboard_cache_oldest = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_cache_oldest(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_cache_rows.js
var en_dashboard_cache_rows, dashboard_cache_rows;
var init_dashboard_cache_rows = __esmMin((() => {
	init_runtime();
	en_dashboard_cache_rows = () => {
		return `rows`;
	};
	dashboard_cache_rows = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_cache_rows(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_cache_total_hits.js
var en_dashboard_cache_total_hits, dashboard_cache_total_hits;
var init_dashboard_cache_total_hits = __esmMin((() => {
	init_runtime();
	en_dashboard_cache_total_hits = () => {
		return `total hits`;
	};
	dashboard_cache_total_hits = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_cache_total_hits(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_cache_unexpired.js
var en_dashboard_cache_unexpired, dashboard_cache_unexpired;
var init_dashboard_cache_unexpired = __esmMin((() => {
	init_runtime();
	en_dashboard_cache_unexpired = () => {
		return `unexpired`;
	};
	dashboard_cache_unexpired = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_cache_unexpired(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_empty.js
var en_dashboard_empty, dashboard_empty;
var init_dashboard_empty = __esmMin((() => {
	init_runtime();
	en_dashboard_empty = () => {
		return `no data yet`;
	};
	dashboard_empty = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_empty(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_error.js
var en_dashboard_error, dashboard_error;
var init_dashboard_error = __esmMin((() => {
	init_runtime();
	en_dashboard_error = (i) => {
		return `error: ${i?.e}`;
	};
	dashboard_error = ((inputs, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_error(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_hit_rate_detail.js
var en_dashboard_hit_rate_detail, dashboard_hit_rate_detail;
var init_dashboard_hit_rate_detail = __esmMin((() => {
	init_runtime();
	en_dashboard_hit_rate_detail = (i) => {
		return `${i?.hits} of ${i?.total} served from cache`;
	};
	dashboard_hit_rate_detail = ((inputs, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_hit_rate_detail(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_page_title.js
var en_dashboard_page_title, dashboard_page_title;
var init_dashboard_page_title = __esmMin((() => {
	init_runtime();
	en_dashboard_page_title = () => {
		return `dashboard`;
	};
	dashboard_page_title = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_page_title(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_panel_cache.js
var en_dashboard_panel_cache, dashboard_panel_cache;
var init_dashboard_panel_cache = __esmMin((() => {
	init_runtime();
	en_dashboard_panel_cache = () => {
		return `cache`;
	};
	dashboard_panel_cache = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_panel_cache(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_panel_clients.js
var en_dashboard_panel_clients, dashboard_panel_clients;
var init_dashboard_panel_clients = __esmMin((() => {
	init_runtime();
	en_dashboard_panel_clients = () => {
		return `client split`;
	};
	dashboard_panel_clients = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_panel_clients(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_panel_hit_rate.js
var en_dashboard_panel_hit_rate, dashboard_panel_hit_rate;
var init_dashboard_panel_hit_rate = __esmMin((() => {
	init_runtime();
	en_dashboard_panel_hit_rate = () => {
		return `cache hit rate`;
	};
	dashboard_panel_hit_rate = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_panel_hit_rate(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_panel_latency.js
var en_dashboard_panel_latency, dashboard_panel_latency;
var init_dashboard_panel_latency = __esmMin((() => {
	init_runtime();
	en_dashboard_panel_latency = () => {
		return `network latency`;
	};
	dashboard_panel_latency = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_panel_latency(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_panel_per_day.js
var en_dashboard_panel_per_day, dashboard_panel_per_day;
var init_dashboard_panel_per_day = __esmMin((() => {
	init_runtime();
	en_dashboard_panel_per_day = () => {
		return `searches per day`;
	};
	dashboard_panel_per_day = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_panel_per_day(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_panel_top_queries.js
var en_dashboard_panel_top_queries, dashboard_panel_top_queries;
var init_dashboard_panel_top_queries = __esmMin((() => {
	init_runtime();
	en_dashboard_panel_top_queries = () => {
		return `top queries`;
	};
	dashboard_panel_top_queries = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_panel_top_queries(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_panel_zero_result.js
var en_dashboard_panel_zero_result, dashboard_panel_zero_result;
var init_dashboard_panel_zero_result = __esmMin((() => {
	init_runtime();
	en_dashboard_panel_zero_result = () => {
		return `zero-result queries`;
	};
	dashboard_panel_zero_result = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_panel_zero_result(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_per_day_total.js
var en_dashboard_per_day_total, dashboard_per_day_total;
var init_dashboard_per_day_total = __esmMin((() => {
	init_runtime();
	en_dashboard_per_day_total = (i) => {
		return `${i?.n} searches in window`;
	};
	dashboard_per_day_total = ((inputs, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_per_day_total(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_title.js
var en_dashboard_title, dashboard_title;
var init_dashboard_title = __esmMin((() => {
	init_runtime();
	en_dashboard_title = () => {
		return `oxe stats`;
	};
	dashboard_title = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_title(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_window.js
var en_dashboard_window, dashboard_window;
var init_dashboard_window = __esmMin((() => {
	init_runtime();
	en_dashboard_window = (i) => {
		return `window: last ${i?.n} days`;
	};
	dashboard_window = ((inputs, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_window(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/dashboard_zero_result_none.js
var en_dashboard_zero_result_none, dashboard_zero_result_none;
var init_dashboard_zero_result_none = __esmMin((() => {
	init_runtime();
	en_dashboard_zero_result_none = () => {
		return `none 🎉`;
	};
	dashboard_zero_result_none = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_dashboard_zero_result_none(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/error_back.js
var en_error_back, error_back;
var init_error_back = __esmMin((() => {
	init_runtime();
	en_error_back = () => {
		return `back to search`;
	};
	error_back = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_error_back(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/error_title.js
var en_error_title, error_title;
var init_error_title = __esmMin((() => {
	init_runtime();
	en_error_title = () => {
		return `something broke`;
	};
	error_title = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_error_title(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/error_try_again.js
var en_error_try_again, error_try_again;
var init_error_try_again = __esmMin((() => {
	init_runtime();
	en_error_try_again = () => {
		return `try again`;
	};
	error_try_again = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_error_try_again(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/header_aria_github.js
var en_header_aria_github, header_aria_github;
var init_header_aria_github = __esmMin((() => {
	init_runtime();
	en_header_aria_github = () => {
		return `GitHub repository`;
	};
	header_aria_github = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_header_aria_github(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/header_aria_settings.js
var en_header_aria_settings, header_aria_settings;
var init_header_aria_settings = __esmMin((() => {
	init_runtime();
	en_header_aria_settings = () => {
		return `Settings`;
	};
	header_aria_settings = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_header_aria_settings(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/header_settings.js
var en_header_settings, header_settings;
var init_header_settings = __esmMin((() => {
	init_runtime();
	en_header_settings = () => {
		return `Settings`;
	};
	header_settings = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_header_settings(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_aria_delete_scope.js
var en_history_aria_delete_scope, history_aria_delete_scope;
var init_history_aria_delete_scope = __esmMin((() => {
	init_runtime();
	en_history_aria_delete_scope = () => {
		return `delete scope`;
	};
	history_aria_delete_scope = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_aria_delete_scope(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_aria_query_filter.js
var en_history_aria_query_filter, history_aria_query_filter;
var init_history_aria_query_filter = __esmMin((() => {
	init_runtime();
	en_history_aria_query_filter = () => {
		return `query filter`;
	};
	history_aria_query_filter = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_aria_query_filter(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_aria_time_filter.js
var en_history_aria_time_filter, history_aria_time_filter;
var init_history_aria_time_filter = __esmMin((() => {
	init_runtime();
	en_history_aria_time_filter = () => {
		return `time filter`;
	};
	history_aria_time_filter = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_aria_time_filter(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_clear.js
var en_history_clear, history_clear;
var init_history_clear = __esmMin((() => {
	init_runtime();
	en_history_clear = () => {
		return `clear`;
	};
	history_clear = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_clear(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_col_detail.js
var en_history_col_detail, history_col_detail;
var init_history_col_detail = __esmMin((() => {
	init_runtime();
	en_history_col_detail = () => {
		return `detail`;
	};
	history_col_detail = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_col_detail(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_col_kind.js
var en_history_col_kind, history_col_kind;
var init_history_col_kind = __esmMin((() => {
	init_runtime();
	en_history_col_kind = () => {
		return `kind`;
	};
	history_col_kind = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_col_kind(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_col_query.js
var en_history_col_query, history_col_query;
var init_history_col_query = __esmMin((() => {
	init_runtime();
	en_history_col_query = () => {
		return `query`;
	};
	history_col_query = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_col_query(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_col_when.js
var en_history_col_when, history_col_when;
var init_history_col_when = __esmMin((() => {
	init_runtime();
	en_history_col_when = () => {
		return `when`;
	};
	history_col_when = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_col_when(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_copy_json.js
var en_history_copy_json, history_copy_json;
var init_history_copy_json = __esmMin((() => {
	init_runtime();
	en_history_copy_json = () => {
		return `copy json`;
	};
	history_copy_json = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_copy_json(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_copy_json_failed.js
var en_history_copy_json_failed, history_copy_json_failed;
var init_history_copy_json_failed = __esmMin((() => {
	init_runtime();
	en_history_copy_json_failed = (i) => {
		return `copy failed: ${i?.e}`;
	};
	history_copy_json_failed = ((inputs, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_copy_json_failed(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_delete_all.js
var en_history_delete_all, history_delete_all;
var init_history_delete_all = __esmMin((() => {
	init_runtime();
	en_history_delete_all = () => {
		return `all history`;
	};
	history_delete_all = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_delete_all(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_delete_arm.js
var en_history_delete_arm, history_delete_arm;
var init_history_delete_arm = __esmMin((() => {
	init_runtime();
	en_history_delete_arm = () => {
		return `delete…`;
	};
	history_delete_arm = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_delete_arm(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_delete_confirm.js
var en_history_delete_confirm, history_delete_confirm;
var init_history_delete_confirm = __esmMin((() => {
	init_runtime();
	en_history_delete_confirm = () => {
		return `confirm delete`;
	};
	history_delete_confirm = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_delete_confirm(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_delete_confirm_all.js
var en_history_delete_confirm_all, history_delete_confirm_all;
var init_history_delete_confirm_all = __esmMin((() => {
	init_runtime();
	en_history_delete_confirm_all = () => {
		return `really delete all?`;
	};
	history_delete_confirm_all = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_delete_confirm_all(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_delete_older_24h.js
var en_history_delete_older_24h, history_delete_older_24h;
var init_history_delete_older_24h = __esmMin((() => {
	init_runtime();
	en_history_delete_older_24h = () => {
		return `older than 24h`;
	};
	history_delete_older_24h = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_delete_older_24h(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_delete_older_30d.js
var en_history_delete_older_30d, history_delete_older_30d;
var init_history_delete_older_30d = __esmMin((() => {
	init_runtime();
	en_history_delete_older_30d = () => {
		return `older than 30d`;
	};
	history_delete_older_30d = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_delete_older_30d(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_delete_older_7d.js
var en_history_delete_older_7d, history_delete_older_7d;
var init_history_delete_older_7d = __esmMin((() => {
	init_runtime();
	en_history_delete_older_7d = () => {
		return `older than 7d`;
	};
	history_delete_older_7d = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_delete_older_7d(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_empty_link.js
var en_history_empty_link, history_empty_link;
var init_history_empty_link = __esmMin((() => {
	init_runtime();
	en_history_empty_link = () => {
		return `search`;
	};
	history_empty_link = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_empty_link(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_empty_prefix.js
var en_history_empty_prefix, history_empty_prefix;
var init_history_empty_prefix = __esmMin((() => {
	init_runtime();
	en_history_empty_prefix = () => {
		return `nothing here yet — open a result from the `;
	};
	history_empty_prefix = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_empty_prefix(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_empty_suffix.js
var en_history_empty_suffix, history_empty_suffix;
var init_history_empty_suffix = __esmMin((() => {
	init_runtime();
	en_history_empty_suffix = () => {
		return ` page.`;
	};
	history_empty_suffix = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_empty_suffix(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_error.js
var en_history_error, history_error;
var init_history_error = __esmMin((() => {
	init_runtime();
	en_history_error = (i) => {
		return `error: ${i?.e}`;
	};
	history_error = ((inputs, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_error(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_filter_placeholder.js
var en_history_filter_placeholder, history_filter_placeholder;
var init_history_filter_placeholder = __esmMin((() => {
	init_runtime();
	en_history_filter_placeholder = () => {
		return `filter by query text…`;
	};
	history_filter_placeholder = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_filter_placeholder(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_hits.js
var en_history_hits, history_hits;
var init_history_hits = __esmMin((() => {
	init_runtime();
	en_history_hits = (i) => {
		return `${i?.hits} hits · expires ${i?.at}`;
	};
	history_hits = ((inputs, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_hits(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_kind_click.js
var en_history_kind_click, history_kind_click;
var init_history_kind_click = __esmMin((() => {
	init_runtime();
	en_history_kind_click = (i) => {
		return `click · ${i?.source}`;
	};
	history_kind_click = ((inputs, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_kind_click(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_kind_search.js
var en_history_kind_search, history_kind_search;
var init_history_kind_search = __esmMin((() => {
	init_runtime();
	en_history_kind_search = () => {
		return `search`;
	};
	history_kind_search = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_kind_search(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_no_query.js
var en_history_no_query, history_no_query;
var init_history_no_query = __esmMin((() => {
	init_runtime();
	en_history_no_query = () => {
		return `(no query)`;
	};
	history_no_query = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_no_query(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_opt_all_time.js
var en_history_opt_all_time, history_opt_all_time;
var init_history_opt_all_time = __esmMin((() => {
	init_runtime();
	en_history_opt_all_time = () => {
		return `all time`;
	};
	history_opt_all_time = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_opt_all_time(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_opt_last_24h.js
var en_history_opt_last_24h, history_opt_last_24h;
var init_history_opt_last_24h = __esmMin((() => {
	init_runtime();
	en_history_opt_last_24h = () => {
		return `last 24h`;
	};
	history_opt_last_24h = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_opt_last_24h(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_opt_last_month.js
var en_history_opt_last_month, history_opt_last_month;
var init_history_opt_last_month = __esmMin((() => {
	init_runtime();
	en_history_opt_last_month = () => {
		return `last month`;
	};
	history_opt_last_month = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_opt_last_month(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_opt_last_week.js
var en_history_opt_last_week, history_opt_last_week;
var init_history_opt_last_week = __esmMin((() => {
	init_runtime();
	en_history_opt_last_week = () => {
		return `last week`;
	};
	history_opt_last_week = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_opt_last_week(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_page_title.js
var en_history_page_title, history_page_title;
var init_history_page_title = __esmMin((() => {
	init_runtime();
	en_history_page_title = () => {
		return `history`;
	};
	history_page_title = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_page_title(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_summary.js
var en_history_summary, history_summary;
var init_history_summary = __esmMin((() => {
	init_runtime();
	en_history_summary = (i) => {
		return `${i?.clicks} clicks · ${i?.rows} cached searches · newest first`;
	};
	history_summary = ((inputs, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_summary(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/history_title.js
var en_history_title, history_title;
var init_history_title = __esmMin((() => {
	init_runtime();
	en_history_title = () => {
		return `History`;
	};
	history_title = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_history_title(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/home_tagline.js
var en_home_tagline, home_tagline;
var init_home_tagline = __esmMin((() => {
	init_runtime();
	en_home_tagline = () => {
		return `your local web intel layer`;
	};
	home_tagline = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_home_tagline(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/mode_aria_label.js
var init_mode_aria_label = __esmMin((() => {
	init_runtime();
}));
//#endregion
//#region src/paraglide/messages/mode_aria_label_lower.js
var en_mode_aria_label_lower, mode_aria_label_lower;
var init_mode_aria_label_lower = __esmMin((() => {
	init_runtime();
	en_mode_aria_label_lower = () => {
		return `search mode`;
	};
	mode_aria_label_lower = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_mode_aria_label_lower(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/mode_label_ai.js
var en_mode_label_ai, mode_label_ai;
var init_mode_label_ai = __esmMin((() => {
	init_runtime();
	en_mode_label_ai = () => {
		return `AI`;
	};
	mode_label_ai = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_mode_label_ai(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/mode_label_traditional.js
var init_mode_label_traditional = __esmMin((() => {
	init_runtime();
}));
//#endregion
//#region src/paraglide/messages/mode_tip_ai_disabled.js
var init_mode_tip_ai_disabled = __esmMin((() => {
	init_runtime();
}));
//#endregion
//#region src/paraglide/messages/mode_tip_ai_disabled_short.js
var en_mode_tip_ai_disabled_short, mode_tip_ai_disabled_short;
var init_mode_tip_ai_disabled_short = __esmMin((() => {
	init_runtime();
	en_mode_tip_ai_disabled_short = () => {
		return `configure a model in settings`;
	};
	mode_tip_ai_disabled_short = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_mode_tip_ai_disabled_short(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/model_aria_label.js
var en_model_aria_label, model_aria_label;
var init_model_aria_label = __esmMin((() => {
	init_runtime();
	en_model_aria_label = () => {
		return `AI model`;
	};
	model_aria_label = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_model_aria_label(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/model_listing_failed.js
var en_model_listing_failed, model_listing_failed;
var init_model_listing_failed = __esmMin((() => {
	init_runtime();
	en_model_listing_failed = (i) => {
		return `Model listing failed: ${i?.e}`;
	};
	model_listing_failed = ((inputs, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_model_listing_failed(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/model_none_hint.js
var en_model_none_hint, model_none_hint;
var init_model_none_hint = __esmMin((() => {
	init_runtime();
	en_model_none_hint = () => {
		return `No models - check provider / API key in settings`;
	};
	model_none_hint = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_model_none_hint(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/model_ph_filter.js
var en_model_ph_filter, model_ph_filter;
var init_model_ph_filter = __esmMin((() => {
	init_runtime();
	en_model_ph_filter = () => {
		return `filter models…`;
	};
	model_ph_filter = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_model_ph_filter(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/nav_dashboard.js
var en_nav_dashboard, nav_dashboard;
var init_nav_dashboard = __esmMin((() => {
	init_runtime();
	en_nav_dashboard = () => {
		return `Dashboard`;
	};
	nav_dashboard = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_nav_dashboard(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/nav_history.js
var en_nav_history, nav_history;
var init_nav_history = __esmMin((() => {
	init_runtime();
	en_nav_history = () => {
		return `History`;
	};
	nav_history = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_nav_history(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/nav_search.js
var en_nav_search, nav_search;
var init_nav_search = __esmMin((() => {
	init_runtime();
	en_nav_search = () => {
		return `Search`;
	};
	nav_search = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_nav_search(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/notfound_back.js
var en_notfound_back, notfound_back;
var init_notfound_back = __esmMin((() => {
	init_runtime();
	en_notfound_back = () => {
		return `back to search`;
	};
	notfound_back = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_notfound_back(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/notfound_body.js
var en_notfound_body, notfound_body;
var init_notfound_body = __esmMin((() => {
	init_runtime();
	en_notfound_body = () => {
		return `this page does not exist — check the address or head back to search`;
	};
	notfound_body = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_notfound_body(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/notfound_history.js
var en_notfound_history, notfound_history;
var init_notfound_history = __esmMin((() => {
	init_runtime();
	en_notfound_history = () => {
		return `history`;
	};
	notfound_history = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_notfound_history(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/notfound_title.js
var en_notfound_title, notfound_title;
var init_notfound_title = __esmMin((() => {
	init_runtime();
	en_notfound_title = () => {
		return `nothing here`;
	};
	notfound_title = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_notfound_title(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/result_cached_text_preview.js
var en_result_cached_text_preview, result_cached_text_preview;
var init_result_cached_text_preview = __esmMin((() => {
	init_runtime();
	en_result_cached_text_preview = () => {
		return `cached page text preview`;
	};
	result_cached_text_preview = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_result_cached_text_preview(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/result_untitled.js
var en_result_untitled, result_untitled;
var init_result_untitled = __esmMin((() => {
	init_runtime();
	en_result_untitled = () => {
		return `(untitled)`;
	};
	result_untitled = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_result_untitled(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_ai_blocked.js
var en_search_ai_blocked, search_ai_blocked;
var init_search_ai_blocked = __esmMin((() => {
	init_runtime();
	en_search_ai_blocked = () => {
		return `AI mode is not configured - set a model in settings`;
	};
	search_ai_blocked = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_ai_blocked(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_aria_cached_refresh.js
var en_search_aria_cached_refresh, search_aria_cached_refresh;
var init_search_aria_cached_refresh = __esmMin((() => {
	init_runtime();
	en_search_aria_cached_refresh = () => {
		return `cached result: click to refresh from the web`;
	};
	search_aria_cached_refresh = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_aria_cached_refresh(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_aria_loading.js
var en_search_aria_loading, search_aria_loading;
var init_search_aria_loading = __esmMin((() => {
	init_runtime();
	en_search_aria_loading = () => {
		return `loading`;
	};
	search_aria_loading = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_aria_loading(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_ask_ai_instead.js
var en_search_ask_ai_instead, search_ask_ai_instead;
var init_search_ask_ai_instead = __esmMin((() => {
	init_runtime();
	en_search_ask_ai_instead = () => {
		return `ask AI instead`;
	};
	search_ask_ai_instead = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_ask_ai_instead(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_cached.js
var init_search_cached = __esmMin((() => {
	init_runtime();
}));
//#endregion
//#region src/paraglide/messages/search_cached_age.js
var en_search_cached_age, search_cached_age;
var init_search_cached_age = __esmMin((() => {
	init_runtime();
	en_search_cached_age = (i) => {
		return ` · ${i?.age} old`;
	};
	search_cached_age = ((inputs, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_cached_age(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_copy_json.js
var en_search_copy_json, search_copy_json;
var init_search_copy_json = __esmMin((() => {
	init_runtime();
	en_search_copy_json = () => {
		return `copy json`;
	};
	search_copy_json = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_copy_json(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_copy_link.js
var en_search_copy_link, search_copy_link;
var init_search_copy_link = __esmMin((() => {
	init_runtime();
	en_search_copy_link = () => {
		return `copy link`;
	};
	search_copy_link = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_copy_link(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_end_of_results.js
var en_search_end_of_results, search_end_of_results;
var init_search_end_of_results = __esmMin((() => {
	init_runtime();
	en_search_end_of_results = () => {
		return `end of results`;
	};
	search_end_of_results = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_end_of_results(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_error_backend.js
var en_search_error_backend, search_error_backend;
var init_search_error_backend = __esmMin((() => {
	init_runtime();
	en_search_error_backend = () => {
		return `search backend failed`;
	};
	search_error_backend = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_error_backend(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_error_rate_limited.js
var en_search_error_rate_limited, search_error_rate_limited;
var init_search_error_rate_limited = __esmMin((() => {
	init_runtime();
	en_search_error_rate_limited = () => {
		return `search backend rate-limited, retry shortly`;
	};
	search_error_rate_limited = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_error_rate_limited(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_meta_results.js
var en_search_meta_results, search_meta_results;
var init_search_meta_results = __esmMin((() => {
	init_runtime();
	en_search_meta_results = (i) => {
		if (i?.n === 1 || i?.n === "1") return `${i?.n} result`;
		return `${i?.n} results`;
	};
	search_meta_results = ((inputs, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_meta_results(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_more_error.js
var en_search_more_error, search_more_error;
var init_search_more_error = __esmMin((() => {
	init_runtime();
	en_search_more_error = () => {
		return `couldn’t load more results`;
	};
	search_more_error = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_more_error(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_more_results.js
var en_search_more_results, search_more_results;
var init_search_more_results = __esmMin((() => {
	init_runtime();
	en_search_more_results = () => {
		return `more results`;
	};
	search_more_results = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_more_results(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_no_results.js
var en_search_no_results, search_no_results;
var init_search_no_results = __esmMin((() => {
	init_runtime();
	en_search_no_results = () => {
		return `no results`;
	};
	search_no_results = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_no_results(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_page_title.js
var en_search_page_title, search_page_title;
var init_search_page_title = __esmMin((() => {
	init_runtime();
	en_search_page_title = () => {
		return `search`;
	};
	search_page_title = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_page_title(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_retry.js
var en_search_retry, search_retry;
var init_search_retry = __esmMin((() => {
	init_runtime();
	en_search_retry = () => {
		return `retry`;
	};
	search_retry = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_retry(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_searching.js
var en_search_searching, search_searching;
var init_search_searching = __esmMin((() => {
	init_runtime();
	en_search_searching = () => {
		return `searching…`;
	};
	search_searching = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_searching(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_tip_refresh.js
var en_search_tip_refresh, search_tip_refresh;
var init_search_tip_refresh = __esmMin((() => {
	init_runtime();
	en_search_tip_refresh = () => {
		return `Actually search the web (refreshes this cache entry)`;
	};
	search_tip_refresh = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_tip_refresh(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/search_toast_refreshed.js
var en_search_toast_refreshed, search_toast_refreshed;
var init_search_toast_refreshed = __esmMin((() => {
	init_runtime();
	en_search_toast_refreshed = () => {
		return `refreshed from the web`;
	};
	search_toast_refreshed = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_search_toast_refreshed(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/searchbox_aria_search.js
var en_searchbox_aria_search, searchbox_aria_search;
var init_searchbox_aria_search = __esmMin((() => {
	init_runtime();
	en_searchbox_aria_search = () => {
		return `search`;
	};
	searchbox_aria_search = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_searchbox_aria_search(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/searchbox_aria_submit.js
var en_searchbox_aria_submit, searchbox_aria_submit;
var init_searchbox_aria_submit = __esmMin((() => {
	init_runtime();
	en_searchbox_aria_submit = () => {
		return `submit search`;
	};
	searchbox_aria_submit = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_searchbox_aria_submit(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/searchbox_ph_ai.js
var en_searchbox_ph_ai, searchbox_ph_ai;
var init_searchbox_ph_ai = __esmMin((() => {
	init_runtime();
	en_searchbox_ph_ai = () => {
		return `Ask anything privately`;
	};
	searchbox_ph_ai = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_searchbox_ph_ai(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/searchbox_ph_traditional.js
var en_searchbox_ph_traditional, searchbox_ph_traditional;
var init_searchbox_ph_traditional = __esmMin((() => {
	init_runtime();
	en_searchbox_ph_traditional = () => {
		return `Search privately`;
	};
	searchbox_ph_traditional = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_searchbox_ph_traditional(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/segments_search.js
var en_segments_search, segments_search;
var init_segments_search = __esmMin((() => {
	init_runtime();
	en_segments_search = () => {
		return `Search`;
	};
	segments_search = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_segments_search(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_aria.js
var en_settings_aria, settings_aria;
var init_settings_aria = __esmMin((() => {
	init_runtime();
	en_settings_aria = () => {
		return `Settings`;
	};
	settings_aria = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_aria(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_aria_close.js
var en_settings_aria_close, settings_aria_close;
var init_settings_aria_close = __esmMin((() => {
	init_runtime();
	en_settings_aria_close = () => {
		return `Close settings`;
	};
	settings_aria_close = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_aria_close(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_aria_theme.js
var en_settings_aria_theme, settings_aria_theme;
var init_settings_aria_theme = __esmMin((() => {
	init_runtime();
	en_settings_aria_theme = () => {
		return `Theme`;
	};
	settings_aria_theme = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_aria_theme(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_backdrop_close.js
var en_settings_backdrop_close, settings_backdrop_close;
var init_settings_backdrop_close = __esmMin((() => {
	init_runtime();
	en_settings_backdrop_close = () => {
		return `close`;
	};
	settings_backdrop_close = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_backdrop_close(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_cancel.js
var en_settings_cancel, settings_cancel;
var init_settings_cancel = __esmMin((() => {
	init_runtime();
	en_settings_cancel = () => {
		return `Cancel`;
	};
	settings_cancel = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_cancel(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_conn_failed.js
var en_settings_conn_failed, settings_conn_failed;
var init_settings_conn_failed = __esmMin((() => {
	init_runtime();
	en_settings_conn_failed = () => {
		return `Connection failed`;
	};
	settings_conn_failed = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_conn_failed(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_conn_ok.js
var en_settings_conn_ok, settings_conn_ok;
var init_settings_conn_ok = __esmMin((() => {
	init_runtime();
	en_settings_conn_ok = () => {
		return `Connection ok`;
	};
	settings_conn_ok = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_conn_ok(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_fieldset_ai.js
var en_settings_fieldset_ai, settings_fieldset_ai;
var init_settings_fieldset_ai = __esmMin((() => {
	init_runtime();
	en_settings_fieldset_ai = () => {
		return `AI`;
	};
	settings_fieldset_ai = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_fieldset_ai(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_fieldset_theme.js
var en_settings_fieldset_theme, settings_fieldset_theme;
var init_settings_fieldset_theme = __esmMin((() => {
	init_runtime();
	en_settings_fieldset_theme = () => {
		return `Theme`;
	};
	settings_fieldset_theme = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_fieldset_theme(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_label_api_key.js
var en_settings_label_api_key, settings_label_api_key;
var init_settings_label_api_key = __esmMin((() => {
	init_runtime();
	en_settings_label_api_key = () => {
		return `API key`;
	};
	settings_label_api_key = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_label_api_key(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_label_base_url.js
var en_settings_label_base_url, settings_label_base_url;
var init_settings_label_base_url = __esmMin((() => {
	init_runtime();
	en_settings_label_base_url = () => {
		return `Base URL`;
	};
	settings_label_base_url = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_label_base_url(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_label_enabled.js
var en_settings_label_enabled, settings_label_enabled;
var init_settings_label_enabled = __esmMin((() => {
	init_runtime();
	en_settings_label_enabled = () => {
		return `Enabled`;
	};
	settings_label_enabled = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_label_enabled(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_label_model.js
var en_settings_label_model, settings_label_model;
var init_settings_label_model = __esmMin((() => {
	init_runtime();
	en_settings_label_model = () => {
		return `Model`;
	};
	settings_label_model = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_label_model(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_label_provider.js
var en_settings_label_provider, settings_label_provider;
var init_settings_label_provider = __esmMin((() => {
	init_runtime();
	en_settings_label_provider = () => {
		return `Provider`;
	};
	settings_label_provider = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_label_provider(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_load_failed.js
var en_settings_load_failed, settings_load_failed;
var init_settings_load_failed = __esmMin((() => {
	init_runtime();
	en_settings_load_failed = (i) => {
		return `Backend settings endpoints not available (${i?.e}) - the server needs GET/PUT /settings support`;
	};
	settings_load_failed = ((inputs, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_load_failed(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_load_failed_msg.js
var en_settings_load_failed_msg, settings_load_failed_msg;
var init_settings_load_failed_msg = __esmMin((() => {
	init_runtime();
	en_settings_load_failed_msg = () => {
		return `settings load failed`;
	};
	settings_load_failed_msg = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_load_failed_msg(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_ph_api_key.js
var en_settings_ph_api_key, settings_ph_api_key;
var init_settings_ph_api_key = __esmMin((() => {
	init_runtime();
	en_settings_ph_api_key = () => {
		return `(unchanged if blank)`;
	};
	settings_ph_api_key = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_ph_api_key(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_pick_model_first.js
var en_settings_pick_model_first, settings_pick_model_first;
var init_settings_pick_model_first = __esmMin((() => {
	init_runtime();
	en_settings_pick_model_first = () => {
		return `Pick a model first`;
	};
	settings_pick_model_first = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_pick_model_first(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_save.js
var en_settings_save, settings_save;
var init_settings_save = __esmMin((() => {
	init_runtime();
	en_settings_save = () => {
		return `Save`;
	};
	settings_save = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_save(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_save_failed_msg.js
var en_settings_save_failed_msg, settings_save_failed_msg;
var init_settings_save_failed_msg = __esmMin((() => {
	init_runtime();
	en_settings_save_failed_msg = () => {
		return `settings save failed`;
	};
	settings_save_failed_msg = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_save_failed_msg(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_test_connection.js
var en_settings_test_connection, settings_test_connection;
var init_settings_test_connection = __esmMin((() => {
	init_runtime();
	en_settings_test_connection = () => {
		return `Test connection`;
	};
	settings_test_connection = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_test_connection(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_test_failed.js
var en_settings_test_failed, settings_test_failed;
var init_settings_test_failed = __esmMin((() => {
	init_runtime();
	en_settings_test_failed = () => {
		return `test failed`;
	};
	settings_test_failed = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_test_failed(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_theme_dark.js
var en_settings_theme_dark, settings_theme_dark;
var init_settings_theme_dark = __esmMin((() => {
	init_runtime();
	en_settings_theme_dark = () => {
		return `Dark`;
	};
	settings_theme_dark = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_theme_dark(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_theme_follows_os.js
var en_settings_theme_follows_os, settings_theme_follows_os;
var init_settings_theme_follows_os = __esmMin((() => {
	init_runtime();
	en_settings_theme_follows_os = () => {
		return `Follows your OS light/dark preference`;
	};
	settings_theme_follows_os = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_theme_follows_os(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_theme_light.js
var en_settings_theme_light, settings_theme_light;
var init_settings_theme_light = __esmMin((() => {
	init_runtime();
	en_settings_theme_light = () => {
		return `Light`;
	};
	settings_theme_light = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_theme_light(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_theme_system.js
var en_settings_theme_system, settings_theme_system;
var init_settings_theme_system = __esmMin((() => {
	init_runtime();
	en_settings_theme_system = () => {
		return `System`;
	};
	settings_theme_system = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_theme_system(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_title.js
var en_settings_title, settings_title;
var init_settings_title = __esmMin((() => {
	init_runtime();
	en_settings_title = () => {
		return `Settings`;
	};
	settings_title = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_title(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_toast_save_failed.js
var en_settings_toast_save_failed, settings_toast_save_failed;
var init_settings_toast_save_failed = __esmMin((() => {
	init_runtime();
	en_settings_toast_save_failed = (i) => {
		return `Settings save failed: ${i?.msg}`;
	};
	settings_toast_save_failed = ((inputs, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_toast_save_failed(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/settings_toast_saved.js
var en_settings_toast_saved, settings_toast_saved;
var init_settings_toast_saved = __esmMin((() => {
	init_runtime();
	en_settings_toast_saved = () => {
		return `Settings saved`;
	};
	settings_toast_saved = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_settings_toast_saved(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/suggest_aria_toggle_web.js
var en_suggest_aria_toggle_web, suggest_aria_toggle_web;
var init_suggest_aria_toggle_web = __esmMin((() => {
	init_runtime();
	en_suggest_aria_toggle_web = () => {
		return `toggle web suggestions`;
	};
	suggest_aria_toggle_web = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_suggest_aria_toggle_web(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/suggest_group_history.js
var en_suggest_group_history, suggest_group_history;
var init_suggest_group_history = __esmMin((() => {
	init_runtime();
	en_suggest_group_history = () => {
		return `your history`;
	};
	suggest_group_history = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_suggest_group_history(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/suggest_web_suggestions.js
var en_suggest_web_suggestions, suggest_web_suggestions;
var init_suggest_web_suggestions = __esmMin((() => {
	init_runtime();
	en_suggest_web_suggestions = () => {
		return `web suggestions`;
	};
	suggest_web_suggestions = ((inputs = {}, options = {}) => {
		experimentalStaticLocale ?? options.locale ?? getLocale();
		return en_suggest_web_suggestions(inputs);
	});
}));
//#endregion
//#region src/paraglide/messages/_index.js
var init__index = __esmMin((() => {
	init_about_ai_body();
	init_about_ai_label();
	init_about_aria();
	init_about_line1();
	init_about_search_body();
	init_about_search_label();
	init_ai_label_model();
	init_ai_label_reasoning();
	init_answer_aria_generating();
	init_answer_from_cache();
	init_answer_heading();
	init_answer_no_sources();
	init_answer_none_produced();
	init_answer_related_heading();
	init_answer_retry();
	init_answer_sources_heading();
	init_answer_stop();
	init_answer_stopped();
	init_answer_stream_interrupted();
	init_answer_switch_classic();
	init_answer_view_classic();
	init_api_network_error();
	init_dashboard_aria_sparkline();
	init_dashboard_cache_db_size();
	init_dashboard_cache_newest();
	init_dashboard_cache_oldest();
	init_dashboard_cache_rows();
	init_dashboard_cache_total_hits();
	init_dashboard_cache_unexpired();
	init_dashboard_empty();
	init_dashboard_error();
	init_dashboard_hit_rate_detail();
	init_dashboard_page_title();
	init_dashboard_panel_cache();
	init_dashboard_panel_clients();
	init_dashboard_panel_hit_rate();
	init_dashboard_panel_latency();
	init_dashboard_panel_per_day();
	init_dashboard_panel_top_queries();
	init_dashboard_panel_zero_result();
	init_dashboard_per_day_total();
	init_dashboard_title();
	init_dashboard_window();
	init_dashboard_zero_result_none();
	init_error_back();
	init_error_title();
	init_error_try_again();
	init_header_aria_github();
	init_header_aria_settings();
	init_header_settings();
	init_history_aria_delete_scope();
	init_history_aria_query_filter();
	init_history_aria_time_filter();
	init_history_clear();
	init_history_col_detail();
	init_history_col_kind();
	init_history_col_query();
	init_history_col_when();
	init_history_copy_json();
	init_history_copy_json_failed();
	init_history_delete_all();
	init_history_delete_arm();
	init_history_delete_confirm();
	init_history_delete_confirm_all();
	init_history_delete_older_24h();
	init_history_delete_older_30d();
	init_history_delete_older_7d();
	init_history_empty_link();
	init_history_empty_prefix();
	init_history_empty_suffix();
	init_history_error();
	init_history_filter_placeholder();
	init_history_hits();
	init_history_kind_click();
	init_history_kind_search();
	init_history_no_query();
	init_history_opt_all_time();
	init_history_opt_last_24h();
	init_history_opt_last_month();
	init_history_opt_last_week();
	init_history_page_title();
	init_history_summary();
	init_history_title();
	init_home_tagline();
	init_mode_aria_label();
	init_mode_aria_label_lower();
	init_mode_label_ai();
	init_mode_label_traditional();
	init_mode_tip_ai_disabled();
	init_mode_tip_ai_disabled_short();
	init_model_aria_label();
	init_model_listing_failed();
	init_model_none_hint();
	init_model_ph_filter();
	init_nav_dashboard();
	init_nav_history();
	init_nav_search();
	init_notfound_back();
	init_notfound_body();
	init_notfound_history();
	init_notfound_title();
	init_result_cached_text_preview();
	init_result_untitled();
	init_search_ai_blocked();
	init_search_aria_cached_refresh();
	init_search_aria_loading();
	init_search_ask_ai_instead();
	init_search_cached();
	init_search_cached_age();
	init_search_copy_json();
	init_search_copy_link();
	init_search_end_of_results();
	init_search_error_backend();
	init_search_error_rate_limited();
	init_search_meta_results();
	init_search_more_error();
	init_search_more_results();
	init_search_no_results();
	init_search_page_title();
	init_search_retry();
	init_search_searching();
	init_search_tip_refresh();
	init_search_toast_refreshed();
	init_searchbox_aria_search();
	init_searchbox_aria_submit();
	init_searchbox_ph_ai();
	init_searchbox_ph_traditional();
	init_segments_search();
	init_settings_aria();
	init_settings_aria_close();
	init_settings_aria_theme();
	init_settings_backdrop_close();
	init_settings_cancel();
	init_settings_conn_failed();
	init_settings_conn_ok();
	init_settings_fieldset_ai();
	init_settings_fieldset_theme();
	init_settings_label_api_key();
	init_settings_label_base_url();
	init_settings_label_enabled();
	init_settings_label_model();
	init_settings_label_provider();
	init_settings_load_failed();
	init_settings_load_failed_msg();
	init_settings_ph_api_key();
	init_settings_pick_model_first();
	init_settings_save();
	init_settings_save_failed_msg();
	init_settings_test_connection();
	init_settings_test_failed();
	init_settings_theme_dark();
	init_settings_theme_follows_os();
	init_settings_theme_light();
	init_settings_theme_system();
	init_settings_title();
	init_settings_toast_save_failed();
	init_settings_toast_saved();
	init_suggest_aria_toggle_web();
	init_suggest_group_history();
	init_suggest_web_suggestions();
}));
//#endregion
//#region src/paraglide/messages.js
var init_messages = __esmMin((() => {
	init__index();
	init__index();
}));
//#endregion
//#region src/lib/i18n.ts
var init_i18n = __esmMin((() => {
	init_messages();
}));
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
			error: v.nullish(str),
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
		throw new ApiError("network_error", e?.message ?? api_network_error(), 0);
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
	init_i18n();
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
//#region \0@oxc-project+runtime@0.149.0/helpers/esm/usingCtx.js
function _usingCtx() {
	var r = "function" == typeof SuppressedError ? SuppressedError : function(r, e) {
		var n = Error();
		return n.name = "SuppressedError", n.error = r, n.suppressed = e, n;
	}, e = {}, n = [];
	function using(r, e) {
		if (null != e) {
			if (Object(e) !== e) throw new TypeError("using declarations can only be used with objects, functions, null, or undefined.");
			if (r) var o = e[Symbol.asyncDispose || Symbol["for"]("Symbol.asyncDispose")];
			if (void 0 === o && (o = e[Symbol.dispose || Symbol["for"]("Symbol.dispose")], r)) var t = o;
			if ("function" != typeof o) throw new TypeError("Object is not disposable.");
			t && (o = function o() {
				try {
					t.call(e);
				} catch (r) {
					return Promise.reject(r);
				}
			}), n.push({
				v: e,
				d: o,
				a: r
			});
		} else r && n.push({
			d: e,
			a: r
		});
		return e;
	}
	return {
		e,
		u: using.bind(null, !1),
		a: using.bind(null, !0),
		d: function d() {
			var o, t = this.e, s = 0;
			function next() {
				for (; o = n.pop();) try {
					if (!o.a && 1 === s) return s = 0, n.push(o), Promise.resolve().then(next);
					if (o.d) {
						var r = o.d.call(o.v);
						if (o.a) return s |= 2, Promise.resolve(r).then(next, err);
					} else s |= 1;
				} catch (r) {
					return err(r);
				}
				if (1 === s) return t !== e ? Promise.reject(t) : Promise.resolve();
				if (t !== e) throw t;
			}
			function err(n) {
				return t = t !== e ? new r(n, t) : n, next();
			}
			return next();
		}
	};
}
var init_usingCtx = __esmMin((() => {}));
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
	try {
		var _usingCtx$1 = _usingCtx();
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
		const reader = _usingCtx$1.a(new StreamReader(res.body.getReader()));
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
	} catch (_) {
		_usingCtx$1.e = _;
	} finally {
		await _usingCtx$1.d();
	}
}
var StreamReader;
var init_ai = __esmMin((() => {
	init_api();
	init_schemas();
	init_usingCtx();
	StreamReader = class {
		reader;
		constructor(reader) {
			this.reader = reader;
		}
		read() {
			return this.reader.read();
		}
		async [Symbol.asyncDispose]() {
			await this.reader.cancel();
		}
	};
}));
//#endregion
//#region src/lib/routes.ts
function useRoute() {
	return useStore(router);
}
function clean(search) {
	if (!search) return {};
	const out = {};
	for (const [k, v] of Object.entries(search)) if (!OMITTED.has(k)) out[k] = v;
	return out;
}
function navigate(name, search) {
	openPage(router, name, {}, clean(search) ?? {});
}
/** Typed programmatic navigation (replace history entry). */
function redirect(name, search) {
	redirectPage(router, name, {}, clean(search) ?? {});
}
/** Build a route URL from its typed name + search params. */
function routeUrl(name, search) {
	return getPagePath(router, name, {}, clean(search) ?? {});
}
/** Open a raw URL through the router (string URLs, e.g. searchUrl() output).
* `replace` swaps the current history entry instead of pushing. */
function openPath(path, replace = false) {
	router.open(path, replace);
}
var config, router, OMITTED;
var init_routes$1 = __esmMin((() => {
	config = {
		home: "/",
		search: "/search",
		history: "/history",
		dashboard: "/dashboard"
	};
	router = createRouter(config);
	if (typeof window !== "undefined" && typeof location !== "undefined") router.open(location.pathname + location.search, true);
	OMITTED = /* @__PURE__ */ new Set(["p"]);
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
function ModelPicker({ models, value, onChange, disabled, id, size = "sm", label = model_aria_label(), modelsError }) {
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
					class: `input ${size === "xs" ? "select-xs" : "select-sm"} oxe-pill-control w-full pr-6 min-w-0`,
					value: open ? filter : value,
					placeholder: value || model_ph_filter(),
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
					children: modelsError ? model_listing_failed({ e: modelsError }) : model_none_hint()
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
	init_i18n();
	modelsVersion = 0;
}));
//#endregion
//#region src/lib/toasts.ts
/** Push a toast; auto-dismissed after 4s (timer cleared on dismiss). */
function toast(type, msg) {
	const t = {
		id: nextId++,
		type,
		msg
	};
	$toasts.set([...$toasts.get(), t]);
	timers.set(t.id, setTimeout(() => dismiss(t.id), 4e3));
}
function dismiss(id) {
	const timer = timers.get(id);
	if (timer) {
		clearTimeout(timer);
		timers.delete(id);
	}
	$toasts.set($toasts.get().filter((t) => t.id !== id));
}
/** Reactive toast list for the renderer. */
function useToasts() {
	return useStore($toasts);
}
var $toasts, nextId, timers;
var init_toasts = __esmMin((() => {
	$toasts = atom([]);
	nextId = 1;
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
			if (e?.name !== "AbortError") setLoadError(e?.message ?? settings_load_failed_msg());
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
			toast("success", settings_toast_saved());
			dialogRef.current?.close();
		}).catch((err) => {
			setSaving(false);
			const msg = err.message ?? settings_save_failed_msg();
			setSaveError(msg);
			toast("error", settings_toast_save_failed({ msg }));
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
				detail: settings_pick_model_first()
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
			toast(r.ok ? "success" : "error", r.detail || (r.ok ? settings_conn_ok() : settings_conn_failed()));
		}).catch((err) => {
			const detail = err.message ?? settings_test_failed();
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
		"aria-label": settings_aria(),
		children: [/* @__PURE__ */ jsxs("div", {
			class: "modal-box w-full max-w-md animate-in fade-in zoom-in-95 duration-150",
			children: [
				/* @__PURE__ */ jsx("h2", {
					class: "text-base font-semibold mb-3",
					children: settings_title()
				}),
				loadError && /* @__PURE__ */ jsx("div", {
					role: "alert",
					class: "alert alert-error text-sm mb-3",
					children: settings_load_failed({ e: loadError })
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
									children: settings_fieldset_ai()
								}),
								/* @__PURE__ */ jsx("label", {
									class: "label text-xs",
									for: "set-provider",
									children: settings_label_provider()
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
									children: settings_label_model()
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
								modelsError && models.length === 0 && /* @__PURE__ */ jsx("p", {
									class: "text-warning text-xs mt-1",
									role: "note",
									children: model_listing_failed({ e: modelsError })
								}),
								/* @__PURE__ */ jsx("label", {
									class: "label text-xs",
									for: "set-api-key",
									children: settings_label_api_key()
								}),
								/* @__PURE__ */ jsx("input", {
									id: "set-api-key",
									name: "api_key",
									type: "password",
									class: "input input-sm w-full",
									placeholder: settings_ph_api_key(),
									autocomplete: "off"
								}),
								err("api_key"),
								/* @__PURE__ */ jsx("label", {
									class: "label text-xs",
									for: "set-base-url",
									children: settings_label_base_url()
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
										children: testing ? /* @__PURE__ */ jsx("span", { class: "loading loading-spinner loading-xs" }) : settings_test_connection()
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
									}), settings_label_enabled()]
								})
							]
						}),
						/* @__PURE__ */ jsxs("fieldset", {
							class: "fieldset gap-2.5 mt-2",
							children: [
								/* @__PURE__ */ jsx("legend", {
									class: "fieldset-legend text-sm",
									children: settings_fieldset_theme()
								}),
								/* @__PURE__ */ jsx("div", {
									role: "radiogroup",
									"aria-label": settings_aria_theme(),
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
										children: t === "system" ? settings_theme_system() : t === "light" ? settings_theme_light() : settings_theme_dark()
									}, t))
								}),
								theme === "system" && /* @__PURE__ */ jsx("p", {
									class: "text-xs opacity-50",
									children: settings_theme_follows_os()
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
								children: settings_cancel()
							}), /* @__PURE__ */ jsx("button", {
								type: "submit",
								class: "btn btn-primary btn-sm",
								disabled: saving,
								children: saving ? /* @__PURE__ */ jsx("span", { class: "loading loading-dots loading-xs" }) : settings_save()
							})]
						})
					]
				})
			]
		}), /* @__PURE__ */ jsx("form", {
			method: "dialog",
			class: "modal-backdrop",
			children: /* @__PURE__ */ jsx("button", {
				"aria-label": settings_aria_close(),
				children: settings_backdrop_close()
			})
		})]
	});
}
var init_SettingsDialog = __esmMin((() => {
	init_ai();
	init_ModelPicker();
	init_toasts();
	init_theme();
	init_schema();
	init_i18n();
}));
//#endregion
//#region ~icons/lucide/github.jsx
var lucideGithub;
var init_github = __esmMin((() => {
	lucideGithub = (props) => /* @__PURE__ */ jsx("svg", {
		viewBox: "0 0 24 24",
		width: "1.2em",
		height: "1.2em",
		...props,
		children: /* @__PURE__ */ jsxs("g", {
			fill: "none",
			stroke: "currentColor",
			strokeLinecap: "round",
			strokeLinejoin: "round",
			strokeWidth: 2,
			children: [/* @__PURE__ */ jsx("path", { d: "M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5c.08-1.25-.27-2.48-1-3.5c.28-1.15.28-2.35 0-3.5c0 0-1 0-3 1.5c-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.4 5.4 0 0 0 4 9c0 3.5 3 5.5 6 5.5c-.39.49-.68 1.05-.85 1.65S8.93 17.38 9 18v4" }), /* @__PURE__ */ jsx("path", { d: "M9 18c-4.51 2-5-2-7-2" })]
		})
	});
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
	const page = useRoute();
	const settingsOpen = page?.search.settings === "open";
	const closeSettings = () => {
		const sp = new URLSearchParams(window.location.search);
		sp.delete("settings");
		const qs = sp.toString();
		openPath(`${window.location.pathname}${qs ? `?${qs}` : ""}`, true);
	};
	const isActive = (item) => item.exact ? page?.route === item.route : page?.route === item.route;
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
					href: routeUrl(item.route),
					class: `px-2 py-1 rounded ${isActive(item) ? "font-semibold" : "opacity-70 hover:opacity-100"}`,
					"aria-current": isActive(item) ? "page" : void 0,
					children: isActive(item) ? `[${item.label}]` : item.label
				}, item.route))
			}),
			/* @__PURE__ */ jsxs("span", {
				class: "ml-auto flex items-center gap-1",
				children: [
					/* @__PURE__ */ jsx("button", {
						type: "button",
						class: "btn btn-ghost btn-xs",
						"aria-label": header_aria_settings(),
						onClick: () => {
							const sp = new URLSearchParams(window.location.search);
							sp.set("settings", "open");
							const qs = sp.toString();
							openPath(`${window.location.pathname}${qs ? `?${qs}` : ""}`);
						},
						children: header_settings()
					}),
					/* @__PURE__ */ jsx("a", {
						href: "https://github.com/espetro/oxe",
						target: "_blank",
						rel: "noopener noreferrer",
						class: "btn btn-ghost btn-sm btn-circle",
						"aria-label": header_aria_github(),
						tabIndex: 0,
						children: /* @__PURE__ */ jsx(GitHubIcon, {})
					}),
					/* @__PURE__ */ jsxs("span", {
						class: "text-xs opacity-50 hidden sm:inline",
						children: ["v", "0.4.0"]
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
		class: `flex w-full flex-col items-center ${vh ? "justify-center grow min-h-[calc(100vh-3rem)]" : ""}`,
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
	init_i18n();
	init_routes$1();
	init_SettingsDialog();
	init_github();
	NAV = [
		{
			route: "home",
			label: nav_search(),
			exact: true
		},
		{
			route: "history",
			label: nav_history()
		},
		{
			route: "dashboard",
			label: nav_dashboard()
		}
	];
	GitHubIcon = () => /* @__PURE__ */ jsx(lucideGithub, {
		class: "w-4 h-4",
		"aria-hidden": "true"
	});
}));
//#endregion
//#region src/components/AboutHint.tsx
init_Header();
init_i18n();
function AboutHint() {
	return /* @__PURE__ */ jsxs("div", {
		class: "dropdown dropdown-end dropdown-top dropdown-hover dropdown-focus fixed bottom-3 left-3 z-40",
		children: [/* @__PURE__ */ jsx("button", {
			type: "button",
			class: "btn btn-ghost btn-xs btn-circle opacity-30 hover:opacity-70",
			"aria-label": about_aria(),
			children: "?"
		}), /* @__PURE__ */ jsxs("div", {
			class: "dropdown-content w-72 max-w-[min(288px,68vw)] bg-base-100 border border-base-300 rounded-md shadow-sm p-3 text-xs z-50",
			role: "note",
			children: [/* @__PURE__ */ jsx("p", {
				class: "mb-1.5",
				children: about_line1()
			}), /* @__PURE__ */ jsxs("p", {
				class: "opacity-60",
				children: [
					/* @__PURE__ */ jsx("span", {
						class: "font-medium opacity-80",
						children: about_search_label()
					}),
					about_search_body(),
					/* @__PURE__ */ jsx("span", {
						class: "font-medium opacity-80",
						children: about_ai_label()
					}),
					about_ai_body()
				]
			})]
		})]
	});
}
//#endregion
//#region src/components/ErrorBoundary.tsx
/** @jsxImportSource preact */
init_i18n();
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
						children: error_title()
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
							children: error_try_again()
						}), /* @__PURE__ */ jsx("a", {
							href: "/",
							class: "btn btn-ghost btn-sm",
							children: error_back()
						})]
					})
				]
			})
		});
		return this.props.children;
	}
};
//#endregion
//#region src/components/Toasts.tsx
init_toasts();
/** Fixed daisyUI toast stack (bottom-end). Mount once, next to the Header. */
function Toasts() {
	const list = useToasts();
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
//#endregion
//#region src/routes/_layout.tsx
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
/** Single source of truth for the search mode: the URL `mode` param when
* present, else the persisted localStorage preference, else "traditional".
* Both routes (/, /search) use this so the toggle and the rendered layout
* always agree from the first paint (fixes the reload divergence where the
* layout came from localStorage but the toggle from the absent URL param).
* Setters persist at event time; routes own their URL updates. */
function useSearchMode() {
	const [mode, setMode] = useState(() => {
		if (typeof window !== "undefined") {
			const url = new URLSearchParams(window.location.search).get("mode");
			if (url === "ai" || url === "traditional") return url;
		}
		if (typeof localStorage !== "undefined" && localStorage.getItem(MODE_KEY) === "ai") return "ai";
		return "traditional";
	});
	const setModeAndStore = (m) => {
		setMode(m);
		localStorage.setItem(MODE_KEY, m);
	};
	return [mode, setModeAndStore];
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
		"aria-label": mode_aria_label_lower(),
		class: "join bg-base-200 rounded-full p-0.5 shrink-0",
		children: SEGMENTS.map((s) => {
			const disabled = s.v === "ai" && aiDisabled;
			const active = mode === s.v;
			return /* @__PURE__ */ jsx("span", {
				class: "tooltip tooltip-bottom",
				"data-tip": disabled ? mode_tip_ai_disabled_short() : void 0,
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
					children: [s.v === "traditional" ? /* @__PURE__ */ jsx(Magnifier, {}) : /* @__PURE__ */ jsx(Sparkle, {}), s.label()]
				})
			}, s.v);
		})
	});
}
/** AI second-row controls: model picker + reasoning toggle chip.
* Pure UI state (localStorage); request wiring is a backend concern.
* `busy` (answer run in flight) disables the reasoning toggle: switching it
* mid-run has no effect on the stream and reads as a broken control. */
function AiControls({ available, models, modelsError, busy }) {
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
				children: ai_label_model()
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
			"aria-disabled": busy || void 0,
			disabled: busy,
			class: `btn btn-xs oxe-pill-control shrink-0 self-start sm:self-auto border ${reasoning ? "btn-primary btn-soft" : "btn-ghost"} ${busy ? "btn-disabled opacity-40" : ""}`,
			onClick: () => {
				if (!busy) setReasoning(!reasoning);
			},
			children: ai_label_reasoning()
		})]
	});
}
var MODE_KEY, SEGMENTS, Magnifier, Sparkle, STORE_KEY, REASONING_KEY;
var init_ModeSegments = __esmMin((() => {
	init_ai();
	init_ModelPicker();
	init_i18n();
	MODE_KEY = "oxe-mode";
	SEGMENTS = [{
		v: "traditional",
		label: segments_search
	}, {
		v: "ai",
		label: mode_label_ai
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
	init_i18n();
	AC_KEY = "oxe-ac";
	GROUP_LABEL = {
		history: suggest_group_history(),
		web: suggest_web_suggestions()
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
				children: [suggest_web_suggestions(), /* @__PURE__ */ jsx("input", {
					type: "checkbox",
					class: "toggle toggle-xs",
					checked: acOn !== false,
					onChange: (e) => setAcOn(e.target.checked),
					"aria-label": suggest_aria_toggle_web()
				})]
			})
		})]
	});
}
var init_SuggestionsDropdown = __esmMin((() => {
	init_useSuggests();
	init_i18n();
}));
//#endregion
//#region src/features/suggests/SearchBox.tsx
/** DDG-style pill search bar: rounded-full container, inline segmented
* mode toggle at the right end, and (AI mode) a second action row that
* reveals via a smooth morphism. Suggestions stay anchored to the pill.
* Does not fetch (the suggests hook owns that) and does not navigate. */
function SearchBox({ value, onInput, onSubmit, placeholder, autoFocus, busy, size = "lg", ariaLabel = searchbox_aria_search(), mode, onModeChange, aiAvailable, models = [], modelsError }) {
	const [open, setOpen] = useState(false);
	const [focused, setFocused] = useState(false);
	const boxRef = useRef(null);
	const inputRef = useRef(null);
	const typedRef = useRef(value);
	typedRef.current = value;
	const aiMode = mode === "ai";
	const ph = placeholder ?? (aiMode ? searchbox_ph_ai() : searchbox_ph_traditional());
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
	const input = /* @__PURE__ */ jsx("input", {
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
	});
	const submitBtn = /* @__PURE__ */ jsx("button", {
		type: "submit",
		class: "btn btn-ghost btn-sm btn-circle shrink-0",
		"aria-label": searchbox_aria_submit(),
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
	});
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
						input,
						mode && onModeChange ? /* @__PURE__ */ jsx(ModeSegments, {
							mode,
							onChange: onModeChange,
							aiAvailable: aiAvailable ?? null
						}) : null,
						submitBtn
					]
				}), mode === "ai" && /* @__PURE__ */ jsx("div", {
					class: "ai-row-in border-t border-base-200 mt-1.5 pt-1.5",
					children: /* @__PURE__ */ jsx(AiControls, {
						available: aiAvailable ?? null,
						models,
						modelsError,
						busy
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
	init_i18n();
}));
//#endregion
//#region src/routes/index.tsx
var routes_exports = /* @__PURE__ */ __exportAll({ default: () => Home });
function Home() {
	usePageTitle("");
	const [q, setQ] = useState("");
	const [mode, setMode] = useSearchMode();
	const { available: aiAvailable, models, error: modelsError } = useModels();
	const effectiveMode = mode === "ai" && aiAvailable === false ? "traditional" : mode;
	const submit = (query) => {
		const trimmed = query.trim();
		if (!trimmed) return;
		setMode(mode);
		navigate("search", effectiveMode === "ai" ? {
			q: trimmed,
			mode: "ai"
		} : { q: trimmed });
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
				children: home_tagline()
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
var init_routes = __esmMin((() => {
	init_Header();
	init_routes$1();
	init_ModeSegments();
	init_SearchBox();
	init_i18n();
}));
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
//#region src/features/search/ResultCard.tsx
/** Card-less Google-anatomy result: favicon + domain, blue title,
* two-line snippet, collapsed cached text preview. */
function ResultCard({ result, onOpen }) {
	const url = result.url ?? "";
	const domain = domainOf(url);
	const snippet = (result.text || result.highlights?.join(" ") || "").trim();
	const title = result.title || result_untitled();
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
					children: result_cached_text_preview()
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
	init_i18n();
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
		if (key) deleteCacheRow(key).catch(() => void 0).then(() => {
			if (qRef.current !== q) return;
			fetchPage(q, 1, "refresh");
		});
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
	return search_meta_results({ n: total });
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
	init_i18n();
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
	const streamingDeltas = status === "streaming";
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
						children: answer_heading()
					}),
					cached && /* @__PURE__ */ jsx("span", {
						class: "badge badge-ghost badge-xs",
						children: answer_from_cache()
					}),
					streamingDeltas && /* @__PURE__ */ jsx("span", {
						class: "loading loading-dots loading-xs opacity-50",
						role: "status",
						"aria-label": answer_aria_generating()
					}),
					/* @__PURE__ */ jsxs("span", {
						class: "ml-auto flex gap-2",
						children: [streaming && /* @__PURE__ */ jsx("button", {
							type: "button",
							class: "btn btn-ghost btn-xs",
							onClick: onStop,
							children: answer_stop()
						}), done && /* @__PURE__ */ jsx("button", {
							type: "button",
							class: "btn btn-ghost btn-xs",
							onClick: onViewClassic,
							children: answer_view_classic()
						})]
					})
				]
			}),
			steps.length > 0 && /* @__PURE__ */ jsx("ul", {
				class: "text-[13px] opacity-70 space-y-1",
				"aria-live": "polite",
				children: steps.map((s, i) => /* @__PURE__ */ jsxs("li", {
					class: "flex items-center gap-2 animate-in fade-in slide-in-from-bottom-2 duration-300",
					children: [!done && !(streamingDeltas && i === steps.length - 1) ? /* @__PURE__ */ jsx("span", { class: "loading loading-spinner loading-xs" }) : /* @__PURE__ */ jsx("span", {
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
						children: /* @__PURE__ */ jsx("span", { children: answer_stream_interrupted({ e: error }) })
					}),
					/* @__PURE__ */ jsxs("div", {
						class: "flex gap-2",
						children: [/* @__PURE__ */ jsx("button", {
							type: "button",
							class: "btn btn-sm",
							onClick: onRetry,
							children: answer_retry()
						}), /* @__PURE__ */ jsx("button", {
							type: "button",
							class: "btn btn-ghost btn-sm",
							onClick: onViewClassic,
							children: answer_switch_classic()
						})]
					})
				]
			}) : emptySources ? /* @__PURE__ */ jsxs("div", {
				class: "text-sm animate-in fade-in zoom-in-95 duration-300",
				children: [/* @__PURE__ */ jsx("p", {
					class: "opacity-60 mb-2",
					children: answer_no_sources()
				}), /* @__PURE__ */ jsx("button", {
					type: "button",
					class: "btn btn-sm",
					onClick: onViewClassic,
					children: answer_switch_classic()
				})]
			}) : text ? /* @__PURE__ */ jsxs("div", { children: [
				/* @__PURE__ */ jsx(MarkdownLite, { text }),
				streamingDeltas && /* @__PURE__ */ jsx("span", {
					class: "animate-pulse font-mono",
					"aria-hidden": "true",
					children: "▌"
				}),
				stopped && /* @__PURE__ */ jsx("p", {
					class: "text-xs opacity-50 mt-1",
					children: answer_stopped()
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
				children: answer_sources_heading()
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
					children: answer_related_heading()
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
			!text && !error && !emptySources && done && /* @__PURE__ */ jsx(Empty$1, { children: answer_none_produced() })
		]
	});
}
var init_AnswerView = __esmMin((() => {
	init_MarkdownLite();
	init_SourceCard();
	init_Header();
	init_i18n();
}));
//#endregion
//#region src/features/answer/useAnswer.ts
function stripAnswerMeta(text) {
	return text.replace(META_TAIL_RE, "");
}
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
		text: stripAnswerMeta(state.text + ev.text)
	};
	if (ev.type === "sources") return {
		...state,
		status: "streaming",
		sources: ev.sources
	};
	if (ev.type === "done") return {
		...state,
		text: ev.answer || stripAnswerMeta(state.text),
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
var META_TAIL_RE, INITIAL;
var init_useAnswer = __esmMin((() => {
	init_ai();
	init_useMountEffect();
	META_TAIL_RE = /\s*\{\s*"confidence"\s*:\s*\d+[\s\S]*?"related_questions"\s*:[\s\S]*?\}\s*$/;
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
function SearchRoute() {
	const query = useRoute()?.search ?? {};
	const q = String(query?.q ?? "");
	usePageTitle(q || search_page_title());
	/** Search URL params, preserving the cross-route ?settings=open flag. */
	const withSettings = (params) => query.settings ? {
		...params,
		settings: query.settings
	} : params;
	const aiAvailable = useAiAvailable();
	const { models, error: modelsError } = useModels();
	const [input, setInput] = useState(q);
	const [mode, setMode] = useSearchMode();
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
			openPath(`${window.location.pathname}${qs ? `?${qs}` : ""}`, true);
		}
	}, [query?.p]);
	useEffect(function rerunOnQueryChange() {
		setInput(q);
		if (q && effectiveMode === "traditional") run(q);
	}, [q, effectiveMode]);
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
	const changeMode = (next) => {
		setMode(next);
		if ((query?.mode === "ai" ? "ai" : "traditional") !== next) redirect("search", next === "ai" ? withSettings({
			q,
			mode: "ai"
		}) : withSettings({ q }));
	};
	useEffect(function runAnswerOnQueryOrModeChange() {
		if (q && effectiveMode === "ai") answer.run(q);
	}, [q, effectiveMode]);
	const submit = (raw) => {
		const t = raw.trim();
		if (!t) return;
		navigate("search", withSettings(mode === "ai" ? {
			q: t,
			mode: "ai"
		} : { q: t }));
	};
	const askAi = (query) => {
		setMode("ai");
		navigate("search", {
			q: query,
			mode: "ai"
		});
	};
	const viewClassic = () => {
		setMode("traditional");
		navigate("search", { q });
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
					children: search_ai_blocked()
				}),
				effectiveMode === "traditional" && results.length > 0 && payload && /* @__PURE__ */ jsxs("div", {
					class: "flex flex-wrap items-center gap-x-3 gap-y-1 text-[13px]",
					children: [
						/* @__PURE__ */ jsx("span", {
							class: "opacity-60",
							children: metaLine(payload, results.length) || (loading ? search_searching() : "")
						}),
						isCacheHit(payload) && /* @__PURE__ */ jsx("span", {
							class: "tooltip",
							"data-tip": search_tip_refresh(),
							children: /* @__PURE__ */ jsxs("button", {
								type: "button",
								class: "badge badge-sm badge-ghost cursor-pointer",
								"aria-label": search_aria_cached_refresh(),
								onClick: () => {
									refresh(q);
									toast("success", search_toast_refreshed());
								},
								children: ["cached", (() => {
									const age = cachedAgeOf(payload);
									return age != null ? search_cached_age({ age: fmtDur(age) }) : "";
								})()]
							})
						}),
						payload && /* @__PURE__ */ jsxs("span", {
							class: "flex gap-2 ml-auto",
							children: [/* @__PURE__ */ jsx("button", {
								type: "button",
								class: "btn btn-ghost btn-xs",
								onClick: () => navigator.clipboard?.writeText(window.location.href),
								children: search_copy_link()
							}), /* @__PURE__ */ jsx("button", {
								type: "button",
								class: "btn btn-ghost btn-xs",
								onClick: () => navigator.clipboard?.writeText(JSON.stringify({
									requestId: payload.requestId,
									results,
									costDollars: payload.costDollars
								})),
								children: search_copy_json()
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
					children: /* @__PURE__ */ jsx("span", { children: search_error_rate_limited() })
				}) : /* @__PURE__ */ jsx("div", {
					role: "alert",
					class: "alert alert-error text-sm",
					children: /* @__PURE__ */ jsx("span", { children: search_error_backend() })
				}), /* @__PURE__ */ jsxs("div", {
					class: "flex gap-2",
					children: [/* @__PURE__ */ jsx("button", {
						type: "button",
						class: "btn btn-sm",
						onClick: () => run(q),
						children: search_retry()
					}), aiAvailable === true && /* @__PURE__ */ jsx("button", {
						type: "button",
						class: "btn btn-ghost btn-sm",
						onClick: () => askAi(q),
						children: search_ask_ai_instead()
					})]
				})]
			}),
			!loading && !error && payload && results.length === 0 && /* @__PURE__ */ jsxs("div", {
				class: "py-10 text-sm animate-in fade-in zoom-in-95 duration-300",
				children: [/* @__PURE__ */ jsx("p", {
					class: "opacity-60 mb-3",
					children: search_no_results()
				}), aiAvailable === true && /* @__PURE__ */ jsx("button", {
					type: "button",
					class: "btn btn-sm",
					onClick: () => askAi(q),
					children: search_ask_ai_instead()
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
						"aria-label": search_aria_loading()
					})
				}),
				!state.hasNext && !state.loadingMore && !state.moreError && /* @__PURE__ */ jsx("p", {
					class: "py-6 text-center text-sm opacity-40",
					children: search_end_of_results()
				}),
				state.moreError && /* @__PURE__ */ jsxs("div", {
					class: "py-6 flex flex-col items-center gap-2 text-sm",
					children: [/* @__PURE__ */ jsx("p", {
						class: "opacity-60",
						children: search_more_error()
					}), /* @__PURE__ */ jsx("button", {
						type: "button",
						class: "btn btn-ghost btn-sm",
						onClick: () => loadMore(q),
						children: search_retry()
					})]
				}),
				state.hasNext && !state.loadingMore && !state.moreError && /* @__PURE__ */ jsx("div", {
					class: "py-6 flex justify-center",
					children: /* @__PURE__ */ jsx("button", {
						type: "button",
						class: "btn btn-ghost btn-sm opacity-60",
						onClick: () => loadMore(q),
						children: search_more_results()
					})
				})
			] })
		] })]
	});
}
var init_search = __esmMin((() => {
	init_Header();
	init_ModeSegments();
	init_api();
	init_toasts();
	init_ResultCard();
	init_search$1();
	init_routes$1();
	init_format();
	init_i18n();
	init_AnswerView();
	init_useAnswer();
	init_SearchBox();
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
			"aria-label": history_aria_delete_scope(),
			children: [
				/* @__PURE__ */ jsx("option", {
					value: "24h",
					children: history_delete_older_24h()
				}),
				/* @__PURE__ */ jsx("option", {
					value: "7d",
					children: history_delete_older_7d()
				}),
				/* @__PURE__ */ jsx("option", {
					value: "30d",
					children: history_delete_older_30d()
				}),
				/* @__PURE__ */ jsx("option", {
					value: "all",
					children: history_delete_all()
				})
			]
		}), /* @__PURE__ */ jsx("button", {
			type: "button",
			class: `btn btn-sm ${armed ? "btn-error" : "btn-ghost text-error"}`,
			disabled: busy,
			onClick: () => armed ? run() : setArmed(true),
			onBlur: () => setArmed(false),
			children: armed ? scope === "all" ? history_delete_confirm_all() : history_delete_confirm() : history_delete_arm()
		})]
	});
}
function HistoryRoute() {
	usePageTitle(history_page_title());
	const query = useRoute()?.search ?? {};
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
		openPath(`${window.location.pathname}${qs ? `?${qs}` : ""}`, true);
	};
	const clearFilters = () => {
		const sp = new URLSearchParams(window.location.search);
		sp.delete("since");
		sp.delete("qf");
		const qs = sp.toString();
		openPath(`${window.location.pathname}${qs ? `?${qs}` : ""}`, true);
	};
	return /* @__PURE__ */ jsxs("div", {
		class: "w-full max-w-[960px] mx-auto px-4 pb-16",
		children: [
			/* @__PURE__ */ jsx("h1", {
				class: "text-xl font-semibold mt-6 mb-1",
				children: history_title()
			}),
			/* @__PURE__ */ jsx("p", {
				class: "text-[13px] opacity-60 mb-4",
				children: history_summary({
					clicks: counts.clicks,
					rows: counts.cache_rows
				})
			}),
			/* @__PURE__ */ jsxs("div", {
				class: "flex flex-wrap items-center gap-2 mb-4",
				children: [
					/* @__PURE__ */ jsxs("select", {
						class: "select select-sm w-32",
						value: since,
						onChange: (e) => setParam("since", e.target.value),
						"aria-label": history_aria_time_filter(),
						children: [
							/* @__PURE__ */ jsx("option", {
								value: "all",
								children: history_opt_all_time()
							}),
							/* @__PURE__ */ jsx("option", {
								value: "24",
								children: history_opt_last_24h()
							}),
							/* @__PURE__ */ jsx("option", {
								value: "168",
								children: history_opt_last_week()
							}),
							/* @__PURE__ */ jsx("option", {
								value: "720",
								children: history_opt_last_month()
							})
						]
					}),
					/* @__PURE__ */ jsx("input", {
						type: "search",
						class: "input input-sm w-56",
						placeholder: history_filter_placeholder(),
						value: qf,
						onInput: (e) => setParam("qf", e.target.value),
						"aria-label": history_aria_query_filter()
					}),
					(since !== "all" || qf) && /* @__PURE__ */ jsx("button", {
						type: "button",
						class: "btn btn-ghost btn-sm",
						onClick: clearFilters,
						children: history_clear()
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
			!loading && error && /* @__PURE__ */ jsx("p", {
				class: "text-error py-6 text-sm",
				children: history_error({ e: error })
			}),
			!loading && !error && items.length === 0 && /* @__PURE__ */ jsxs("p", {
				class: "opacity-60 py-8 text-sm",
				children: [
					history_empty_prefix(),
					/* @__PURE__ */ jsx("a", {
						href: "/",
						class: "link link-primary",
						children: history_empty_link()
					}),
					history_empty_suffix()
				]
			}),
			!loading && items.length > 0 && /* @__PURE__ */ jsx("div", {
				class: "overflow-x-auto",
				children: /* @__PURE__ */ jsxs("table", {
					class: "table table-sm",
					children: [/* @__PURE__ */ jsx("thead", { children: /* @__PURE__ */ jsxs("tr", {
						class: "text-[13px] opacity-60",
						children: [
							/* @__PURE__ */ jsx("th", { children: history_col_when() }),
							/* @__PURE__ */ jsx("th", { children: history_col_kind() }),
							/* @__PURE__ */ jsx("th", { children: history_col_query() }),
							/* @__PURE__ */ jsx("th", {
								class: "hidden md:table-cell",
								children: history_col_detail()
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
									children: r.kind === "click" ? history_kind_click({ source: r.source ?? "web" }) : history_kind_search()
								})
							}),
							/* @__PURE__ */ jsx("td", {
								class: "text-[13px]",
								children: /* @__PURE__ */ jsx("a", {
									href: `/row/${r.query_hash}`,
									class: "link link-primary",
									onClick: (e) => {
										e.preventDefault();
										if (r.query) openPath(`/search?q=${encodeURIComponent(r.query)}`);
										else window.location.href = `/row/${r.query_hash}`;
									},
									children: r.query || history_no_query()
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
								}) : /* @__PURE__ */ jsx(Fragment, { children: history_hits({
									hits: r.hits ?? 0,
									at: fmtLocal(r.expires_at ?? 0)
								}) })
							}),
							/* @__PURE__ */ jsx("td", { children: r.kind === "click" && /* @__PURE__ */ jsx("button", {
								type: "button",
								class: "btn btn-ghost btn-xs",
								onClick: () => fetch(`/search?q=${encodeURIComponent(r.query || "")}`, { headers: { Accept: "application/json" } }).then((res) => res.text()).then((t) => navigator.clipboard?.writeText(t)).catch((err) => toast("error", history_copy_json_failed({ e: err.message }))),
								children: history_copy_json()
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
	init_toasts();
	init_i18n();
	init_routes$1();
	SINCE_VALUES = [
		"24",
		"168",
		"720",
		"all"
	];
}));
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
		children: dashboard_empty()
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
		"aria-label": dashboard_aria_sparkline(),
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
	usePageTitle(dashboard_page_title());
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
				children: dashboard_title()
			}),
			/* @__PURE__ */ jsx("p", {
				class: "text-[13px] opacity-60 mb-4",
				children: dashboard_window({ n: stats?.days ?? 14 })
			}),
			error && /* @__PURE__ */ jsx("p", {
				class: "text-error py-6 text-sm",
				children: dashboard_error({ e: error })
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
						title: dashboard_panel_per_day(),
						children: stats.searches_per_day.some((d) => d.total > 0) ? /* @__PURE__ */ jsxs(Fragment, { children: [/* @__PURE__ */ jsx(Sparkline, { days: stats.searches_per_day }), /* @__PURE__ */ jsx("p", {
							class: "text-[13px] opacity-60 mt-1",
							children: dashboard_per_day_total({ n: stats.searches_per_day.reduce((a, d) => a + d.total, 0) })
						})] }) : /* @__PURE__ */ jsx(Empty, {})
					}),
					/* @__PURE__ */ jsx(Panel, {
						title: dashboard_panel_hit_rate(),
						children: stats.hit_rate.rate != null ? /* @__PURE__ */ jsxs(Fragment, { children: [/* @__PURE__ */ jsxs("div", {
							class: "text-3xl font-bold",
							children: [stats.hit_rate.rate, "%"]
						}), /* @__PURE__ */ jsx("p", {
							class: "text-[13px] opacity-60",
							children: dashboard_hit_rate_detail({
								hits: stats.hit_rate.cache_hits,
								total: stats.hit_rate.total
							})
						})] }) : /* @__PURE__ */ jsx(Empty, {})
					}),
					/* @__PURE__ */ jsx(Panel, {
						title: dashboard_panel_latency(),
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
						title: dashboard_panel_clients(),
						children: stats.client_split.length > 0 ? /* @__PURE__ */ jsx("div", { children: stats.client_split.map((c) => /* @__PURE__ */ jsx(Bar, {
							value: c.count,
							label: c.client,
							max: Math.max(...stats.client_split.map((x) => x.count))
						}, c.client)) }) : /* @__PURE__ */ jsx(Empty, {})
					}),
					/* @__PURE__ */ jsx(Panel, {
						title: dashboard_panel_top_queries(),
						wide: true,
						children: stats.top_queries.length > 0 ? /* @__PURE__ */ jsx("div", { children: stats.top_queries.slice(0, 10).map((q) => /* @__PURE__ */ jsx(Bar, {
							value: q.count,
							label: q.query,
							max: Math.max(...stats.top_queries.slice(0, 10).map((x) => x.count))
						}, q.query)) }) : /* @__PURE__ */ jsx(Empty, {})
					}),
					/* @__PURE__ */ jsx(Panel, {
						title: dashboard_panel_zero_result(),
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
							children: dashboard_zero_result_none()
						})
					}),
					/* @__PURE__ */ jsx(Panel, {
						title: dashboard_panel_cache(),
						wide: true,
						children: /* @__PURE__ */ jsx("table", {
							class: "table table-sm text-[13px]",
							children: /* @__PURE__ */ jsxs("tbody", { children: [
								/* @__PURE__ */ jsxs("tr", { children: [
									/* @__PURE__ */ jsx("td", {
										class: "opacity-60",
										children: dashboard_cache_rows()
									}),
									/* @__PURE__ */ jsx("td", {
										class: "text-right",
										children: stats.cache.rows
									}),
									/* @__PURE__ */ jsx("td", {
										class: "opacity-60",
										children: dashboard_cache_unexpired()
									}),
									/* @__PURE__ */ jsx("td", {
										class: "text-right",
										children: stats.cache.unexpired_rows
									})
								] }),
								/* @__PURE__ */ jsxs("tr", { children: [
									/* @__PURE__ */ jsx("td", {
										class: "opacity-60",
										children: dashboard_cache_db_size()
									}),
									/* @__PURE__ */ jsx("td", {
										class: "text-right",
										children: fmtBytes(stats.cache.db_size_bytes)
									}),
									/* @__PURE__ */ jsx("td", {
										class: "opacity-60",
										children: dashboard_cache_total_hits()
									}),
									/* @__PURE__ */ jsx("td", {
										class: "text-right",
										children: stats.cache.total_hits
									})
								] }),
								/* @__PURE__ */ jsxs("tr", { children: [
									/* @__PURE__ */ jsx("td", {
										class: "opacity-60",
										children: dashboard_cache_newest()
									}),
									/* @__PURE__ */ jsx("td", {
										class: "text-right",
										children: fmtTs(stats.cache.newest)
									}),
									/* @__PURE__ */ jsx("td", {
										class: "opacity-60",
										children: dashboard_cache_oldest()
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
	init_i18n();
}));
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
				children: notfound_title()
			}),
			/* @__PURE__ */ jsx("p", {
				class: "mt-1 text-sm opacity-60",
				children: notfound_body()
			}),
			/* @__PURE__ */ jsxs("div", {
				class: "mt-6 flex items-center gap-2",
				children: [/* @__PURE__ */ jsx("a", {
					href: "/",
					class: "btn btn-primary btn-sm",
					children: notfound_back()
				}), /* @__PURE__ */ jsx("a", {
					href: "/history",
					class: "btn btn-ghost btn-sm",
					children: notfound_history()
				})]
			})
		]
	});
}
var init__404 = __esmMin((() => {
	init_i18n();
}));
//#endregion
//#region src/app.tsx
init_routes$1();
var withLayout = (load) => lazy(async () => {
	const { default: Page } = await load();
	return { default: (props) => /* @__PURE__ */ jsx(Layout, { children: /* @__PURE__ */ jsx(Page, { ...props }) }) };
});
var Pages = {
	home: withLayout(() => Promise.resolve().then(() => (init_routes(), routes_exports))),
	search: withLayout(() => Promise.resolve().then(() => (init_search(), search_exports))),
	history: withLayout(() => Promise.resolve().then(() => (init_history(), history_exports))),
	dashboard: withLayout(() => Promise.resolve().then(() => (init_dashboard(), dashboard_exports))),
	notFound: withLayout(() => Promise.resolve().then(() => (init__404(), _404_exports)))
};
function Routed() {
	const page = useRoute();
	const Page = page ? Pages[page.route] : Pages.notFound;
	return /* @__PURE__ */ jsx(Page, {});
}
function App() {
	return /* @__PURE__ */ jsx(ErrorBoundary, { children: /* @__PURE__ */ jsx(Routed, {}) });
}
//#endregion
//#region src/entry-prerender.tsx
init_routes$1();
/** Render the App for `url` to { html, links }. Routes with dynamic content
* (search results, history) prerender only the shell; data loads client-side
* on hydration. The router store has no window in SSR, so seed it with the
* target URL before rendering. */
async function prerenderApp(url) {
	locationStub(url);
	openPath(url);
	return await prerender(/* @__PURE__ */ jsx(App, {}));
}
//#endregion
export { prerenderApp };
