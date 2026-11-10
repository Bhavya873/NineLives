// Unit tests for the motion classifier: twist left and right, flick forward and back, a shake
// with no direction, readings below threshold, the cooldown, and landscape 90 vs 270. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { StuntCode } from '../src/game/stunt-requests.ts';
import {
  FLICK_RATE_DEG_S,
  GESTURE_COOLDOWN_MS,
  SHAKE_RATE_DEG_S,
  TWIST_RATE_DEG_S,
  classifyRates,
  detectGesture,
  newGestureGate,
  screenRates,
} from '../src/ui/motion-gesture.ts';

const quiet = { twist: 0, pitch: 0, side: 0 };

test('twisting counter-clockwise rolls left and clockwise rolls right', () => {
  const rate = TWIST_RATE_DEG_S + 40;
  assert.equal(classifyRates({ ...quiet, twist: rate }), StuntCode.BarrelLeft);
  assert.equal(classifyRates({ ...quiet, twist: -rate }), StuntCode.BarrelRight);
});

test('the top edge going away is a front flip and coming toward you is a back flip', () => {
  const rate = FLICK_RATE_DEG_S + 40;
  assert.equal(classifyRates({ ...quiet, pitch: -rate }), StuntCode.FrontFlip);
  assert.equal(classifyRates({ ...quiet, pitch: rate }), StuntCode.BackFlip);
});

test('readings below the thresholds ask for nothing', () => {
  assert.equal(classifyRates({ ...quiet, twist: TWIST_RATE_DEG_S - 1 }), null);
  assert.equal(classifyRates({ ...quiet, pitch: FLICK_RATE_DEG_S - 1 }), null);
  assert.equal(classifyRates(quiet), null);
});

test('a reading where no axis is 1.5 times the others is not a twist or a flick', () => {
  // twist and pitch both strong and close: neither dominates, and the total is under a shake
  assert.equal(classifyRates({ twist: 240, pitch: 200, side: 0 }), null);
});

test('a hard shake with no clear direction is a front flip', () => {
  const each = SHAKE_RATE_DEG_S / 1.5;
  assert.equal(classifyRates({ twist: each, pitch: each, side: each }), StuntCode.FrontFlip);
});

test('after a trigger the gate ignores gestures until the cooldown ends', () => {
  const gate = newGestureGate();
  const twist = { alpha: TWIST_RATE_DEG_S + 60, beta: 0, gamma: 0 };
  assert.equal(detectGesture(gate, twist, 90, 1000), StuntCode.BarrelLeft);
  assert.equal(detectGesture(gate, twist, 90, 1000 + GESTURE_COOLDOWN_MS - 1), null);
  assert.equal(detectGesture(gate, twist, 90, 1000 + GESTURE_COOLDOWN_MS), StuntCode.BarrelLeft);
});

test('a reading that triggers nothing does not start the cooldown', () => {
  const gate = newGestureGate();
  assert.equal(detectGesture(gate, { alpha: 10, beta: 0, gamma: 0 }, 90, 0), null);
  assert.equal(
    detectGesture(gate, { alpha: TWIST_RATE_DEG_S + 60, beta: 0, gamma: 0 }, 90, 1),
    StuntCode.BarrelLeft,
  );
});

test('landscape 90 and 270 flip the sign of the flick but not of the twist', () => {
  const rate = { alpha: 0, beta: 0, gamma: FLICK_RATE_DEG_S + 60 };
  assert.equal(classifyRates(screenRates(rate, 90)), StuntCode.FrontFlip);
  assert.equal(classifyRates(screenRates(rate, 270)), StuntCode.BackFlip);
  const twist = { alpha: TWIST_RATE_DEG_S + 60, beta: 0, gamma: 0 };
  assert.equal(classifyRates(screenRates(twist, 90)), StuntCode.BarrelLeft);
  assert.equal(classifyRates(screenRates(twist, 270)), StuntCode.BarrelLeft);
});

test('null rates count as zero and odd angles wrap', () => {
  const zero = screenRates({ alpha: null, beta: null, gamma: null }, 90);
  assert.equal(Math.hypot(zero.twist, zero.pitch, zero.side), 0);
  assert.deepEqual(
    screenRates({ alpha: 0, beta: 0, gamma: 5 }, -90),
    screenRates({ alpha: 0, beta: 0, gamma: 5 }, 270),
  );
});
