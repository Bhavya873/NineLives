// Centre-screen toasts (SPEC §11.5): "SAVE" with the air used, "CRASH" with the cause,
// and "NO AIR". It turns drained sim events into short messages; it never touches physics.
// The .ts extension lets the Node unit tests load this file without a bundler.
import { FLOATS_PER_EVENT } from '../sim-events.ts';
import { CrashCode, EventKind } from '../wasm/nl_wasm.js';

/** How long a toast stays on screen. */
const TOAST_SHOW_MS = 1800;
/** Below this air fraction the tank counts as empty (it fades out, never quite 0). */
const NO_AIR_FRACTION = 0.01;

/** What a CRASH toast says about the cause, by wasm `CrashCode` value. */
const CRASH_CAUSE_TEXT: Record<number, string> = {
  [CrashCode.HardImpact]: 'hit too hard',
  [CrashCode.UpsideDown]: 'landed on the roof',
  [CrashCode.Stuck]: 'stuck on its side',
  [CrashCode.OutOfBounds]: 'out of bounds',
};

/** The toast element and when it should disappear. */
export interface Toasts {
  element: HTMLElement;
  hideAtMs: number;
  /** Air fraction last frame, to spot the moment the tank runs dry. */
  lastAirFraction: number;
}

/** Starts with nothing showing. */
export function createToasts(element: HTMLElement): Toasts {
  element.hidden = true;
  return { element, hideAtMs: 0, lastAirFraction: 1 };
}

/**
 * The toast for a batch of drained events, or null if none deserves one. When several
 * do, the last one wins (a crash after a save is the news).
 */
export function toastForEvents(events: Float32Array): string | null {
  let text: string | null = null;
  for (let start = 0; start < events.length; start += FLOATS_PER_EVENT) {
    const kind = events[start];
    if (kind === EventKind.Save) {
      const airUsedKg = events[start + 2] ?? 0;
      text = `SAVE · ${airUsedKg.toFixed(1)} kg air`;
    } else if (kind === EventKind.Crash) {
      const cause = CRASH_CAUSE_TEXT[events[start + 1] ?? 0] ?? '';
      text = cause ? `CRASH · ${cause}` : 'CRASH';
    }
  }
  return text;
}

/** True on the frame the tank runs dry (never with unlimited air). */
export function tankJustRanDry(
  toasts: Toasts,
  airFraction: number,
  unlimitedAir: boolean,
): boolean {
  const ranDry =
    !unlimitedAir && airFraction < NO_AIR_FRACTION && toasts.lastAirFraction >= NO_AIR_FRACTION;
  toasts.lastAirFraction = airFraction;
  return ranDry;
}

/** Shows `text` from `nowMs` for a couple of seconds, replacing any toast showing. */
export function showToast(toasts: Toasts, text: string, nowMs: number): void {
  toasts.element.textContent = text;
  toasts.element.hidden = false;
  toasts.hideAtMs = nowMs + TOAST_SHOW_MS;
}

/** Hides the toast once its time is up. */
export function updateToasts(toasts: Toasts, nowMs: number): void {
  if (!toasts.element.hidden && nowMs >= toasts.hideAtMs) {
    toasts.element.hidden = true;
  }
}
