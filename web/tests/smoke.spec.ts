// Smoke test (SPEC §12): the page loads, holding W drives the car forward, and
// nothing logs an error.
//
// It waits on *simulated* time, not wall-clock time: on a slow machine with software
// graphics the page draws few frames, and the game runs at most 8 physics ticks per
// frame (SPEC §11.2), so 3 real seconds can be much less than 3 simulated seconds.
import { expect, test, type Page } from '@playwright/test';

/** Seconds of simulated full throttle; about 20 m of driving at the car's acceleration. */
const THROTTLE_SIM_S = 2;
/** Generous limit for slow CI machines. */
const SLOW_MACHINE_TIMEOUT_MS = 60_000;

interface DebugHook {
  chassisPose: () => number[];
  simTimeS: () => number;
}

/** Reads one value through the page's debug hook, or NaN before the game has started. */
async function readDebug(page: Page, what: 'x' | 'time'): Promise<number> {
  return page.evaluate((key) => {
    const hook = (window as unknown as { ninelivesDebug?: DebugHook }).ninelivesDebug;
    if (!hook) {
      return Number.NaN;
    }
    return key === 'x' ? (hook.chassisPose()[0] ?? 0) : hook.simTimeS();
  }, what);
}

test('holding W drives the car forward without console errors', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') {
      consoleErrors.push(message.text());
    }
  });

  await page.goto('/');
  await expect.poll(() => readDebug(page, 'x'), { timeout: SLOW_MACHINE_TIMEOUT_MS }).not.toBeNaN();
  const startX = await readDebug(page, 'x');
  const startTimeS = await readDebug(page, 'time');

  await page.keyboard.down('KeyW');
  await expect
    .poll(() => readDebug(page, 'time'), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThan(startTimeS + THROTTLE_SIM_S);
  await page.keyboard.up('KeyW');

  expect(await readDebug(page, 'x')).toBeGreaterThan(startX + 5);
  await expect(page.locator('#hud-speed')).not.toHaveText('0 km/h');
  expect(consoleErrors).toEqual([]);
});
