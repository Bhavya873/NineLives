// The first-jump hint on touch devices: "Twist to roll, flick to flip", or the swipe version
// when motion is off. It shows in the message line, a few times per session, then stays quiet.
import { setText } from '../shared/dom.ts';

/** Hint when motion stunts work. */
export const MOTION_HINT_TEXT = 'Twist to roll, flick to flip';
/** Hint when only the swipe works. */
export const SWIPE_HINT_TEXT = 'Swipe to roll or flip';
/** The hint shows at most this many times per page session. */
export const AIR_HINT_MAX_SHOWS = 3;
/** The car must be airborne this long before the hint shows, in ms (hops do not count). */
const AIR_BEFORE_HINT_MS = 300;
/** How long the hint stays up, in ms. */
const HINT_SHOW_MS = 2500;

/** The hint element and what it has done so far. */
export interface AirHint {
  element: HTMLElement;
  shows: number;
  /** When the car left the ground, or null while it is down. */
  airborneSinceMs: number | null;
  /** When the current showing ends, or null while hidden. */
  hideAtMs: number | null;
}

/** A hint that has not shown yet. */
export function createAirHint(element: HTMLElement): AirHint {
  element.hidden = true;
  return { element, shows: 0, airborneSinceMs: null, hideAtMs: null };
}

/** Runs each frame: shows the hint on a sustained jump, hides it after a moment or on landing. */
export function updateAirHint(
  hint: AirHint,
  touch: boolean,
  airborne: boolean,
  motionOn: boolean,
  nowMs: number,
): void {
  if (!airborne) {
    hint.airborneSinceMs = null;
  } else if (hint.airborneSinceMs === null) {
    hint.airborneSinceMs = nowMs;
  }
  const showing = hint.hideAtMs !== null;
  if (showing && (!airborne || nowMs >= (hint.hideAtMs ?? 0))) {
    hint.hideAtMs = null;
    hint.element.hidden = true;
  }
  const longEnough =
    hint.airborneSinceMs !== null && nowMs - hint.airborneSinceMs >= AIR_BEFORE_HINT_MS;
  if (touch && longEnough && hint.hideAtMs === null && hint.shows < AIR_HINT_MAX_SHOWS) {
    hint.shows += 1;
    // One showing per jump: wait for the next take-off.
    hint.airborneSinceMs = Number.POSITIVE_INFINITY;
    hint.hideAtMs = nowMs + HINT_SHOW_MS;
    setText(hint.element, motionOn ? MOTION_HINT_TEXT : SWIPE_HINT_TEXT);
    hint.element.hidden = false;
  }
}
