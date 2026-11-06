// Training viewer smoke tests (SPEC §11.6): the page loads the newest run's checkpoints, every
// stage's crowd flies at once (each stage stepped in its own worker), cars land, the courses
// are drawn, a tab or a key focuses one stage, and nothing logs an error.
import { type Page, expect, test } from '@playwright/test';

import { collectConsoleErrors } from './console-errors.ts';

/** Generous limit for slow CI machines with software graphics. */
const SLOW_MACHINE_TIMEOUT_MS = 90_000;

/** One stage as the viewer's test hook reports it. */
interface StageReading {
  arena: string;
  round: number;
  rows: number;
  landed: number;
  crashed: number;
  shapeCount: number;
  running: boolean;
  stepping: boolean;
  visible: boolean;
}

interface ViewerHook {
  checkpointCount: () => number;
  landed: () => number;
  arena: () => string;
  focus: () => string | null;
  courseShapeCount: (arena?: string) => number;
  stages: () => StageReading[];
}

/** Every stage's state (empty until the page sets up its hook). */
function readStages(page: Page): Promise<StageReading[]> {
  return page.evaluate(() => {
    const hook = (window as unknown as { ninelivesViewer?: ViewerHook }).ninelivesViewer;
    return hook ? hook.stages() : [];
  });
}

/** Cars landed so far over every stage (-1 before the hook exists). */
async function readLanded(page: Page): Promise<number> {
  const stages = await readStages(page);
  return stages.length === 0 ? -1 : stages.reduce((sum, stage) => sum + stage.landed, 0);
}

/** One stage's reading. */
async function readStage(page: Page, arena: string): Promise<StageReading | undefined> {
  return (await readStages(page)).find((stage) => stage.arena === arena);
}

const ARENAS = ['flat', 'side', 'obstacles', 'driving', 'flight'];

test('the training viewer flies every stage at once and cars land', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);

  await page.goto('/viewer.html');

  await expect(page.locator('#viewer-status')).toContainText('8 checkpoints', {
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });
  await expect(page.locator('#viewer-status')).toContainText('5 stages');
  await expect(page.locator('#viewer-board')).toContainText('Checkpoint');
  // Five stage panels, one title each.
  await expect(page.locator('.panel-title')).toHaveCount(5);
  await expect(page.locator('.panel-title:visible')).toHaveCount(5);
  // Every stage's worker has posted all eight checkpoint rows.
  await expect
    .poll(async () => (await readStages(page)).map((stage) => stage.arena), {
      timeout: SLOW_MACHINE_TIMEOUT_MS,
    })
    .toEqual(ARENAS);
  await expect
    .poll(async () => (await readStages(page)).every((stage) => stage.rows === 8), {
      timeout: SLOW_MACHINE_TIMEOUT_MS,
    })
    .toBe(true);
  await expect
    .poll(() => readLanded(page), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThan(0);
  expect(consoleErrors).toEqual([]);
});

test('every course stage draws its shapes and the drop stages draw none', async ({ page }) => {
  // Each stage's worker builds all its crowds before it posts its first round, and CI runs the
  // debug wasm with five workers on a couple of cores: that takes about a minute, and all the
  // stages arrive together. So one shared deadline covers every stage, not one wait per stage.
  test.setTimeout(CROWD_BUILD_TIMEOUT_MS + SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);
  await page.goto('/viewer.html');

  await expect
    .poll(
      async () => {
        const counts = new Map<string, number>();
        for (const stage of await readStages(page)) {
          counts.set(stage.arena, stage.shapeCount);
        }
        // The arena's parked cars merge into a few vehicle meshes, so it has fewer children than shapes.
        return (
          (counts.get('obstacles') ?? 0) > 10 &&
          (counts.get('driving') ?? 0) > 1 &&
          (counts.get('flight') ?? 0) > 1
        );
      },
      { timeout: CROWD_BUILD_TIMEOUT_MS },
    )
    .toBe(true);
  expect((await readStage(page, 'flat'))?.shapeCount).toBe(0);
  expect((await readStage(page, 'side'))?.shapeCount).toBe(0);
  expect(consoleErrors).toEqual([]);
});

test('a stage tab focuses that stage and All shows every stage again', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);
  await page.goto('/viewer.html');
  await expect(page.locator('.panel-title:visible')).toHaveCount(5, {
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });

  await page.locator('#stage-tabs button', { hasText: /^D$/ }).click();

  await expect(page.locator('.panel-title:visible')).toHaveCount(1);
  await expect(page.locator('.panel-title:visible')).toContainText('Stage D course');
  await expect(page).toHaveURL(/arena=driving/);
  expect(
    await page.evaluate(() =>
      (window as unknown as { ninelivesViewer: ViewerHook }).ninelivesViewer.arena(),
    ),
  ).toBe('driving');
  // The stages nobody can see stop stepping; the focused one keeps going.
  const stages = await readStages(page);
  expect(stages.filter((stage) => stage.visible).map((stage) => stage.arena)).toEqual(['driving']);

  await page.locator('#stage-tabs button', { hasText: /^All$/ }).click();

  await expect(page.locator('.panel-title:visible')).toHaveCount(5);
  await expect(page).not.toHaveURL(/arena=/);
  expect(
    await page.evaluate(() =>
      (window as unknown as { ninelivesViewer: ViewerHook }).ninelivesViewer.arena(),
    ),
  ).toBe('all');
  expect(consoleErrors).toEqual([]);
});

