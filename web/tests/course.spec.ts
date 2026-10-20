// The Gauntlet in the browser (SPEC §7.11, §11.5, M7): ?course=gauntlet opens straight
// into the course, which is drawn, shows "Checkpoint 0/6" and logs no errors; F goes
// back to free driving in the playground (no checkpoints) and Enter starts the course
// again. A plain visit opens in the playground, and ?world=flat in the empty arena.
import { expect, test, type Page } from '@playwright/test';

import { collectConsoleErrors } from './console-errors.ts';

/** Generous limit for slow CI machines. */
const SLOW_MACHINE_TIMEOUT_MS = 60_000;

interface CourseDebugHook {
  courseMeshCount: () => number;
  checkpointCount: () => number;
}

/** Reads one course value through the debug hook, or NaN before the game has started. */
async function readCourse(page: Page, what: 'meshes' | 'checkpoints'): Promise<number> {
  return page.evaluate((key) => {
    const hook = (window as unknown as { ninelivesDebug?: CourseDebugHook }).ninelivesDebug;
    if (!hook) {
      return Number.NaN;
    }
    return key === 'meshes' ? hook.courseMeshCount() : hook.checkpointCount();
  }, what);
}

test('the Gauntlet loads from the URL, and F and Enter switch courses', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);

  await page.goto('/?course=gauntlet');

  await expect
    .poll(() => readCourse(page, 'meshes'), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThan(20);
  expect(await readCourse(page, 'checkpoints')).toBe(6);
  await expect(page.locator('#hud-status')).toContainText('Checkpoint 0/6');

  await page.keyboard.press('KeyF');
  expect(await readCourse(page, 'meshes')).toBeGreaterThan(60);
  expect(await readCourse(page, 'checkpoints')).toBe(0);
  await expect(page.locator('#hud-status')).not.toContainText('Checkpoint');

  await page.keyboard.press('Enter');
  expect(await readCourse(page, 'checkpoints')).toBe(6);
  expect(consoleErrors).toEqual([]);
});

test('free driving opens in the playground, and ?world=flat in the empty arena', async ({
  page,
}) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);

  await page.goto('/');
  await expect
    .poll(() => readCourse(page, 'meshes'), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThan(60);
  expect(await readCourse(page, 'checkpoints')).toBe(0);

  await page.goto('/?world=flat');
  await expect.poll(() => readCourse(page, 'meshes'), { timeout: SLOW_MACHINE_TIMEOUT_MS }).toBe(0);
  expect(consoleErrors).toEqual([]);
});
