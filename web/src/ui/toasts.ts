// Toasts in the centre-low message stack (SPEC §11.5): "landed" after a jump, "crashed: <cause>",
// "no air" and "back in bounds". It turns drained sim events into short messages (a word or
// three, no numbers); it never touches physics.
import { FLOATS_PER_EVENT } from '../wasm-bridge/sim-events.ts';
import { CrashCode, EventKind } from '../wasm/nl_wasm.js';

/** How long a toast stays on screen. */
const TOAST_SHOW_MS = 1000;

/** What a CRASH toast says about the cause, by wasm `CrashCode` value. */
const CRASH_CAUSE_TEXT: Record<number, string> = {
  [CrashCode.HardImpact]: 'too hard',
  [CrashCode.UpsideDown]: 'roof',
  [CrashCode.Stuck]: 'on its side',
};

/** Said when the car flew out of the world: a free respawn, no life lost (game/main.ts). */
export const BACK_IN_BOUNDS_TEXT = 'back in bounds';
/** Said when the tank runs dry. */
export const NO_AIR_TEXT = 'no air';

/** The toast element and when it should disappear. */
interface Toasts {
  /** The centre element the text goes in. */
  element: HTMLElement;
  /** When to hide it, in `performance.now()` milliseconds. */
  hideAtMs: number;
  /**
   * Whether "no air" may show again. It re-arms only once the tank is full, so air
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
 * do, the last one wins (a crash after a save is the news).
 */
export function toastForEvents(events: Float32Array): string | null {
  let text: string | null = null;
  for (let start = 0; start < events.length; start += FLOATS_PER_EVENT) {
    const kind = events[start];
    if (kind === EventKind.Save) {
      text = 'landed';
    } else if (kind === EventKind.Crash) {
      const code = events[start + 1] ?? 0;
      const cause = CRASH_CAUSE_TEXT[code] ?? '';
      if (code === CrashCode.OutOfBounds) {
        text = BACK_IN_BOUNDS_TEXT;
      } else {
        text = cause ? `crashed: ${cause}` : 'crashed';
      }
    } else if (kind === EventKind.CheckpointReached) {
      text = 'checkpoint';
    } else if (kind === EventKind.Finished) {
      text = 'finished';
    }
  }
  return text;
}

/** The ticket's colour: crashes and running out of air read as danger (the words say it too). */
export function toastTone(text: string): string {
  return text.startsWith('crashed') || text === NO_AIR_TEXT ? 'danger' : 'plain';
}

/** Shows `text` from `nowMs` for about a second, replacing any toast showing. */
export function showToast(toasts: Toasts, text: string, nowMs: number): void {
  toasts.element.textContent = text;
  toasts.element.dataset['tone'] = toastTone(text);
  toasts.element.hidden = false;
  toasts.hideAtMs = nowMs + TOAST_SHOW_MS;
}

/**
 * Shows "no air" when the sim says the tank has run out, unless another toast is
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
    showToast(toasts, NO_AIR_TEXT, nowMs);
    toasts.noAirArmed = false;
  }
}

/** Hides the toast once its time is up. */
export function updateToasts(toasts: Toasts, nowMs: number): void {
  if (!toasts.element.hidden && nowMs >= toasts.hideAtMs) {
    toasts.element.hidden = true;
  }
}