test('clicking a panel title, a number key and Esc move the focus', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);
  await page.goto('/viewer.html');
  await expect(page.locator('.panel-title:visible')).toHaveCount(5, {
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });

  await page.locator('.panel-title[data-arena="obstacles"]').click();
  await expect(page).toHaveURL(/arena=obstacles/);
  await expect(page.locator('.panel-title:visible')).toHaveCount(1);

  await page.keyboard.press('Escape');
  await expect(page.locator('.panel-title:visible')).toHaveCount(5);

  await page.keyboard.press('Digit5');
  await expect(page).toHaveURL(/arena=flight/);
  await expect(page.locator('.panel-title:visible')).toContainText('Stage F flip plate');
  // Stage F flights end within seconds; the cars land.
  await expect
    .poll(async () => (await readStage(page, 'flight'))?.landed ?? 0, {
      timeout: SLOW_MACHINE_TIMEOUT_MS,
    })
    .toBeGreaterThan(0);
  expect(consoleErrors).toEqual([]);
});

test('?arena=driving opens focused on Stage D and has no Gauntlet anywhere', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);
  await page.goto('/viewer.html?arena=driving');

  await expect(page.locator('.panel-title:visible')).toHaveCount(1, {
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });
  await expect(page.locator('.panel-title:visible')).toContainText('Stage D course');
  await expect(page.locator('body')).not.toContainText(/gauntlet/i);

  // The old Gauntlet link now opens the all-stages view.
  await page.goto('/viewer.html?arena=gauntlet');
  await expect(page.locator('.panel-title:visible')).toHaveCount(5, {
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });
  expect(consoleErrors).toEqual([]);
});

test('the stage panels tile the canvas beside the side column', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto('/viewer.html');
  await expect(page.locator('.panel-title:visible')).toHaveCount(5, {
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });

  const side = await page.locator('#viewer-side').boundingBox();
  const titles = await page.locator('.panel-title').evaluateAll((elements) =>
    elements.map((element) => {
      const box = element.getBoundingClientRect();
      return { left: box.left, right: box.right, top: box.top };
    }),
  );
  expect(side).not.toBeNull();
  for (const title of titles) {
    // No title sits under the side column.
    expect(title.right).toBeLessThanOrEqual((side?.x ?? 0) + 1);
  }
  // Two rows of panels: the titles have two distinct heights.
  expect(new Set(titles.map((title) => Math.round(title.top))).size).toBe(2);
});

test("F3 shows the frame rate and the workers' stepping cost", async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  await page.goto('/viewer.html');
  await expect(page.locator('.panel-title:visible')).toHaveCount(5, {
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });
  await expect(page.locator('#hud-perf')).toBeHidden();

  await page.keyboard.press('F3');

  await expect(page.locator('#hud-perf')).toContainText('fps', {
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });
  await expect(page.locator('#hud-perf-steps')).toContainText('worker ms per tick');
});

test('a hidden tab stops every stage and showing it again starts them', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  await page.goto('/viewer.html');
  await expect(page.locator('.panel-title:visible')).toHaveCount(5, {
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });
  const setHidden = (hidden: boolean) =>
    page.evaluate((value) => {
      Object.defineProperty(document, 'hidden', { configurable: true, get: () => value });
      document.dispatchEvent(new Event('visibilitychange'));
    }, hidden);

  await setHidden(true);
  await expect
    .poll(async () => (await readStages(page)).every((stage) => !stage.stepping))
    .toBe(true);

  await setHidden(false);
  await expect
    .poll(async () => (await readStages(page)).every((stage) => stage.stepping))
    .toBe(true);
});

test('blocked workers give one clear message, not "undefined"', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  await page.addInitScript(() => {
    (window as unknown as { Worker: unknown }).Worker = function BlockedWorker() {
      throw new Error('workers are blocked');
    };
  });
  const status = page.locator('#viewer-status');
  await page.goto('/viewer.html');

  await expect(status).toContainText('all stages failed', { timeout: SLOW_MACHINE_TIMEOUT_MS });
  await expect(status).toContainText('could not start');
  await expect(status).not.toContainText('undefined');
  // One message for five stages, not five copies.
  expect(((await status.textContent()) ?? '').match(/could not start/g)).toHaveLength(1);
});
