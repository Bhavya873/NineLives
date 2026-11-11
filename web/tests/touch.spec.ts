// Touch and phone layout (SPEC §11.9): the controls show only on touch devices, the floating
// stick drives, HOP works with the other thumb, the wind and air-mode readouts are buttons, the
// pause card has Drop test, a portrait phone gets the rotate card, swipes ask for stunts, and
// no HUD piece overlaps another. Touches are sent with Chromium's own touch events (CDP).
import { expect, test, type CDPSession, type Locator, type Page } from '@playwright/test';

import { collectConsoleErrors } from './console-errors.ts';

/** Generous limit for slow CI machines. */
const SLOW_MACHINE_TIMEOUT_MS = 60_000;

/** One finger on the screen. */
interface Finger {
  id: number;
  x: number;
  y: number;
}

/** Sends touches by finger, so two thumbs can be down at once. */
class Fingers {
  private readonly down = new Map<number, Finger>();
  private constructor(private readonly session: CDPSession) {}

  static async attach(page: Page): Promise<Fingers> {
    return new Fingers(await page.context().newCDPSession(page));
  }

  private async send(type: 'touchStart' | 'touchMove' | 'touchEnd'): Promise<void> {
    await this.session.send('Input.dispatchTouchEvent', {
      type,
      touchPoints: [...this.down.values()].map((f) => ({ x: f.x, y: f.y, id: f.id })),
    });
  }

  async press(id: number, x: number, y: number): Promise<void> {
    this.down.set(id, { id, x, y });
    await this.send('touchStart');
  }

  async move(id: number, x: number, y: number): Promise<void> {
    this.down.set(id, { id, x, y });
    await this.send('touchMove');
  }

  async lift(id: number): Promise<void> {
    const finger = this.down.get(id);
    if (!finger) {
      return;
    }
    // touchEnd names the fingers that lift; the others stay down.
    await this.session.send('Input.dispatchTouchEvent', {
      type: 'touchEnd',
      touchPoints: [{ x: finger.x, y: finger.y, id }],
    });
    this.down.delete(id);
  }

  /** A swipe from one point to another with one finger. */
  async swipe(id: number, from: [number, number], to: [number, number]): Promise<void> {
    await this.press(id, from[0], from[1]);
    await this.move(id, (from[0] + to[0]) / 2, (from[1] + to[1]) / 2);
    await this.move(id, to[0], to[1]);
    await this.lift(id);
  }
}

interface DebugHook {
  chassisPose: () => number[];
  simTimeS: () => number;
  stuntRequestCount: () => number;
}

async function debugValue(page: Page, what: 'y' | 'time' | 'stunts'): Promise<number> {
  return page.evaluate((key) => {
    const hook = (window as unknown as { ninelivesDebug?: DebugHook }).ninelivesDebug;
    if (!hook) {
      return Number.NaN;
    }
    if (key === 'y') {
      return hook.chassisPose()[1] ?? 0;
    }
    return key === 'time' ? hook.simTimeS() : hook.stuntRequestCount();
  }, what);
}

async function waitForGame(page: Page): Promise<void> {
  await expect
    .poll(() => debugValue(page, 'time'), { timeout: SLOW_MACHINE_TIMEOUT_MS })
    .toBeGreaterThan(0);
}

async function speedKmH(page: Page): Promise<number> {
  return Number((await page.locator('#hud-speed-value').textContent()) ?? '0');
}

/** The centre of an element, for placing a finger. */
async function centreOf(locator: Locator): Promise<[number, number]> {
  const box = await locator.boundingBox();
  expect(box).not.toBeNull();
  return [(box?.x ?? 0) + (box?.width ?? 0) / 2, (box?.y ?? 0) + (box?.height ?? 0) / 2];
}

