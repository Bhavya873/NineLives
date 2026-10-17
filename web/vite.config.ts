// Vite settings for the Nine Lives web game.
import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    target: 'es2022',
    // three.js alone is about 700 kB minified; one bundle is fine for this game.
    chunkSizeWarningLimit: 1000,
  },
  server: {
    // The game imports configs/vehicle.toml from the repo root. Allow only that folder
    // (plus this one), so the dev server never serves other files from the repo.
    fs: { allow: ['.', '../configs'] },
  },
});
