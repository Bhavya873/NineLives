// Toasts in the centre-low message stack (SPEC §11.5): "Saved" (or "Flip saved" after the car went past
// upside down) with the air used, "Crashed" with the cause, and "Out of air". It turns
// drained sim events into short messages; it never touches physics.
import { FLOATS_PER_EVENT } from '../wasm-bridge/sim-events.ts';
import { CrashCode, EventKind } from '../wasm/nl_wasm.js';

/** How long a toast stays on screen. */
const TOAST_SHOW_MS = 1800;

/** What a CRASH toast says about the cause, by wasm `CrashCode` value. */
const CRASH_CAUSE_TEXT: Record<number, string> = {
  [CrashCode.HardImpact]: 'hit too hard',
  [CrashCode.UpsideDown]: 'landed on the roof',
  [CrashCode.Stuck]: 'stuck on its side',
  [CrashCode.OutOfBounds]: 'out of bounds',
};

/** The toast element and when it should disappear. */
interface Toasts {
  /** The centre element the text goes in. */
  element: HTMLElement;
  /** When to hide it, in `performance.now()` milliseconds. */
  hideAtMs: number;
  /**
   * Whether "Out of air" may show again. It re-arms only once the tank is full, so air
   * hovering around the empty mark does not flash the toast over and over.
   */
  noAirArmed: boolean;
}

/** Starts with nothing showing. */
export function createToasts(element: HTMLElement): Toasts {
  element.hidden = true;
  return { element, hideAtMs: 0, noAirArmed: true };
}

/**
 * The toast for a batch of drained events, or null if none deserves one. When several
 * do, the last one wins (a crash after a save is the news). A save whose peak tilt
 * reached `invertedTiltRad` (the sim's flipped mark, `Game.inverted_tilt_deg()`) is a
 * flip save.
 */
export function toastForEvents(events: Float32Array, invertedTiltRad: number): string | null {
  let text: string | null = null;
  for (let start = 0; start < events.length; start += FLOATS_PER_EVENT) {
    const kind = events[start];
    if (kind === EventKind.Save) {
      const airUsedKg = events[start + 2] ?? 0;
      const peakTiltRad = events[start + 3] ?? 0;
      const label = peakTiltRad >= invertedTiltRad ? 'Flip saved' : 'Saved';
      text = `${label} with ${airUsedKg.toFixed(1)} kg of air`;
    } else if (kind === EventKind.Crash) {
      const cause = CRASH_CAUSE_TEXT[events[start + 1] ?? 0] ?? '';
      text = cause ? `Crashed: ${cause}` : 'Crashed';
    } else if (kind === EventKind.CheckpointReached) {
      text = 'Checkpoint';
    } else if (kind === EventKind.Finished) {
      text = 'Finished';
    }
  }
  return text;
}

/** The ticket's colour: crashes and running out of air read as danger. */
export function toastTone(text: string): string {
  return text.startsWith('Crashed') || text === 'Out of air' ? 'danger' : 'plain';
}

/** Shows `text` from `nowMs` for a couple of seconds, replacing any toast showing. */
export function showToast(toasts: Toasts, text: string, nowMs: number): void {
  toasts.element.textContent = text;
  toasts.element.dataset['tone'] = toastTone(text);
  toasts.element.hidden = false;
  toasts.hideAtMs = nowMs + TOAST_SHOW_MS;
}

/**
 * Shows "Out of air" when the sim says the tank has run out, unless another toast is
 * showing (a save or crash is bigger news). `airFraction` re-arms it once the tank is full.
 */
export function showNoAirIfNew(
  toasts: Toasts,
  outOfAir: boolean,
  airFraction: number,
  nowMs: number,
): void {
  if (airFraction >= 1) {
    toasts.noAirArmed = true;
  }
  if (outOfAir && toasts.noAirArmed && toasts.element.hidden) {
    showToast(toasts, 'Out of air', nowMs);
    toasts.noAirArmed = false;
  }
}

/** Hides the toast once its time is up. */
export function updateToasts(toasts: Toasts, nowMs: number): void {
  if (!toasts.element.hidden && nowMs >= toasts.hideAtMs) {
    toasts.element.hidden = true;
  }
}
