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

test('the key card starts folded on every screen size and the chip opens it', async ({ page }) => {
  // two page boots in one test: give a slow machine room
  test.setTimeout(SLOW_MACHINE_TIMEOUT_MS);
  for (const width of [500, 1200]) {
    await page.setViewportSize({ width, height: 800 });
    await page.goto('/?play=1');
    await waitForHook(page);
    await expect(page.locator('#hud-keys-toggle')).toBeVisible();
    await expect(page.locator('#hud-keys-toggle')).toContainText('H');
    await expect(page.locator('#hud-keys-toggle')).toHaveAttribute('aria-expanded', 'false');
    await expect(page.locator('#hud-keys')).toBeHidden();
  }

  await page.locator('#hud-keys-toggle').click();
  await expect(page.locator('#hud-keys')).toBeVisible();
  await expect(page.locator('#hud-keys')).toContainText('barrel roll');
  await expect(page.locator('#hud-keys-toggle')).toHaveAttribute('aria-expanded', 'true');
  await page.locator('#hud-keys-toggle').click();
  await expect(page.locator('#hud-keys')).toBeHidden();
});

test('H unfolds and folds the key card, which lists the keys', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 800 });
  await page.goto('/?play=1');
  await waitForHook(page);
  await page.keyboard.press('KeyH');
  await expect(page.locator('#hud-keys')).toBeVisible();
  await expect(page.locator('#hud-keys')).toContainText('pause');
  await expect(page.locator('#hud-keys')).toContainText('barrel roll');
  // The ghost and the results screen are gone, so their keys are not listed.
  await expect(page.locator('#hud-keys')).not.toContainText(/ghost|results/i);
  // A S roll, D F flip, arrows drive; R respawns but is not listed
  await expect(page.locator('#hud-keys')).toContainText('A S');
  await expect(page.locator('#hud-keys')).toContainText('D F');
  await expect(page.locator('#hud-keys')).not.toContainText(/W S|Q \/ E|respawn/);
  await expect(page.locator('#hud-keys-toggle')).toHaveAttribute('aria-expanded', 'true');
  await page.keyboard.press('KeyH');
  await expect(page.locator('#hud-keys')).toBeHidden();
  await expect(page.locator('#hud-keys-toggle')).toHaveAttribute('aria-expanded', 'false');
});

test('the driving HUD has no viewer link; the saves counter and message stack have places', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto('/?play=1');
  await waitForHook(page);
  await expect(page.locator('#hud-watch')).toHaveCount(0);
  await expect(page.locator('#hud-saves')).toHaveText('0 saves');

  // The message stack sits in the top third, off the road. It may be empty, so look at its box.
  const messages = await page.locator('#hud-messages').evaluate((element) => {
    const box = element.getBoundingClientRect();
    return { top: box.top, innerHeight: window.innerHeight };
  });
  expect(messages.top).toBeLessThan(messages.innerHeight / 3);

  const dash = await page.locator('#hud-dash').boundingBox();
  expect(dash).not.toBeNull();
  const dashCentre = (dash?.x ?? 0) + (dash?.width ?? 0) / 2;
  expect(Math.abs(dashCentre - 1280 / 2)).toBeLessThanOrEqual(2);
});

test('the nozzle map shows when a nozzle fires, and the dash does not move', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto('/?play=1');
  await waitForHook(page);
  await expect(page.locator('#hud-nozzle-tile')).toBeHidden();
  const before = await page.locator('#hud-dash').boundingBox();

  // X throws the car into a drop; the automatic thrusters fire to save it.
  await page.keyboard.press('KeyX');
  await expect(page.locator('#hud-nozzle-tile')).toBeVisible({ timeout: SLOW_MACHINE_TIMEOUT_MS });
  await expect(page.locator('#hud-nozzle-who')).toHaveText(/Thrusters|AI flying/);
  const after = await page.locator('#hud-dash').boundingBox();
  expect(after).toEqual(before);
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

test('the Barlow fonts load and the HUD panels are glass', async ({ page }) => {
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
    return { background: style.backgroundColor, filter: style.backdropFilter };
  });
  // Liquid glass: a blurred, see-through pane (alpha below 1).
  expect(dash.filter).not.toBe('none');
  const alpha = Number(/,\s*([\d.]+)\)$/.exec(dash.background)?.[1] ?? '1');
  expect(alpha).toBeLessThan(1);
});

test('reduced transparency gives the glass panels an opaque ink look', async ({ page }) => {
  // Playwright 1.64 has no reducedTransparency option, so ask Chromium directly.
  const session = await page.context().newCDPSession(page);
  await session.send('Emulation.setEmulatedMedia', {
    features: [{ name: 'prefers-reduced-transparency', value: 'reduce' }],
  });
  await page.goto('/?play=1');
  await waitForHook(page);
  const dash = await page.locator('#hud-dash').evaluate((element) => {
    const style = getComputedStyle(element);
    return { background: style.backgroundColor, filter: style.backdropFilter };
  });
  expect(dash.background).toBe('rgb(21, 25, 31)');
  expect(dash.filter).toBe('none');
});

test('Tab never lands behind the title or the pause card', async ({ page }) => {
  await page.goto('/');
  await waitForHook(page);
  const behind = async () =>
    page.evaluate(() => {
      const active = document.activeElement;
      const dialog = document.querySelector('#title:not([hidden]), #paused:not([hidden])');
      return !!active && active !== document.body && !!dialog && !dialog.contains(active);
    });
  for (let press = 0; press < 6; press += 1) {
    await page.keyboard.press('Tab');
    expect(await behind()).toBe(false);
  }
  // the page behind is inert while the title is up, and live again after Play
  await expect(page.locator('#hud-top-right')).toHaveJSProperty('inert', true);
  await page.locator('#title-play').click();
  await expect(page.locator('#hud-top-right')).toHaveJSProperty('inert', false);

  await page.keyboard.press('Escape');
  await expect(page.locator('#paused')).toBeVisible();
  await expect(page.locator('#hud-top-right')).toHaveJSProperty('inert', true);
  for (let press = 0; press < 4; press += 1) {
    await page.keyboard.press('Tab');
    expect(await behind()).toBe(false);
  }
  await page.keyboard.press('Escape');
  await expect(page.locator('#hud-top-right')).toHaveJSProperty('inert', false);
});
