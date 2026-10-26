// Training viewer smoke test (SPEC §11.6): the page loads the newest run's checkpoints,
// the crowd flies, cars land, and nothing logs an error, on the flat arena and on the
// obstacle arena.
import { type Page, expect, test } from '@playwright/test';

import { collectConsoleErrors } from './console-errors.ts';

/** Generous limit for slow CI machines with software graphics. */
const SLOW_MACHINE_TIMEOUT_MS = 90_000;

/** What the viewer's test hook reports, read in one go (null until the page sets it up). */
interface ViewerReading {
  landed: number;
  arena: string;
  courseShapeCount: number;
}

interface ViewerHook {
  checkpointCount: () => number;
  landed: () => number;
  arena: () => string;
  courseShapeCount: () => number;
}

/** Reads the viewer's test hook. */
function readViewer(page: Page): Promise<ViewerReading | null> {
  return page.evaluate(() => {
    const hook = (window as unknown as { ninelivesViewer?: ViewerHook }).ninelivesViewer;
    if (!hook) {
      return null;
    }
    return {
      landed: hook.landed(),
      arena: hook.arena(),
      courseShapeCount: hook.courseShapeCount(),
    };
  });
}

/** Cars landed so far (-1 before the hook exists). */
async function readLanded(page: Page): Promise<number> {
  return (await readViewer(page))?.landed ?? -1;
}

/** Waits until the status line names `label` ("flat arena" or "obstacle arena"). */
async function expectArena(page: Page, label: string): Promise<void> {
  await expect(page.locator('#viewer-status')).toContainText(label, {
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });
}

test('the training viewer flies every checkpoint at once and cars land', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);

  await page.goto('/viewer.html');

  await expect(page.locator('#viewer-status')).toContainText('checkpoints at once', {
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });
  await expect(page.locator('#viewer-board')).toContainText('Checkpoint');
  await expect
    .poll(() => readLanded(page), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThan(0);
  expect(consoleErrors).toEqual([]);
});

test('the viewer switches to the obstacle arena, draws it, and cars land there', async ({
  page,
}) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);
  await page.goto('/viewer.html');
  await expectArena(page, 'flat arena');

  await page.locator('#arena-toggle').click();

  await expectArena(page, 'obstacle arena');
  await expect(page).toHaveURL(/arena=obstacles/);
  await expect
    .poll(async () => (await readViewer(page))?.courseShapeCount ?? 0)
    .toBeGreaterThan(20);
  expect((await readViewer(page))?.arena).toBe('obstacles');
  await expect
    .poll(() => readLanded(page), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThan(0);
  expect(consoleErrors).toEqual([]);
});

test('?arena=obstacles opens on the obstacle arena and O goes back to flat', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);
  await page.goto('/viewer.html?arena=obstacles');
  await expectArena(page, 'obstacle arena');

  await page.keyboard.press('KeyO');

  await expectArena(page, 'flat arena');
  await expect.poll(async () => (await readViewer(page))?.courseShapeCount ?? -1).toBe(0);
  expect(consoleErrors).toEqual([]);
});
