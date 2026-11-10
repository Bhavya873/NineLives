// The way from the game to the training viewer (SPEC §11.6): a title link and a pause-menu
// item both open the viewer, and the viewer links back to the game.
import { expect, test } from '@playwright/test';

/** Generous limit for slow CI machines. */
const SLOW_MACHINE_TIMEOUT_MS = 60_000;

test('the title link opens the training viewer and its back link returns', async ({ page }) => {
  await page.goto('/');
  const watch = page.locator('#title-watch');
  await expect(watch).toBeVisible({ timeout: SLOW_MACHINE_TIMEOUT_MS });
  // the driving HUD has no viewer link of its own any more
  await expect(page.locator('#hud-watch')).toHaveCount(0);

  await watch.click();
  await expect(page).toHaveURL(/viewer\.html$/);
  await expect(page.locator('#viewer-status')).toBeVisible();

  await page.getByRole('link', { name: 'back to game' }).click();
  await expect(page.locator('#hud-hearts .heart').first()).toBeVisible({
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });
});

test('the pause menu offers the training viewer', async ({ page }) => {
  await page.goto('/?play=1');
  await expect(page.locator('#hud-hearts .heart').first()).toBeVisible({
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });

  await page.keyboard.press('Escape');
  const pauseLink = page.locator('#paused').getByRole('link', { name: 'Watch the AI train' });
  await expect(pauseLink).toBeVisible();

  await pauseLink.click();
  await expect(page).toHaveURL(/viewer\.html$/);
});

/** Opens the pause card and follows its link to the training viewer. */
async function goToViewerFromPause(page: import('@playwright/test').Page): Promise<void> {
  await page.keyboard.press('Escape');
  await page.locator('#paused').getByRole('link', { name: 'Watch the AI train' }).click();
}

/** Calls a debug hook and returns its value. */
async function hook(page: import('@playwright/test').Page, name: string): Promise<unknown> {
  return page.evaluate((hookName) => {
    const debug = (window as unknown as { ninelivesDebug?: Record<string, () => unknown> })
      .ninelivesDebug;
    return debug?.[hookName]?.();
  }, name);
}

test('the back button returns to the game with the same lives and no title', async ({ page }) => {
  await page.goto('/?play=1&testhooks=1');
  await expect.poll(() => hook(page, 'heartsLeft'), { timeout: SLOW_MACHINE_TIMEOUT_MS }).toBe(9);
  await hook(page, 'loseLife');
  await expect(page.locator('#hud-hearts .heart.lost')).toHaveCount(1);

  await goToViewerFromPause(page);
  await expect(page).toHaveURL(/viewer\.html$/);
  // the run is kept for the trip back, saves included
  const kept = await page.evaluate(() => sessionStorage.getItem('ninelives.run'));
  expect(JSON.parse(kept ?? '{}')).toEqual({ lostLives: 1, saves: 0 });
  await page.locator('#viewer-back').click();

  await expect(page).toHaveURL(/play=1/);
  await expect(page.locator('#title')).toBeHidden();
  await expect(page.locator('#hud-hearts .heart.lost')).toHaveCount(1, {
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });
});

test('a fresh tab or a plain ?play=1 starts with nine lives', async ({ page, context }) => {
  test.setTimeout(3 * SLOW_MACHINE_TIMEOUT_MS);
  await page.goto('/?play=1&testhooks=1');
  await expect.poll(() => hook(page, 'heartsLeft'), { timeout: SLOW_MACHINE_TIMEOUT_MS }).toBe(9);
  await hook(page, 'loseLife');
  await expect(page.locator('#hud-hearts .heart.lost')).toHaveCount(1);
  await goToViewerFromPause(page);
  await expect(page).toHaveURL(/viewer\.html$/);
  // close the viewer tab: five stage workers would slow the new tab's boot
  await page.close();

  const fresh = await context.newPage();
  await fresh.goto('/?play=1&resume=1');
  // wait for a booted game, not just for hearts: the debug wasm can take a while to load
  await expect.poll(() => hook(fresh, 'started'), { timeout: SLOW_MACHINE_TIMEOUT_MS }).toBe(true);
  await expect.poll(() => hook(fresh, 'heartsLeft')).toBe(9);
  await expect(fresh.locator('#hud-hearts .heart')).toHaveCount(9);
  await expect(fresh.locator('#hud-hearts .heart.lost')).toHaveCount(0);
  // a new navigation starts only once the game is up, so the first one is not aborted
  await fresh.goto('/');
  await expect(fresh.locator('#title')).toBeVisible({ timeout: SLOW_MACHINE_TIMEOUT_MS });
});
