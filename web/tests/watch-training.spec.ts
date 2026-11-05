// The way from the game to the training viewer (SPEC §11.6): a corner link and a pause-menu
// item both open the viewer, and the viewer links back to the game.
import { expect, test } from '@playwright/test';

/** Generous limit for slow CI machines. */
const SLOW_MACHINE_TIMEOUT_MS = 60_000;

test('the corner link opens the training viewer and its back link returns', async ({ page }) => {
  await page.goto('/?play=1');
  const watch = page.locator('#hud-watch');
  await expect(watch).toBeVisible({ timeout: SLOW_MACHINE_TIMEOUT_MS });

  await watch.click();
  await expect(page).toHaveURL(/viewer\.html$/);
  await expect(page.locator('#viewer-status')).toBeVisible();

  await page.getByRole('link', { name: 'Back to the game' }).click();
  await expect(page.locator('#hud-watch')).toBeVisible({ timeout: SLOW_MACHINE_TIMEOUT_MS });
});

test('the pause menu offers the training viewer', async ({ page }) => {
  await page.goto('/?play=1');
  await expect(page.locator('#hud-hearts .heart').first()).toBeVisible({
    timeout: SLOW_MACHINE_TIMEOUT_MS,
  });

  await page.keyboard.press('Escape');
  const pauseLink = page.locator('#paused').getByRole('link', { name: 'Watch AI training' });
  await expect(pauseLink).toBeVisible();

  await pauseLink.click();
  await expect(page).toHaveURL(/viewer\.html$/);
});
