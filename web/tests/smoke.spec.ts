// Smoke test (SPEC §12): the page loads, holding W drives the car forward, and
// nothing logs an error.
import { expect, test } from '@playwright/test';

/** Reads the car's position through the page's debug hook. */
async function chassisX(page: import('@playwright/test').Page): Promise<number> {
  return page.evaluate(() => {
    const hook = (window as unknown as { ninelivesDebug?: { chassisPose: () => number[] } })
      .ninelivesDebug;
    return hook ? (hook.chassisPose()[0] ?? 0) : Number.NaN;
  });
}

test('holding W drives the car forward without console errors', async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') {
      consoleErrors.push(message.text());
    }
  });

  await page.goto('/');
  await expect.poll(() => chassisX(page)).not.toBeNaN();
  const startX = await chassisX(page);

  await page.keyboard.down('KeyW');
  await page.waitForTimeout(3000);
  await page.keyboard.up('KeyW');

  expect(await chassisX(page)).toBeGreaterThan(startX + 5);
  await expect(page.locator('#hud-speed')).not.toHaveText('0 km/h');
  expect(consoleErrors).toEqual([]);
});
