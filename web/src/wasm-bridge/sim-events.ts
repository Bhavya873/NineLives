// Reading the flat event array from `Game.drain_events` (SPEC §7.9, §11.1).
// Each event is FLOATS_PER_EVENT numbers: an `EventKind`, then up to four values (Save: duration, air used, peak tilt, peak g).
import type { EventKind } from '../wasm/nl_wasm.js';

/** Numbers per event; layout-check.ts checks it against the wasm build at start-up. */
export const FLOATS_PER_EVENT = 5;

/** True if any of the events is of this kind. */
export function containsEvent(events: Float32Array, kind: EventKind): boolean {
  for (let start = 0; start < events.length; start += FLOATS_PER_EVENT) {
    if (events[start] === kind) {
      return true;
    }
  }
  return false;
}
