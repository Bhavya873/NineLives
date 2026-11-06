// Unit tests for the free respawn when the car leaves the world: no life lost, a short
// "back in bounds" toast, and no wreck. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { countOutOfBoundsCrashes, hasRealCrash } from '../src/game/out-of-bounds.ts';
import { grantFreeRespawns, LIVES, livesLeft } from '../src/ui/lives.ts';
import { toastForEvents, toastTone } from '../src/ui/toasts.ts';
import { CrashCode, EventKind } from './load-wasm.mjs';

/** One crash event: [kind, cause, 0, 0, 0]. */
function crash(code) {
  return [EventKind.Crash, code, 0, 0, 0];
}

test('only crashes with the out-of-bounds cause are counted as free', () => {
  const events = new Float32Array([
    ...crash(CrashCode.OutOfBounds),
    ...crash(CrashCode.HardImpact),
    ...crash(CrashCode.OutOfBounds),
  ]);

  assert.equal(countOutOfBoundsCrashes(events), 2);
  assert.equal(countOutOfBoundsCrashes(new Float32Array(0)), 0);
});

test('a batch with only out-of-bounds crashes has no wreck to burn', () => {
  assert.equal(hasRealCrash(new Float32Array(crash(CrashCode.OutOfBounds))), false);
  assert.equal(hasRealCrash(new Float32Array(crash(CrashCode.UpsideDown))), true);
});

test('an out-of-bounds crash costs no life, a real one does', () => {
  const lives = { crashesAtStart: 0, freeRespawns: 0 };
  // the sim has counted two crashes: one out of bounds, one hard landing
  const values = { crashes: 2 };

  grantFreeRespawns(lives, 1);

  assert.equal(livesLeft(lives, values), LIVES - 1);
  assert.equal(livesLeft(lives, { crashes: 1 }), LIVES);
});

test('flying out nine times in a row never ends the run', () => {
  const lives = { crashesAtStart: 0, freeRespawns: 0 };
  for (let crashes = 1; crashes <= LIVES + 3; crashes += 1) {
    grantFreeRespawns(lives, 1);
    assert.equal(livesLeft(lives, { crashes }), LIVES);
  }
});

test('the toast says back in bounds, in the plain tone', () => {
  const text = toastForEvents(new Float32Array(crash(CrashCode.OutOfBounds)));

  assert.equal(text, 'back in bounds');
  assert.equal(toastTone(text), 'plain');
});
