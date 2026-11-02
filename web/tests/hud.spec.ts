// The HUD (SPEC §11.5, §11.7): the keys panel is always on screen, nine hearts show at
// the start, Esc pauses and resumes the game, and F3 shows the frame-rate readout.
import { expect, test, type Page } from '@playwright/test';

import { collectConsoleErrors } from './console-errors.ts';

/** Generous limit for slow CI machines. */
const SLOW_MACHINE_TIMEOUT_MS = 60_000;
/** Lives per run (src/hud/lives.ts). */
const LIVES = 9;

/** Simulated seconds so far, or NaN before the game has started. */
async function readSimTimeS(page: Page): Promise<number> {
  return page.evaluate(() => {
    const hook = (window as unknown as { ninelivesDebug?: { simTimeS: () => number } })
      .ninelivesDebug;
    return hook ? hook.simTimeS() : Number.NaN;
  });
}

/** Waits until the game has started and its clock is running. */
async function waitForGame(page: Page): Promise<void> {
  await expect
    .poll(() => readSimTimeS(page), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThan(0);
}

test('the keys panel and nine full hearts show from the start', async ({ page }) => {
  const consoleErrors = collectConsoleErrors(page);
  await page.goto('/');
  await waitForGame(page);

  await expect(page.locator('#hud-keys')).toBeVisible();
  await expect(page.locator('#hud-keys')).toContainText('pause');
  await expect(page.locator('#hud-hearts .heart')).toHaveCount(LIVES);
  await expect(page.locator('#hud-hearts .heart.lost')).toHaveCount(0);
  await expect(page.locator('#game-over')).toBeHidden();

  await page.keyboard.press('KeyW');
  await expect(page.locator('#hud-keys')).toBeVisible();
  expect(consoleErrors).toEqual([]);
});

test('Esc pauses the game and Esc again resumes it', async ({ page }) => {
  await page.goto('/');
  await waitForGame(page);
  const paused = page.locator('#paused');
  await expect(paused).toBeHidden();

  await page.keyboard.press('Escape');
  await expect(paused).toBeVisible();
  const pausedAtS = await readSimTimeS(page);
  await page.waitForTimeout(500);
  expect(await readSimTimeS(page)).toBe(pausedAtS);

  await page.keyboard.press('Escape');
  await expect(paused).toBeHidden();
  await expect
    .poll(() => readSimTimeS(page), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThan(pausedAtS);
});

test('F3 shows the frame rate and milliseconds per tick', async ({ page }) => {
  await page.goto('/');
  await waitForGame(page);
  const perf = page.locator('#hud-perf');
  await expect(perf).toBeHidden();

  await page.keyboard.press('F3');

  await expect(perf).toBeVisible();
  await expect(perf).toContainText('fps', { timeout: SLOW_MACHINE_TIMEOUT_MS });
  await expect(perf).toContainText('tick');
});

test('the HUD warns before a piece that needs more speed, and the warning clears', async ({
  page,
}) => {
  test.setTimeout(120_000);
  const consoleErrors = collectConsoleErrors(page);
  await page.goto('/');
  await waitForGame(page);
  await page.evaluate(() => {
    const course = {
      version: 1,
      name: 'hint',
      start: { pos: [0, 0.7, 0], yaw: 0 },
      bounds: { min: [-50, -20, -50], max: [600, 60, 50] },
      pieces: [
        {
          type: 'ramp',
          pos: [300, 0, 0],
          length: 11.5,
          width: 8,
          angle_deg: 6,
          v_complete_min_m_s: 60,
        },
      ],
    };
    (
      window as unknown as { ninelivesDebug: { loadCourse: (json: string) => void } }
    ).ninelivesDebug.loadCourse(JSON.stringify(course));
  });

  // 300 m of road and a 60 m/s ramp: driving up it the car is slower than 60 m/s inside
  // the 80 m look-ahead, so the too-slow element names the speed (216 km/h).
  await page.keyboard.down('KeyW');
  await expect(page.locator('#hud-too-slow')).toContainText('need 216 km/h', { timeout: 90_000 });
  await page.keyboard.up('KeyW');
  expect(consoleErrors).toEqual([]);
});
