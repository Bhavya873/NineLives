// The HUD (SPEC §11.5, §11.7): the keys panel is always on screen, nine hearts show at
// the start, Esc pauses and resumes the game, and F3 shows the frame-rate readout.
import { expect, test, type Page } from '@playwright/test';

import { collectConsoleErrors } from './console-errors.ts';

/** Generous limit for slow CI machines. */
const SLOW_MACHINE_TIMEOUT_MS = 60_000;
/** Lives per run (src/ui/lives.ts). */
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
  await page.goto('/?play=1');
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
  await page.goto('/?play=1');
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
  await page.goto('/?play=1');
  await waitForGame(page);
  const perf = page.locator('#hud-perf');
  await expect(perf).toBeHidden();

  await page.keyboard.press('F3');

  await expect(perf).toBeVisible();
  await expect(perf).toContainText('fps', { timeout: SLOW_MACHINE_TIMEOUT_MS });
  await expect(perf).toContainText('tick');
});

/** A course with one ramp that needs 60 m/s (216 km/h), `rampX` m down the road from the start. */
function rampCourseJson(rampX: number): string {
  return JSON.stringify({
    version: 1,
    name: 'hint',
    start: { pos: [0, 0.7, 0], yaw: 0 },
    bounds: { min: [-50, -20, -50], max: [600, 60, 50] },
    pieces: [
      {
        type: 'ramp',
        pos: [rampX, 0, 0],
        length: 11.5,
        width: 8,
        angle_deg: 6,
        v_complete_min_m_s: 60,
      },
    ],
  });
}

/** Starts a course of the test's own through the test-only debug hook. */
async function loadCourse(page: Page, courseJson: string): Promise<void> {
  await page.evaluate((json) => {
    const hook = (window as unknown as { ninelivesDebug: { loadCourse: (j: string) => void } })
      .ninelivesDebug;
    hook.loadCourse(json);
  }, courseJson);
}

// No driving: the car starts at rest, so the hint does not depend on how fast the machine
// runs the sim (CI's software GL gives about 3 fps).
test('the HUD warns before a piece that needs more speed, and the warning clears', async ({
  page,
}) => {
  const consoleErrors = collectConsoleErrors(page);
  await page.goto('/?play=1');
  await waitForGame(page);

  // The ramp is 60 m ahead of a car at rest: inside the 80 m look-ahead, far too slow.
  await loadCourse(page, rampCourseJson(60));
  await expect(page.locator('#hud-too-slow')).toContainText('need 216 km/h', { timeout: 15_000 });

  // A course whose ramp is 300 m away is outside the look-ahead: the warning goes.
  await loadCourse(page, rampCourseJson(300));
  await expect(page.locator('#hud-too-slow')).toBeHidden({ timeout: 15_000 });
  expect(consoleErrors).toEqual([]);
});