test.describe('on a phone in landscape', () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 844, height: 390 } });

  test('the touch controls show and the key chip is hidden', async ({ page }) => {
    const consoleErrors = collectConsoleErrors(page);
    await page.goto('/?play=1');
    await waitForGame(page);
    for (const id of ['touch-stick', 'touch-hop', 'touch-boost', 'touch-pause']) {
      await expect(page.locator(`#${id}`)).toBeVisible();
    }
    await expect(page.locator('#hud-keys-toggle')).toBeHidden();
    await expect(page.locator('#rotate-card')).toBeHidden();
    for (const id of ['touch-hop', 'touch-boost', 'touch-pause']) {
      const box = await page.locator(`#${id}`).boundingBox();
      expect(Math.min(box?.width ?? 0, box?.height ?? 0)).toBeGreaterThanOrEqual(56);
    }
    expect(consoleErrors).toEqual([]);
  });

  test('dragging in the left zone moves the stick and the car drives, with HOP on the other thumb', async ({
    page,
  }) => {
    await page.goto('/?play=1');
    await waitForGame(page);
    const fingers = await Fingers.attach(page);
    const ring = page.locator('#touch-stick');
    const restBox = await ring.boundingBox();

    // Thumb 1 lands left of the rest place; the ring re-centres under it.
    await fingers.press(1, 200, 250);
    await fingers.move(1, 200, 200);
    await expect(ring).toHaveClass(/drag/);
    const movedBox = await ring.boundingBox();
    expect(Math.abs((movedBox?.x ?? 0) + (movedBox?.width ?? 0) / 2 - 200)).toBeLessThan(2);
    expect(movedBox?.x).not.toBe(restBox?.x);
    const knobY = await page
      .locator('#touch-stick-knob')
      .evaluate((knob) => getComputedStyle(knob).transform);
    expect(knobY).not.toBe('none');
    await expect
      .poll(() => speedKmH(page), { timeout: SLOW_MACHINE_TIMEOUT_MS })
      .toBeGreaterThan(5);

    // Thumb 2 presses HOP while thumb 1 is still down: the car leaves the ground.
    const groundY = await debugValue(page, 'y');
    const [hopX, hopY] = await centreOf(page.locator('#touch-hop'));
    await fingers.press(2, hopX, hopY);
    await expect(page.locator('#touch-hop')).toHaveClass(/pressed/);
    await expect
      .poll(() => debugValue(page, 'y'), { timeout: SLOW_MACHINE_TIMEOUT_MS })
      .toBeGreaterThan(groundY + 0.15);
    await fingers.lift(2);
    await expect(page.locator('#touch-hop')).not.toHaveClass(/pressed/);

    // Letting go eases the ring home.
    await fingers.lift(1);
    await expect(ring).not.toHaveClass(/drag/);
  });

  test('tapping the wind readout cycles it and tapping the air-mode label switches mode', async ({
    page,
  }) => {
    await page.goto('/?play=1');
    await waitForGame(page);
    const wind = page.locator('#hud-wind');
    await expect(wind).toBeVisible();
    await expect(wind).toContainText('Off');
    await wind.tap();
    await expect(wind).toContainText('Breeze');
    await wind.tap();
    await expect(wind).toContainText('Storm');
    await wind.tap();
    await expect(wind).toContainText('Off');

    await expect(page.locator('#hud-status')).toHaveText('Limited air');
    await page.locator('#hud-mode').tap();
    await expect(page.locator('#hud-status')).toHaveText('Unlimited air');
    await page.locator('#hud-mode').tap();
    await expect(page.locator('#hud-status')).toHaveText('Limited air');
  });

  test('the pause button opens the pause card, and Drop test resumes and drops the car', async ({
    page,
  }) => {
    await page.goto('/?play=1');
    await waitForGame(page);
    await page.locator('#touch-pause').tap();
    await expect(page.locator('#paused')).toBeVisible();
    await expect(page.locator('#paused-drop')).toHaveText('Drop test');
    await expect(page.locator('#paused-resume')).toBeVisible();
    const pausedAtS = await debugValue(page, 'time');
    await page.waitForTimeout(400);
    expect(await debugValue(page, 'time')).toBe(pausedAtS);

    await page.locator('#paused-drop').tap();
    await expect(page.locator('#paused')).toBeHidden();
    await expect(page.locator('#hud-nozzle-tile')).toBeVisible({
      timeout: SLOW_MACHINE_TIMEOUT_MS,
    });
  });

  test('the Motion stunts toggle in the pause card remembers its choice', async ({ page }) => {
    await page.goto('/?play=1');
    await waitForGame(page);
    await page.locator('#touch-pause').tap();
    const toggle = page.locator('#paused-motion');
    // Chromium defines DeviceMotionEvent, so the toggle shows.
    await expect(toggle).toHaveText('Motion stunts: on');
    await toggle.tap();
    await expect(toggle).toHaveText('Motion stunts: off');
    expect(await page.evaluate(() => localStorage.getItem('ninelives.motionStunts'))).toBe('off');
    await toggle.tap();
    await expect(toggle).toHaveText('Motion stunts: on');
  });

  test('a swipe on the right half asks for a stunt; taps, short swipes and buttons do not', async ({
    page,
  }) => {
    await page.goto('/?play=1');
    await waitForGame(page);
    // Throw the car into the air first (the Drop test path), then swipe.
    await page.locator('#touch-pause').tap();
    await page.locator('#paused-drop').tap();
    const fingers = await Fingers.attach(page);
    expect(await debugValue(page, 'stunts')).toBe(0);

    await fingers.swipe(1, [600, 200], [500, 200]);
    expect(await debugValue(page, 'stunts')).toBe(1);
    await fingers.swipe(1, [500, 250], [500, 150]);
    expect(await debugValue(page, 'stunts')).toBe(2);
    // under 40 px, and a swipe that starts on the left half
    await fingers.swipe(1, [600, 200], [620, 200]);
    await fingers.swipe(1, [200, 200], [100, 200]);
    expect(await debugValue(page, 'stunts')).toBe(2);
    // a swipe that starts on HOP belongs to the button
    const [hopX, hopY] = await centreOf(page.locator('#touch-hop'));
    await fingers.swipe(1, [hopX, hopY], [hopX - 80, hopY]);
    expect(await debugValue(page, 'stunts')).toBe(2);
  });
});

