// Smoke test (SPEC §12): the page loads, holding the up arrow drives the car forward, and
// nothing logs an error.
//
// It waits on *simulated* time, not wall-clock time: on a slow machine with software
// graphics the page draws few frames, and the game runs at most 8 physics ticks per
// frame (SPEC §11.2), so 3 real seconds can be much less than 3 simulated seconds.
import { expect, test, type Page } from '@playwright/test';

import { collectConsoleErrors } from './console-errors.ts';

/** Seconds of simulated full throttle; about 20 m of driving at the car's acceleration. */
const THROTTLE_SIM_S = 2;
/** Generous limit for slow CI machines. */
const SLOW_MACHINE_TIMEOUT_MS = 60_000;

interface DebugHook {
  chassisPose: () => number[];
  simTimeS: () => number;
  controllerCode: () => number;
}

/** The wasm `ControllerCode.Pd` value: PD alone. */
const CONTROLLER_CODE_PD = 2;
/** The wasm `ControllerCode.HybridPd` value: Hybrid, with PD in charge. */
const CONTROLLER_CODE_HYBRID_PD = 5;
/** Sim seconds to wait before checking the game has not switched controller by itself. */
const SETTLE_SIM_S = 3;

/** The sim's current controller code (read straight from the game, not the HUD text,
 * which only updates on the next drawn frame). */
async function readControllerCode(page: Page): Promise<number> {
  return page.evaluate(() => {
    const hook = (window as unknown as { ninelivesDebug?: DebugHook }).ninelivesDebug;
    return hook ? hook.controllerCode() : -1;
  });
}

/** Reads one value through the page's debug hook, or NaN before the game has started. */
async function readDebug(page: Page, what: 'x' | 'time'): Promise<number> {
  return page.evaluate((key) => {
    const hook = (window as unknown as { ninelivesDebug?: DebugHook }).ninelivesDebug;
    if (!hook) {
      return Number.NaN;
    }
    return key === 'x' ? (hook.chassisPose()[0] ?? 0) : hook.simTimeS();
  }, what);
}

test('holding the up arrow drives the car forward without console errors', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);

  await page.goto('/?play=1');
  await expect.poll(() => readDebug(page, 'x'), { timeout: SLOW_MACHINE_TIMEOUT_MS }).not.toBeNaN();
  const startX = await readDebug(page, 'x');
  const startTimeS = await readDebug(page, 'time');

  await page.keyboard.down('ArrowUp');
  await expect
    .poll(() => readDebug(page, 'time'), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThan(startTimeS + THROTTLE_SIM_S);
  await page.keyboard.up('ArrowUp');

  expect(await readDebug(page, 'x')).toBeGreaterThan(startX + 5);
  await expect(page.locator('#hud-speed-value')).not.toHaveText('0');
  expect(consoleErrors).toEqual([]);
});

test('the game switches to Hybrid by default once the policy has loaded', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  await page.goto('/?play=1');

  await expect
    .poll(() => readControllerCode(page), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBe(CONTROLLER_CODE_HYBRID_PD);
  await expect(page.locator('#hud-status')).toHaveText('Limited air');
});

test('?controller=pd keeps the game on PD alone', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  await page.goto('/?play=1&controller=pd');
  await expect
    .poll(() => readDebug(page, 'time'), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .not.toBeNaN();
  const startTimeS = await readDebug(page, 'time');

  await expect
    .poll(() => readDebug(page, 'time'), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThan(startTimeS + SETTLE_SIM_S);

  expect(await readControllerCode(page)).toBe(CONTROLLER_CODE_PD);
  await expect(page.locator('#hud-status')).toHaveText('Limited air');
});

test('?controller=hybrid switches to Hybrid once the policy has loaded', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  await page.goto('/?play=1&controller=hybrid');

  await expect
    .poll(() => readControllerCode(page), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBe(CONTROLLER_CODE_HYBRID_PD);
  await expect(page.locator('#hud-status')).toHaveText('Limited air');
});

test('a drop test runs clean under the Hybrid', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);
  await page.goto('/?play=1&controller=hybrid');
  await expect
    .poll(() => readControllerCode(page), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBe(CONTROLLER_CODE_HYBRID_PD);

  await page.keyboard.press('KeyX');
  const dropTimeS = await readDebug(page, 'time');
  await expect
    .poll(() => readDebug(page, 'time'), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThan(dropTimeS + 2);
  expect(consoleErrors).toEqual([]);
});
