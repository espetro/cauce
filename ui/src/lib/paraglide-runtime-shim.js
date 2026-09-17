// Minimal client-side stand-in for the paraglide runtime.
//
// This app is EN-only (see vite.config.ts paraglide options), so the full
// locale-detection machinery (cookie/globalVariable strategies, setLocale,
// server middleware) tree-shakes poorly: the compiled messages import
// `getLocale`, and because it is an assignable `export let`, Rollup keeps
// its entire transitive graph (~19KB gz). This shim provides the same
// API surface for the EN-only case at ~0KB.
//
// If multi-locale support is ever added, delete this alias from
// vite.config.ts and let the real runtime back in.
export const baseLocale = "en";
export const locales = /** @type {const} */ (["en"]);
export const experimentalStaticLocale = /** @type {"en"} */ ("en");
export let getLocale = () => experimentalStaticLocale;
export const isServer = typeof window === "undefined";
export const cookieName = "PARAGLIDE_LOCALE";
export const cookieMaxAge = 34560000;
export const cookieDomain = "";
export const setLocale = () => {};
export const overwriteGetLocale = (fn) => {
  getLocale = fn;
};
export const overwriteServerAsyncLocalStorage = () => {};
export let serverAsyncLocalStorage = undefined;