/** The pieces that must not overlap, when visible. The air-mode label sits on the dash by design. */
const HUD_PIECES = [
  '#hud-top-left',
  '#hud-toast',
  '#hud-alert',
  '#hud-hint',
  '#hud-wind',
  '#touch-pause',
  '#hud-dash',
  '#hud-nozzle-tile',
  '#hud-g-tile',
  '#touch-stick',
  '#touch-hop',
  '#touch-boost',
];

/** Names of visible pieces whose boxes overlap, or fall outside the screen. */
async function overlappingPieces(page: Page): Promise<string[]> {
  return page.evaluate((selectors) => {
    const boxes: { id: string; box: DOMRect }[] = [];
    for (const selector of selectors) {
      const element = document.querySelector(selector);
      if (element && element.getClientRects().length > 0) {
        boxes.push({ id: selector, box: element.getBoundingClientRect() });
      }
    }
    const problems: string[] = [];
    for (const { id, box } of boxes) {
      if (
        box.left < -0.5 ||
        box.top < -0.5 ||
        box.right > innerWidth + 0.5 ||
        box.bottom > innerHeight + 0.5
      ) {
        problems.push(`${id} leaves the screen`);
      }
    }
    for (let i = 0; i < boxes.length; i += 1) {
      for (let j = i + 1; j < boxes.length; j += 1) {
        const a = boxes[i]?.box;
        const b = boxes[j]?.box;
        if (
          a &&
          b &&
          a.left < b.right - 0.5 &&
          b.left < a.right - 0.5 &&
          a.top < b.bottom - 0.5 &&
          b.top < a.bottom - 0.5
        ) {
          problems.push(`${boxes[i]?.id} overlaps ${boxes[j]?.id}`);
        }
      }
    }
    return problems;
  }, HUD_PIECES);
}

