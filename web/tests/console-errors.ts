// Shared by the Playwright tests: collects every console error a page logs, so a test can
// assert the page ran clean.
import type { Page } from '@playwright/test';

/** Collects the text of every console error the page logs from now on. */
export function collectConsoleErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') {
      errors.push(message.text());
    }
  });
  return errors;
}
