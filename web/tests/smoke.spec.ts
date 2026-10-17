// Smoke test (SPEC §12): the page loads, holding W drives the car forward, and
// nothing logs an error.
//
// It waits on *simulated* time, not wall-clock time: on a slow machine with software
// graphics the page draws few frames, and the game runs at most 8 physics ticks per
// frame (SPEC §11.2), so 3 real seconds can be much less than 3 simulated seconds.
import { expect, test, type Page } from '@playwright/test';

/** Seconds of simulated full throttle; about 20 m of driving at the car's acceleration. */
const THROTTLE_SIM_S = 2;
/** Generous limit for slow CI machines. */
const SLOW_MACHINE_TIMEOUT_MS = 60_000;

interface DebugHook {
  chassisPose: () => number[];
  simTimeS: () => number;
  controllerCode: () => number;
}

/** The wasm `ControllerCode.Ai` value (crates/nl-wasm). */
const CONTROLLER_CODE_AI = 3;

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

/** Collects the text of every console error the page logs from now on. */
function collectConsoleErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') {
      errors.push(message.text());
    }
  });
  return errors;
}

test('holding W drives the car forward without console errors', async ({ page }) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);

  await page.goto('/');
  await expect.poll(() => readDebug(page, 'x'), { timeout: SLOW_MACHINE_TIMEOUT_MS }).not.toBeNaN();
  const startX = await readDebug(page, 'x');
  const startTimeS = await readDebug(page, 'time');

  await page.keyboard.down('KeyW');
  await expect
    .poll(() => readDebug(page, 'time'), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThan(startTimeS + THROTTLE_SIM_S);
  await page.keyboard.up('KeyW');

  expect(await readDebug(page, 'x')).toBeGreaterThan(startX + 5);
  await expect(page.locator('#hud-speed')).not.toHaveText('0 km/h');
  expect(consoleErrors).toEqual([]);
});

test('T reaches the AI once the policy has loaded, and a drop test runs clean', async ({
  page,
}) => {
  test.setTimeout(2 * SLOW_MACHINE_TIMEOUT_MS);
  const consoleErrors = collectConsoleErrors(page);
  await page.goto('/');
  await expect.poll(() => readDebug(page, 'x'), { timeout: SLOW_MACHINE_TIMEOUT_MS }).not.toBeNaN();

  // Off → PD → AI (SPEC §11.3). The policy loads in the background, so keep cycling
  // until the sim reports the AI; never press T once it does.
  await expect
    .poll(
      async () => {
        if ((await readControllerCode(page)) !== CONTROLLER_CODE_AI) {
          await page.keyboard.press('KeyT');
        }
        return readControllerCode(page);
      },
      { timeout: SLOW_MACHINE_TIMEOUT_MS },
    )
    .toBe(CONTROLLER_CODE_AI);
  await expect(page.locator('#hud-status')).toContainText('Controller: AI');

  await page.keyboard.press('KeyX');
  const dropTimeS = await readDebug(page, 'time');
  await expect
    .poll(() => readDebug(page, 'time'), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThan(dropTimeS + 2);
  expect(consoleErrors).toEqual([]);
});
