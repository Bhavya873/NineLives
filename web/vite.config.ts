// Vite settings for the Nine Lives web game: two pages, the game and the training viewer.
import { fileURLToPath } from 'node:url';

import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    target: 'es2022',
    // three.js alone is about 700 kB minified; one bundle is fine for this game.
    chunkSizeWarningLimit: 1000,
    rollupOptions: {
      input: {
        game: fileURLToPath(new URL('./index.html', import.meta.url)),
        viewer: fileURLToPath(new URL('./viewer.html', import.meta.url)),
      },
    },
  },
  server: {
    // The game imports configs/*.toml and models/policy.json from the repo root. Allow
    // only those folders (plus this one), so the dev server never serves other repo files.
    fs: { allow: ['.', '../configs', '../models'] },
  },
});
