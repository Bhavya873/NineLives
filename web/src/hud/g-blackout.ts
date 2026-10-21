// G-force blackout (a HUD effect, not physics): a full-screen dark vignette that closes
// in while the passenger feels more than 3 g, the way a pilot's vision greys out. The
// edges darken with g at once; held above 3 g it keeps closing toward full black over
// about a second, and it fades back over 1.5 s once g drops. A plain HTML layer above
// the canvas; the maths is pure so tests can run it without a browser.

/**
 * Below this load (in g) the screen stays clear. A HUD-only look number; it matches the
 * reward's `shape.g_soft_limit` in reward.toml (owner: soft limit about 3 g).
 */
export const BLACKOUT_THRESHOLD_G = 3;
/** Above the threshold, each extra g adds this much darkness at once (6 g: 0.75). */
const DARKNESS_PER_EXTRA_G = 0.25;
/** The most darkness g alone gives at once; the rest builds up while g is held. */
const MAX_INSTANT_DARKNESS = 0.75;
/** Held above the threshold, darkness builds from clear to full black in this time. */
export const BLACKOUT_BUILD_S = 1.0;
/** Below the threshold, darkness fades from full black to clear in this time. */
export const BLACKOUT_RECOVER_S = 1.5;

/** How dark the view is right now, 0 (clear) to 1 (black). Carried between frames. */
export interface GBlackoutState {
  darkness: number;
}

/** A clear view. */
export function newGBlackoutState(): GBlackoutState {
  return { darkness: 0 };
}

/**
 * Steps the blackout by `dtS` seconds at a passenger load of `gLoad` (in g) and returns
 * the new darkness, 0..1. Above 3 g the darkness jumps to at least what that g gives at
 * once, then keeps building toward 1; below 3 g it fades back toward 0.
 */
export function updateGBlackout(state: GBlackoutState, gLoad: number, dtS: number): number {
  if (gLoad > BLACKOUT_THRESHOLD_G) {
    const instant = Math.min(
      MAX_INSTANT_DARKNESS,
      (gLoad - BLACKOUT_THRESHOLD_G) * DARKNESS_PER_EXTRA_G,
    );
    const built = state.darkness + dtS / BLACKOUT_BUILD_S;
    state.darkness = Math.min(1, Math.max(instant, built));
  } else {
    state.darkness = Math.max(0, state.darkness - dtS / BLACKOUT_RECOVER_S);
  }
  return state.darkness;
}

/** How the vignette layer looks at one darkness: its gradient and its flat black fill. */
export interface BlackoutLook {
  /** CSS `background-image`: clear in the middle, black at the edges. */
  gradient: string;
  /** Opacity of the plain black under the gradient, 0..1; it covers the middle last. */
  fillOpacity: number;
}

/**
 * The vignette for `darkness` (0..1). The clear middle shrinks as it darkens; past
 * halfway a flat black fill fades in so the whole view goes black at 1. `pulse` (0..1)
 * narrows the middle a little more, for a heartbeat throb; pass 0 for none.
 */
export function blackoutLook(darkness: number, pulse: number): BlackoutLook {
  const amount = Math.min(1, Math.max(0, darkness));
  const clearPercent = Math.max(0, 70 * (1 - amount) - 6 * pulse);
  const blackPercent = clearPercent + 45;
  const edgeAlpha = Math.min(1, amount * 2);
  return {
    gradient:
      `radial-gradient(ellipse at center, rgb(0 0 0 / 0) ${clearPercent.toFixed(1)}%, ` +
      `rgb(0 0 0 / ${edgeAlpha.toFixed(3)}) ${blackPercent.toFixed(1)}%)`,
    fillOpacity: Math.max(0, (amount - 0.5) * 2),
  };
}

/** Heartbeat rate of the throb while dark, in beats per second. */
const PULSE_HZ = 1.6;

/** The overlay element and what the last frame wrote, so unchanged frames cost nothing. */
export interface GBlackoutView {
  element: HTMLElement;
  reducedMotion: boolean;
  lastGradient: string;
}

/** Wraps the page's overlay element. With `reducedMotion` it darkens but never throbs. */
export function createGBlackoutView(element: HTMLElement, reducedMotion: boolean): GBlackoutView {
  return { element, reducedMotion, lastGradient: '' };
}

/** Draws `darkness` (0..1) on the overlay; `timeS` drives the throb. */
export function applyGBlackout(view: GBlackoutView, darkness: number, timeS: number): void {
  // The throb starts once the view is a third dark, and never with reduced motion.
  const throbs = !view.reducedMotion && darkness > 0.33;
  const pulse = throbs ? 0.5 + 0.5 * Math.sin(2 * Math.PI * PULSE_HZ * timeS) : 0;
  const look = darkness > 0 ? blackoutLook(darkness, pulse) : null;
  const gradient = look?.gradient ?? 'none';
  if (gradient === view.lastGradient) {
    return;
  }
  view.lastGradient = gradient;
  view.element.hidden = look === null;
  view.element.style.backgroundImage = gradient;
  view.element.style.backgroundColor = `rgb(0 0 0 / ${(look?.fillOpacity ?? 0).toFixed(3)})`;
}
