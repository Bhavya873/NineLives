// Playwright smoke tests run against the production build (`vite preview`).
// Set PLAYWRIGHT_CHROMIUM_PATH to use an already installed Chromium instead of
// the one `npx playwright install` downloads.
import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: 'tests',
  use: {
    baseURL: 'http://localhost:4173',
    launchOptions: {
      executablePath: process.env.PLAYWRIGHT_CHROMIUM_PATH || undefined,
    },
  },
  webServer: {
    command: 'npm run preview',
    url: 'http://localhost:4173',
    reuseExistingServer: !process.env.CI,
  },
});
