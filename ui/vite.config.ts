import { defineConfig } from "vite";
import preact from "@preact/preset-vite";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [preact(), tailwindcss()],
  server: {
    proxy: {
      "/search": "http://127.0.0.1:4479",
      "/suggest": "http://127.0.0.1:4479",
      "/history": "http://127.0.0.1:4479",
      "/click": "http://127.0.0.1:4479",
      "/mcp": "http://127.0.0.1:4479",
    },
  },
});
