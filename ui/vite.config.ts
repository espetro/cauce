import { defineConfig } from "vite";
import preact from "@preact/preset-vite";
import tailwindcss from "@tailwindcss/vite";

// Document navigations must fall through to the SPA (index.html);
// only fetch/XHR traffic is proxied to the oxe backend.
const isDocument = (req: { headers: Record<string, string | string[] | undefined> }) =>
  (req.headers["sec-fetch-dest"] ?? "") === "document";

export default defineConfig({
  plugins: [preact(), tailwindcss()],
  server: {
    proxy: {
      "/search": {
        target: "http://127.0.0.1:4479",
        // POST /search from the app is JSON; a GET document hit should
        // render the SPA instead of the backend's HTML page.
        bypass: (req) => {
          if (isDocument(req)) return "/index.html";
        },
      },
      "/suggest": "http://127.0.0.1:4479",
      "/history": {
        target: "http://127.0.0.1:4479",
        // direct SPA navigation to /history serves index.html instead
        bypass: (req) => {
          if (isDocument(req)) return "/index.html";
        },
      },
      "/click": "http://127.0.0.1:4479",
      "/cache/stats": "http://127.0.0.1:4479",
      "/row": "http://127.0.0.1:4479",
      "/mcp": "http://127.0.0.1:4479",
    },
  },
});
