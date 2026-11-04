// UI polish (SPEC M8): the title screen, the results overlay, the ghost run, reduced motion
// and the folding key list. The CI runner draws about 3 frames a second, so nothing here
// relies on driving in real time: tests read the sim's own clock or use the debug hooks.
import { expect, test, type Page } from '@playwright/test';

import { collectConsoleErrors } from './console-errors.ts';

/** Generous limit for slow CI machines. */
const SLOW_MACHINE_TIMEOUT_MS = 60_000;
const GHOST_KEY = 'ninelives.ghost.v1:playground';

interface Debug {
  started: () => boolean;
  simTimeS: () => number;
  resultsOpen: () => boolean;
  ghost: () => { loaded: boolean; visible: boolean; position: number[] };
  ghostSamples: () => number;
  ghostEnabled: () => boolean;
  ghostScope: () => string;
  ghostTainted: () => boolean;
}

/** Calls a debug hook by name, or returns `fallback` before the game is up. */
async function debugCall<K extends keyof Debug>(
  page: Page,
  name: K,
  fallback: ReturnType<Debug[K]>,
): Promise<ReturnType<Debug[K]>> {
  const value = await page.evaluate((hook) => {
    const debug = (window as unknown as { ninelivesDebug?: Record<string, () => unknown> })
      .ninelivesDebug;
    return debug?.[hook]?.();
  }, name);
  return (value ?? fallback) as ReturnType<Debug[K]>;
}

/** One field of the ghost hook's state. */
async function ghostField<F extends 'loaded' | 'visible'>(
  page: Page,
  field: F,
  fallback: boolean,
): Promise<boolean> {
  return (await ghostState(page))?.[field] ?? fallback;
}

/** The ghost hook's state, or null before the game is up. */
async function ghostState(page: Page): Promise<ReturnType<Debug['ghost']> | null> {
  return debugCall(page, 'ghost', null as unknown as ReturnType<Debug['ghost']>);
}

async function waitForHook(page: Page): Promise<void> {
  await expect
    .poll(() => debugCall(page, 'simTimeS', Number.NaN), {
      timeout: SLOW_MACHINE_TIMEOUT_MS,
    })
    .toBeGreaterThanOrEqual(0);
}

/** A ghost run of `samples` points along +x, 10 Hz, upright. */
function ghostRun(samples: number): string {
  const poses: number[] = [];
  for (let index = 0; index < samples; index += 1) {
    poses.push(index * 0.5, 1, 4, 0, 0, 0, 1);
  }
  return JSON.stringify({
    version: 1,
    hz: 10,
    saves: 2,
    durationS: (samples - 1) / 10,
    poses,
  });
}

test('the title waits for Play, and the clock does not run behind it', async ({ page }) => {
  const consoleErrors = collectConsoleErrors(page);
  await page.goto('/');
  await waitForHook(page);

  await expect(page.locator('#title')).toBeVisible();
  await expect(page.locator('#title-watch')).toHaveAttribute('href', /viewer\.html/);
  expect(await debugCall(page, 'started', true)).toBe(false);
  expect(await debugCall(page, 'simTimeS', -1)).toBe(0);

  await expect(page.locator('#title-help')).toBeHidden();
  await page.locator('#title-how').click();
  await expect(page.locator('#title-help')).toBeVisible();
  await expect(page.locator('#title-how')).toHaveAttribute('aria-expanded', 'true');

  await page.locator('#title-play').click();
  await expect(page.locator('#title')).toBeHidden();
  await expect
    .poll(() => debugCall(page, 'simTimeS', 0), {
      timeout: SLOW_MACHINE_TIMEOUT_MS,
    })
    .toBeGreaterThan(0);
  expect(consoleErrors).toEqual([]);
});

test('Enter starts the game from the title, and ?play skips it', async ({ page }) => {
  await page.goto('/');
  await waitForHook(page);
  await page.keyboard.press('Enter');
  await expect(page.locator('#title')).toBeHidden();
  expect(await debugCall(page, 'started', false)).toBe(true);

  await page.goto('/?play=1');
  await waitForHook(page);
  await expect(page.locator('#title')).toBeHidden();
  expect(await debugCall(page, 'started', false)).toBe(true);
});

