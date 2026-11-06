// Pressing a stunt key again while a stunt runs adds one more rotation (round 7): two taps
// of F in Unlimited air on a long fall fly a double back flip, and the landing toast says so.
// Uses the existing debug hooks only (loadCourse), no new ones.
import { expect, test, type Page } from '@playwright/test';

import { collectConsoleErrors } from './console-errors.ts';

/** Generous limit for slow CI machines. */
const SLOW_MACHINE_TIMEOUT_MS = 60_000;

/** A flat course with the car starting 150 m up: a fall of about 5.5 s, room for two flips. */
const HIGH_START_COURSE = JSON.stringify({
  version: 1,
  name: 'high start',
  start: { pos: [0, 150, 0], yaw: 0 },
  bounds: { min: [-300, -20, -300], max: [300, 400, 300] },
  pieces: [],
});

async function simTimeS(page: Page): Promise<number> {
  return page.evaluate(() => {
    const hook = (window as unknown as { ninelivesDebug?: { simTimeS: () => number } })
      .ninelivesDebug;
    return hook ? hook.simTimeS() : Number.NaN;
  });
}

test('tapping the flip key twice flies a double back flip and says so', async ({ page }) => {
  test.setTimeout(3 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);
  await page.goto('/?play=1');
  await expect.poll(() => simTimeS(page), { timeout: SLOW_MACHINE_TIMEOUT_MS }).toBeGreaterThan(0);

  // Unlimited air: a 150 m fall leaves no tank for the landing reserve in Limited air.
  await page.keyboard.press('KeyM');
  await page.evaluate((json) => {
    const hook = (window as unknown as { ninelivesDebug: { loadCourse: (j: string) => void } })
      .ninelivesDebug;
    hook.loadCourse(json);
  }, HIGH_START_COURSE);

  await page.keyboard.press('KeyF');
  await page.keyboard.press('KeyF');

  // The toast shows for about a second: poll for the name as the landing happens.
  const toast = page.locator('#hud-toast');
  await expect(toast).toHaveText('double back flip', { timeout: 2 * SLOW_MACHINE_TIMEOUT_MS });
  expect(consoleErrors).toEqual([]);
});
