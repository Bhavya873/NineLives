// UI polish (SPEC M8): the title screen, reduced motion, the folding key card and the
// Scene-theme look. The CI runner draws about 3 frames a second, so nothing here
// relies on driving in real time: tests read the sim's own clock or use the debug hooks.
import { expect, test, type Page } from '@playwright/test';

import { collectConsoleErrors } from './console-errors.ts';

/** Generous limit for slow CI machines. */
const SLOW_MACHINE_TIMEOUT_MS = 60_000;

interface Debug {
  started: () => boolean;
  simTimeS: () => number;
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

async function waitForHook(page: Page): Promise<void> {
  await expect
    .poll(() => debugCall(page, 'simTimeS', Number.NaN), {
      timeout: SLOW_MACHINE_TIMEOUT_MS,
    })
    .toBeGreaterThanOrEqual(0);
}

test('the title waits for Play, and the clock does not run behind it', async ({ page }) => {
  const consoleErrors = collectConsoleErrors(page);
  await page.goto('/');
  await waitForHook(page);

  await expect(page.locator('#title')).toBeVisible();
  await expect(page.locator('#title-watch')).toHaveAttribute('href', /viewer\.html/);
  expect(await debugCall(page, 'started', true)).toBe(false);
  expect(await debugCall(page, 'simTimeS', -1)).toBe(0);

  // one line of copy, Play and the viewer link; no How it works block
  await expect(page.locator('#title')).toContainText(
    'Jump, flip, land on your wheels. Nine tries.',
  );
  await expect(page.locator('#title-how')).toHaveCount(0);
  await expect(page.locator('#title-help')).toHaveCount(0);

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

test('the key card starts folded on a small screen and its Keys button opens it', async ({
  page,
}) => {
  await page.setViewportSize({ width: 500, height: 800 });
  await page.goto('/?play=1');
  await waitForHook(page);

  await expect(page.locator('#hud-keys-toggle')).toBeVisible();
  await expect(page.locator('#hud-keys')).toBeHidden();
  await page.locator('#hud-keys-toggle').click();
  await expect(page.locator('#hud-keys')).toBeVisible();
  await expect(page.locator('#hud-keys')).toContainText('barrel roll');
  await expect(page.locator('#hud-keys-toggle')).toHaveAttribute('aria-expanded', 'true');
  await page.locator('#hud-keys-toggle').click();
  await expect(page.locator('#hud-keys')).toBeHidden();
});

test('the key card starts open on a wide screen and can be folded', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 800 });
  await page.goto('/?play=1');
  await waitForHook(page);
  await expect(page.locator('#hud-keys')).toBeVisible();
  await expect(page.locator('#hud-keys')).toContainText('barrel roll');
  // The ghost and the results screen are gone, so their keys are not listed.
  await expect(page.locator('#hud-keys')).not.toContainText(/ghost|results/i);
  // A S roll, D F flip, arrows drive; R respawns but is not listed
  await expect(page.locator('#hud-keys')).toContainText('A S');
  await expect(page.locator('#hud-keys')).toContainText('D F');
  await expect(page.locator('#hud-keys')).not.toContainText(/W S|Q \/ E|respawn/);
  await page.locator('#hud-keys-toggle').click();
  await expect(page.locator('#hud-keys')).toBeHidden();
  await expect(page.locator('#hud-keys-toggle')).toHaveAttribute('aria-expanded', 'false');
});

test('Tab is not swallowed in a run, and there is no results screen or ghost', async ({ page }) => {
  await page.goto('/?play=1');
  await waitForHook(page);
  await page.keyboard.press('Tab');
  await page.keyboard.press('KeyG');
  expect(await page.locator('#results').count()).toBe(0);
  expect(await page.locator('#game-over-results').count()).toBe(0);
  await expect(page.locator('#paused')).toBeHidden();
  // The sim keeps running: neither key pauses it.
  const before = await debugCall(page, 'simTimeS', -1);
  await expect
    .poll(() => debugCall(page, 'simTimeS', -1), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThan(before);
});

test('the bottom-centre tiles stay hidden while the car sits on the ground', async ({ page }) => {
  await page.goto('/?play=1');
  await waitForHook(page);
  await expect(page.locator('#hud-stunt-ready')).toHaveCount(0);
  await expect(page.locator('#hud-g-record')).toHaveCount(0);
  await expect(page.locator('#hud-g-tile')).toBeHidden();
  await expect(page.locator('#hud-nozzle-tile')).toBeHidden();
  await expect(page.locator('#hud-g-peak')).toHaveAttribute('title', /raw contact spike/);
});

test('Tab moves focus on the title and the dialogs announce themselves', async ({ page }) => {
  await page.goto('/');
  await waitForHook(page);
  for (const id of ['title', 'game-over']) {
    await expect(page.locator(`#${id}`)).toHaveAttribute('role', 'dialog');
    await expect(page.locator(`#${id}`)).toHaveAttribute('aria-modal', 'true');
  }
  // Focus starts on Play; Tab is not swallowed, so it reaches the next control.
  await expect(page.locator('#title-play')).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(page.locator('#title-watch')).toBeFocused();
  await page.locator('#title-play').focus();
  await page.keyboard.press('Enter');
  await expect(page.locator('#title')).toBeHidden();
  await expect(page.locator('#title-play')).not.toBeFocused();
});

test('the Barlow fonts load and the tiles are flat slate', async ({ page }) => {
  await page.goto('/?play=1');
  await waitForHook(page);
  const fonts = await page.evaluate(async () => {
    await Promise.all([
      document.fonts.load('700 20px "Barlow Condensed"'),
      document.fonts.load('600 16px "Barlow"'),
    ]);
    return [
      document.fonts.check('700 20px "Barlow Condensed"'),
      document.fonts.check('600 16px "Barlow"'),
    ];
  });
  expect(fonts).toEqual([true, true]);
  const dash = await page.locator('#hud-dash').evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      background: style.backgroundColor,
      filter: style.backdropFilter,
      shadow: style.boxShadow,
    };
  });
  expect(dash.background).toBe('rgb(31, 43, 56)');
  expect(dash.filter).toBe('none');
  expect(dash.shadow).toBe('none');
});