test('reduced motion turns the title animation off', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/');
  const seconds = await page
    .locator('#title')
    .evaluate((element) => parseFloat(getComputedStyle(element).animationDuration));
  expect(seconds).toBeLessThan(0.001);

  await page.emulateMedia({ reducedMotion: 'no-preference' });
  const normal = await page
    .locator('#title')
    .evaluate((element) => parseFloat(getComputedStyle(element).animationDuration));
  expect(normal).toBeGreaterThan(0.1);
});

test('Tab opens the results, pauses the game, and Tab or Esc closes them', async ({ page }) => {
  await page.goto('/?play=1');
  await waitForHook(page);
  await expect(page.locator('#results')).toBeHidden();

  await page.keyboard.press('Tab');
  await expect(page.locator('#results')).toBeVisible();
  const labels = await page.locator('#results-list dt').allTextContents();
  expect(labels).toEqual([
    'Saves',
    'Air used',
    'Peak g',
    'Stunts landed',
    'Time',
    'Best spike ever',
  ]);
  const pausedAtS = await debugCall(page, 'simTimeS', -1);
  await page.waitForTimeout(500);
  expect(await debugCall(page, 'simTimeS', -2)).toBe(pausedAtS);

  await page.keyboard.press('Escape');
  await expect(page.locator('#results')).toBeHidden();
  await page.keyboard.press('Tab');
  await expect(page.locator('#results')).toBeVisible();
  await page.keyboard.press('Tab');
  await expect(page.locator('#results')).toBeHidden();
});

test('the key list folds on a small screen and the Keys button opens it', async ({ page }) => {
  await page.setViewportSize({ width: 500, height: 800 });
  await page.goto('/?play=1');
  await waitForHook(page);

  await expect(page.locator('#hud-keys-toggle')).toBeVisible();
  await expect(page.locator('#hud-keys')).toBeHidden();
  await page.locator('#hud-keys-toggle').click();
  await expect(page.locator('#hud-keys')).toBeVisible();
  await expect(page.locator('#hud-keys')).toContainText('results');
  await expect(page.locator('#hud-keys-toggle')).toHaveAttribute('aria-expanded', 'true');
  await page.locator('#hud-keys-toggle').click();
  await expect(page.locator('#hud-keys')).toBeHidden();
});

test('the key list stays open on a wide screen, with no Keys button', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 800 });
  await page.goto('/?play=1');
  await waitForHook(page);
  await expect(page.locator('#hud-keys')).toBeVisible();
  await expect(page.locator('#hud-keys-toggle')).toBeHidden();
  await expect(page.locator('#hud-keys')).toContainText('barrel roll');
});

test('a stored ghost replays, and this run is recorded and stored on page hide', async ({
  page,
}) => {
  const consoleErrors = collectConsoleErrors(page);
  await page.addInitScript(
    ([key, value]) => {
      window.localStorage.setItem(key as string, value as string);
    },
    [GHOST_KEY, ghostRun(100)],
  );
  await page.goto('/?play=1');
  await waitForHook(page);

  await expect
    .poll(() => ghostField(page, 'loaded', false), {
      timeout: SLOW_MACHINE_TIMEOUT_MS,
    })
    .toBe(true);
  await expect
    .poll(() => ghostField(page, 'visible', false), {
      timeout: SLOW_MACHINE_TIMEOUT_MS,
    })
    .toBe(true);

  // The run records at 10 Hz of sim time; two samples are enough to store.
  await expect
    .poll(() => debugCall(page, 'ghostSamples', 0), {
      timeout: SLOW_MACHINE_TIMEOUT_MS,
    })
    .toBeGreaterThanOrEqual(2);
  // A stored run with 2 saves only gives way to a better one, so this short run keeps it.
  await page.evaluate(() => window.dispatchEvent(new Event('pagehide')));
  const stored = await page.evaluate((key) => window.localStorage.getItem(key), GHOST_KEY);
  expect(JSON.parse(stored ?? '{}').saves).toBe(2);
  expect(consoleErrors).toEqual([]);
});