for (const size of [
  { width: 844, height: 390 },
  { width: 667, height: 375 },
]) {
  test.describe(`HUD layout at ${size.width}x${size.height}`, () => {
    test.use({ hasTouch: true, isMobile: true, viewport: size });

    test('no HUD piece overlaps another or leaves the screen', async ({ page }) => {
      await page.goto('/?play=1&wind=storm');
      await waitForGame(page);
      // Show everything that can show: the long hint, both side tiles and the longest wind text.
      await page.evaluate(() => {
        const hint = document.getElementById('hud-hint');
        if (hint) {
          hint.textContent = 'Twist to roll, flick to flip';
          hint.hidden = false;
        }
      });
      await page.locator('#touch-pause').tap();
      await page.locator('#paused-drop').tap();
      await expect(page.locator('#hud-nozzle-tile')).toBeVisible({
        timeout: SLOW_MACHINE_TIMEOUT_MS,
      });
      await page.evaluate(() => {
        const tile = document.getElementById('hud-g-tile');
        if (tile) {
          tile.hidden = false;
        }
        const hint = document.getElementById('hud-hint');
        if (hint) {
          hint.hidden = false;
        }
      });
      expect(await overlappingPieces(page)).toEqual([]);
    });
  });
}

test.describe('on a phone held upright', () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 844 } });

  test('the rotate card shows and the game waits underneath it', async ({ page }) => {
    await page.goto('/?play=1');
    await expect(page.locator('#rotate-card')).toBeVisible();
    await page.waitForFunction(
      () =>
        (
          window as unknown as { ninelivesDebug?: { started: () => boolean } }
        ).ninelivesDebug?.started(),
      null,
      { timeout: SLOW_MACHINE_TIMEOUT_MS },
    );
    await expect(page.locator('#rotate-card')).toContainText('Rotate your phone');
    await expect(page.locator('#rotate-card')).toContainText('Nine Lives plays in landscape');
    const before = await debugValue(page, 'time');
    await page.waitForTimeout(500);
    expect(await debugValue(page, 'time')).toBe(before);
    // (The card going away in landscape is checked in the landscape tests above.)
  });
});

test.describe('on a desktop', () => {
  test.use({ viewport: { width: 1280, height: 720 } });

  test('the touch controls are absent and the readouts still click', async ({ page }) => {
    await page.goto('/?play=1');
    await waitForGame(page);
    for (const id of ['touch-stick', 'touch-hop', 'touch-boost', 'touch-pause', 'rotate-card']) {
      await expect(page.locator(`#${id}`)).toBeHidden();
    }
    await expect(page.locator('#hud-keys-toggle')).toBeVisible();

    const wind = page.locator('#hud-wind');
    await expect(wind).toContainText('Off');
    await wind.click();
    await expect(wind).toContainText('Breeze');
    await page.locator('#hud-mode').click();
    await expect(page.locator('#hud-status')).toHaveText('Unlimited air');
    // a click leaves no focus behind, so Space (hop) cannot press the button again
    await expect(page.locator('#hud-mode')).not.toBeFocused();
  });

  test('the key card lists the gamepad buttons once a gamepad connects', async ({ page }) => {
    await page.goto('/?play=1');
    await waitForGame(page);
    await page.keyboard.press('KeyH');
    await expect(page.locator('#hud-keys')).toBeVisible();
    await expect(page.locator('#hud-pad-keys')).toBeHidden();
    await page.evaluate(() => window.dispatchEvent(new Event('gamepadconnected')));
    await expect(page.locator('#hud-pad-keys')).toBeVisible();
    await expect(page.locator('#hud-pad-keys')).toContainText('Start');
  });
});
