// Playwright smoke tests run against the production build (`vite preview`).
// Set PLAYWRIGHT_CHROMIUM_PATH to use an already installed Chromium instead of
// the one `npx playwright install` downloads.
import { defineConfig } from '@playwright/test';

const PREVIEW_PORT = 4173;

export default defineConfig({
  testDir: 'tests',
  use: {
    baseURL: `http://localhost:${PREVIEW_PORT}`,
    launchOptions: {
      executablePath: process.env.PLAYWRIGHT_CHROMIUM_PATH || undefined,
    },
  },
  webServer: {
    command: `npx vite preview --port ${PREVIEW_PORT} --strictPort`,
    url: `http://localhost:${PREVIEW_PORT}`,
    reuseExistingServer: !process.env.CI,
  },
});
