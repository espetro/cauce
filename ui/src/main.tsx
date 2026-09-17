import { hydrate } from "preact-iso";
import { App } from "./app";
import { initTheme } from "./lib/theme";
import "./index.css";

// Prerendered pages carry <script type="isodata" data-url="...">. Hydrate only
// when that URL matches the current address; otherwise the static shell was
// prerendered for a different route (the backend serves dist/index.html for
// every SPA path), so drop it and render clean — deep links stay correct.
const container = document.getElementById("app")!;
const isodata = document.querySelector("script[type=isodata]");
if (isodata && isodata.getAttribute("data-url") !== location.pathname + location.search) {
  isodata.remove();
  container.replaceChildren();
}

hydrate(<App />, container);
initTheme();