test('a first run is stored on page hide when no ghost exists yet', async ({ page }) => {
  await page.goto('/?play=1');
  await waitForHook(page);
  expect(await ghostField(page, 'loaded', true)).toBe(false);
  await expect
    .poll(() => debugCall(page, 'ghostSamples', 0), {
      timeout: SLOW_MACHINE_TIMEOUT_MS,
    })
    .toBeGreaterThanOrEqual(2);
  await page.evaluate(() => window.dispatchEvent(new Event('pagehide')));
  const stored = await page.evaluate((key) => window.localStorage.getItem(key), GHOST_KEY);
  expect(JSON.parse(stored ?? '{}').version).toBe(1);
});

test('G hides the ghost and brings it back', async ({ page }) => {
  await page.goto('/?play=1');
  await waitForHook(page);
  expect(await debugCall(page, 'ghostEnabled', false)).toBe(true);
  await page.keyboard.press('KeyG');
  expect(await debugCall(page, 'ghostEnabled', true)).toBe(false);
  await page.keyboard.press('KeyG');
  expect(await debugCall(page, 'ghostEnabled', false)).toBe(true);
});

test('a broken stored ghost is ignored', async ({ page }) => {
  const consoleErrors = collectConsoleErrors(page);
  await page.addInitScript(() => {
    window.localStorage.setItem('ninelives.ghost.v1', '{"version":1,"poses":"x"}');
  });
  await page.goto('/?play=1');
  await waitForHook(page);
  expect(await ghostField(page, 'loaded', true)).toBe(false);
  expect(consoleErrors).toEqual([]);
});

test('no stunt-ready hint shows on the ground, and the g peak has a raw-spike tooltip', async ({
  page,
}) => {
  await page.goto('/?play=1');
  await waitForHook(page);
  await expect(page.locator('#hud-stunt-ready')).toBeHidden();
  await expect(page.locator('#hud-g-peak')).toHaveAttribute('title', /raw contact spike/);
});

test('Tab moves focus on the title and the dialogs announce themselves', async ({ page }) => {
  await page.goto('/');
  await waitForHook(page);
  for (const id of ['title', 'results', 'game-over']) {
    await expect(page.locator(`#${id}`)).toHaveAttribute('role', 'dialog');
    await expect(page.locator(`#${id}`)).toHaveAttribute('aria-modal', 'true');
  }
  // Focus starts on Play; Tab is not swallowed, so it reaches the next control.
  await expect(page.locator('#title-play')).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(page.locator('#title-how')).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(page.locator('#title-watch')).toBeFocused();
  await page.locator('#title-play').focus();
  await page.keyboard.press('Enter');
  await expect(page.locator('#title')).toBeHidden();
  await expect(page.locator('#title-play')).not.toBeFocused();
});

test('the results dialog takes focus and gives it back', async ({ page }) => {
  await page.goto('/?play=1');
  await waitForHook(page);
  await page.locator('#hud-keys-toggle').evaluate((element) => element.focus());
  await page.keyboard.press('Tab');
  await expect(page.locator('#results')).toBeVisible();
  await expect(page.locator('#results')).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.locator('#results')).toBeHidden();
  await expect(page.locator('#results')).not.toBeFocused();
});

test('a drop test is not recorded, and ghosts are kept per course', async ({ page }) => {
  await page.goto('/?play=1');
  await waitForHook(page);
  expect(await debugCall(page, 'ghostScope', '')).toBe('playground');
  await expect
    .poll(() => debugCall(page, 'ghostSamples', 0), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThanOrEqual(2);

  await page.keyboard.press('KeyX');
  expect(await debugCall(page, 'ghostTainted', false)).toBe(true);
  await page.evaluate(() => window.dispatchEvent(new Event('pagehide')));
  expect(await page.evaluate((key) => window.localStorage.getItem(key), GHOST_KEY)).toBeNull();

  // Another course has its own scope, and the playground's stored ghost does not follow.
  await page.evaluate(
    (json) => {
      const hook = (window as unknown as { ninelivesDebug: { loadCourse: (j: string) => void } })
        .ninelivesDebug;
      hook.loadCourse(json);
    },
    JSON.stringify({
      version: 1,
      name: 'scope',
      start: { pos: [0, 0.7, 0], yaw: 0 },
      bounds: { min: [-50, -20, -50], max: [600, 60, 50] },
      pieces: [],
    }),
  );
  expect(await debugCall(page, 'ghostScope', '')).toMatch(/^course-/);
  expect(await debugCall(page, 'ghostTainted', true)).toBe(false);
});
