// Training viewer on short landscape screens (a phone held sideways): the side column takes its
// own strip, the stage panels tile the rest, and nothing covers a panel title. Desktop keeps the
// chart and the board.
import { type Page, expect, test } from '@playwright/test';

/** Time for the page to build its panels and titles on a slow machine. */
const PAGE_READY_TIMEOUT_MS = 90_000;
/** Smallest comfortable touch target, in CSS pixels. */
const MIN_TOUCH_TARGET_PX = 44;

interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A visible element's box and label, as the page reports them. */
interface Reading {
  box: Box;
  text: string;
}

/** The boxes of every visible element matching `selector`, read in one round trip (the page is
 * busy flying cars, so one call per element would be slow). */
function readVisible(page: Page, selector: string): Promise<Reading[]> {
  return page.evaluate((sel) => {
    const readings: Reading[] = [];
    for (const element of document.querySelectorAll(sel)) {
      const rect = element.getBoundingClientRect();
      if (rect.width > 0 && rect.height > 0) {
        readings.push({
          box: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
          text: (element.textContent ?? '').trim(),
        });
      }
    }
    return readings;
  }, selector);
}

function intersects(a: Box, b: Box): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

function isInside(box: Box, size: { width: number; height: number }): boolean {
  const slack = 0.5;
  return (
    box.x >= -slack &&
    box.y >= -slack &&
    box.x + box.width <= size.width + slack &&
    box.y + box.height <= size.height + slack
  );
}

async function openViewer(page: Page): Promise<void> {
  await page.goto('/viewer.html');
  await expect(page.locator('#viewer-status')).toContainText('8 checkpoints', {
    timeout: PAGE_READY_TIMEOUT_MS,
  });
  await expect(page.locator('.panel-title:visible')).toHaveCount(5);
}

for (const size of [
  { width: 844, height: 390 },
  { width: 667, height: 375 },
]) {
  test.describe(`${size.width}x${size.height} touch`, () => {
    test.use({ viewport: size, hasTouch: true, isMobile: true, deviceScaleFactor: 2 });

    test('the compact column and the panels do not overlap', async ({ page }) => {
      test.setTimeout(2 * PAGE_READY_TIMEOUT_MS);
      await openViewer(page);
      const [side] = await readVisible(page, '#viewer-side');
      const titles = await readVisible(page, '.panel-title');
      expect(titles).toHaveLength(5);
      titles.forEach(({ box }, i) => {
        expect(intersects(side.box, box), `panel title ${i} under the side column`).toBe(false);
        expect(isInside(box, size), `panel title ${i} outside the viewport`).toBe(true);
      });
    });

    test('the column keeps status, back link and usable tabs, and hides chart and board', async ({
      page,
    }) => {
      test.setTimeout(2 * PAGE_READY_TIMEOUT_MS);
      await openViewer(page);
      await expect(page.locator('#viewer-status')).toBeVisible();
      await expect(page.getByRole('link', { name: /back to game/i })).toBeVisible();
      await expect(page.locator('#camera-toggle')).toBeVisible();
      await expect(page.locator('#curve-panel')).toBeHidden();
      await expect(page.locator('#viewer-board')).toBeHidden();

      const tabs = await readVisible(page, '#stage-tabs button');
      expect(tabs.length).toBeGreaterThan(1);
      expect(await page.locator('#stage-tabs button').count()).toBe(tabs.length);
      tabs.forEach(({ box, text }, i) => {
        expect(text.length, `tab ${i} has no label`).toBeGreaterThan(0);
        expect(isInside(box, size), `tab ${i} outside the viewport`).toBe(true);
        expect(box.height).toBeGreaterThanOrEqual(MIN_TOUCH_TARGET_PX);
      });
    });

    test('the page does not scroll sideways', async ({ page }) => {
      test.setTimeout(2 * PAGE_READY_TIMEOUT_MS);
      await openViewer(page);
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(overflow).toBeLessThanOrEqual(0);
    });
  });
}

test.describe('1280x720 desktop', () => {
  test.use({ viewport: { width: 1280, height: 720 } });

  test('the chart and the board stay visible', async ({ page }) => {
    test.setTimeout(2 * PAGE_READY_TIMEOUT_MS);
    await openViewer(page);
    await expect(page.locator('#curve-panel')).toBeVisible();
    await expect(page.locator('#viewer-board')).toBeVisible();
  });
});
