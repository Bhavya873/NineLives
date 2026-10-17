// Vite settings for the Nine Lives web game.
import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    target: 'es2022',
    // three.js alone is about 700 kB minified; one bundle is fine for this game.
    chunkSizeWarningLimit: 1000,
  },
  server: {
    // The game imports configs/*.toml and models/policy.json from the repo root. Allow
    // only those folders (plus this one), so the dev server never serves other repo files.
    fs: { allow: ['.', '../configs', '../models'] },
  },
});
