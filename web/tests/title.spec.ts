// The title screen and the F3 overlay (SPEC §11.6, §11.7): the title shows on load,
// driving hides it and Esc brings it back, an AI pick made before the policy loads takes
// effect once it has, and F3 shows the frame-rate readout.
import { expect, test, type Page } from '@playwright/test';

import { collectConsoleErrors } from './console-errors.ts';

/** Generous limit for slow CI machines. */
const SLOW_MACHINE_TIMEOUT_MS = 60_000;
/** The wasm `ControllerCode.Ai` value (crates/nl-wasm). */
const CONTROLLER_CODE_AI = 3;

/** The sim's current controller code, or -1 before the game has started. */
async function readControllerCode(page: Page): Promise<number> {
  return page.evaluate(() => {
    const hook = (window as unknown as { ninelivesDebug?: { controllerCode: () => number } })
      .ninelivesDebug;
    return hook ? hook.controllerCode() : -1;
  });
}

/** Waits until the game has started (its debug hook is up). */
async function waitForGame(page: Page): Promise<void> {
  await expect
    .poll(() => readControllerCode(page), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .not.toBe(-1);
}

test('the title shows on load, driving hides it and Esc brings it back', async ({ page }) => {
  const consoleErrors = collectConsoleErrors(page);
  await page.goto('/');
  await waitForGame(page);
  const title = page.locator('#title-screen');
  await expect(title).toBeVisible();

  await page.keyboard.press('KeyW');
  await expect(title).toBeHidden();
  await page.keyboard.press('Escape');
  await expect(title).toBeVisible();
  await page.locator('#title-play').click();
  await expect(title).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test('an AI pick on the title screen holds once the policy has loaded', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  await page.goto('/');
  await waitForGame(page);
  const aiButton = page.locator('#title-screen button[data-controller="ai"]');

  await aiButton.click();

  await expect
    .poll(() => readControllerCode(page), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBe(CONTROLLER_CODE_AI);
  await expect(aiButton).toHaveAttribute('aria-pressed', 'true');
});

test('an AI pick is undone and greyed out when the policy fails to load', async ({ page }) => {
  // Hold the policy request until the AI pick is made, then fail it.
  let failPolicy = () => {};
  const pickMade = new Promise<void>((resolve) => (failPolicy = resolve));
  await page.route('**/policy*.json', async (route) => {
    await pickMade;
    await route.fulfill({ status: 404, body: '' });
  });
  await page.goto('/');
  await waitForGame(page);
  const aiButton = page.locator('#title-screen button[data-controller="ai"]');
  const pdButton = page.locator('#title-screen button[data-controller="pd"]');

  await aiButton.click();
  failPolicy();

  await expect(aiButton).toBeDisabled({ timeout: SLOW_MACHINE_TIMEOUT_MS });
  await expect(aiButton).toHaveAttribute('aria-pressed', 'false');
  await expect(pdButton).toHaveAttribute('aria-pressed', 'true');
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

test('the course and wind pickers start the Gauntlet and turn on the wind', async ({ page }) => {
  await page.goto('/');
  await waitForGame(page);

  await page.locator('#title-screen button[data-course="gauntlet"]').click();
  await page.locator('#title-screen button[data-wind="storm"]').click();

  await expect(page.locator('#hud-status')).toContainText('Checkpoint 0/');
  await expect(page.locator('#hud-wind')).toBeVisible({ timeout: SLOW_MACHINE_TIMEOUT_MS });
});

test('the keys panel shows when play starts and H hides it', async ({ page }) => {
  await page.goto('/');
  await waitForGame(page);
  const keys = page.locator('#hud-help');
  await expect(keys).toBeHidden();

  await page.locator('#title-play').click();
  await expect(keys).toBeVisible();
  await page.keyboard.press('KeyH');

  await expect(keys).toBeHidden();
});
