// MPL-2.0
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [svelte()],
  build: {
    // Landing must stay lean: warn loudly rather than hide growth.
    chunkSizeWarningLimit: 150,
  },
});
