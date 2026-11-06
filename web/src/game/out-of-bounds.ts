// Out of bounds is a free respawn: a crash event whose cause is OutOfBounds costs no life.
// The sim still counts it as a crash (evals train on it), so the game layer gives the life
// back. These helpers only read a batch of drained events; no wasm call, no physics.
import { FLOATS_PER_EVENT } from '../wasm-bridge/sim-events.ts';
import { CrashCode, EventKind } from '../wasm/nl_wasm.js';

/** How many crash events in the batch came from leaving the world. */
export function countOutOfBoundsCrashes(events: Float32Array): number {
  let count = 0;
  for (let start = 0; start < events.length; start += FLOATS_PER_EVENT) {
    if (events[start] === EventKind.Crash && events[start + 1] === CrashCode.OutOfBounds) {
      count += 1;
    }
  }
  return count;
}

/** True if the batch holds a crash that is not an out-of-bounds one (the wreck should burn). */
export function hasRealCrash(events: Float32Array): boolean {
  let crashes = 0;
  for (let start = 0; start < events.length; start += FLOATS_PER_EVENT) {
    if (events[start] === EventKind.Crash) {
      crashes += 1;
    }
  }
  return crashes > countOutOfBoundsCrashes(events);
}
