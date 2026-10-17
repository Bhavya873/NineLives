// Training viewer smoke test (SPEC §11.6): the page loads the newest run's checkpoints,
// the crowd flies, cars land, and nothing logs an error.
import { expect, test } from '@playwright/test';

import { collectConsoleErrors } from './console-errors.ts';

/** Generous limit for slow CI machines with software graphics. */
const SLOW_MACHINE_TIMEOUT_MS = 90_000;

interface ViewerHook {
  checkpoint: () => number;
  checkpointCount: () => number;
  landed: () => number;
}

test('the training viewer replays a checkpoint and cars land', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);

  await page.goto('/viewer.html');
  const readLanded = () =>
    page.evaluate(() => {
      const hook = (window as unknown as { ninelivesViewer?: ViewerHook }).ninelivesViewer;
      return hook ? hook.landed() : -1;
    });

  await expect(page.locator('#viewer-status')).toContainText('checkpoint 1 of', {
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });
  await expect.poll(readLanded, { timeout: SLOW_MACHINE_TIMEOUT_MS }).toBeGreaterThan(0);
  expect(consoleErrors).toEqual([]);
});
