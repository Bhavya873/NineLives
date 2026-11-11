// Playwright smoke tests run against the production build (`vite preview`).
// Set PLAYWRIGHT_CHROMIUM_PATH to use an already installed Chromium instead of
// the one `npx playwright install` downloads.
import { defineConfig } from '@playwright/test';

// Set PLAYWRIGHT_PREVIEW_PORT to run next to another preview server (Playwright reuses a server
// that is already on the port, so a second checkout would test the first one's build).
const PREVIEW_PORT = Number(process.env.PLAYWRIGHT_PREVIEW_PORT ?? 4173);

export default defineConfig({
  testDir: 'tests',
  // CI runs the debug wasm on a small runner, where one page boot can take longer than the
  // 30 s default; the slow viewer specs set their own, longer timeouts.
  timeout: process.env.CI ? 90_000 : 30_000,
  // CI runs one test at a time: with 2 workers, viewer.spec.ts and viewer-landscape.spec.ts ran
  // together, so 10 stage Web Workers shared 4 cores and the landing waits timed out now and then.
  workers: process.env.CI ? 1 : undefined,
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
