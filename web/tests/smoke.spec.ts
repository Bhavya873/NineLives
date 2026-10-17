// Smoke test: the page loads, the wasm module starts and nothing logs an error.
import { expect, test } from '@playwright/test';

test('page loads the wasm module without console errors', async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') {
      consoleErrors.push(message.text());
    }
  });

  await page.goto('/');

  await expect(page.locator('#build-info')).toContainText('120 Hz');
  expect(consoleErrors).toEqual([]);
});
