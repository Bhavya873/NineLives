// Unit tests for the arrow pad's maths: eight directions, a dead zone in the middle, and the
// same signs as the game (up is gas, left is positive steer). Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { DPAD_DEAD_ZONE, dpadAxes } from '../src/ui/dpad-math.ts';

const HALF_PX = 80;

test('a thumb on the centre presses nothing', () => {
  assert.deepEqual(dpadAxes(0, 0, HALF_PX), { throttle: 0, steer: 0 });
});

test('each arrow gives one axis: up gas, down brake, left positive steer, right negative', () => {
  assert.deepEqual(dpadAxes(0, -60, HALF_PX), { throttle: 1, steer: 0 });
  assert.deepEqual(dpadAxes(0, 60, HALF_PX), { throttle: -1, steer: 0 });
  assert.deepEqual(dpadAxes(-60, 0, HALF_PX), { throttle: 0, steer: 1 });
  assert.deepEqual(dpadAxes(60, 0, HALF_PX), { throttle: 0, steer: -1 });
});

test('each diagonal holds gas or brake together with a turn', () => {
  assert.deepEqual(dpadAxes(-50, -50, HALF_PX), { throttle: 1, steer: 1 });
  assert.deepEqual(dpadAxes(50, -50, HALF_PX), { throttle: 1, steer: -1 });
  assert.deepEqual(dpadAxes(-50, 50, HALF_PX), { throttle: -1, steer: 1 });
  assert.deepEqual(dpadAxes(50, 50, HALF_PX), { throttle: -1, steer: -1 });
});

test('a thumb a little off an arrow stays on that arrow, a clear slant becomes a diagonal', () => {
  assert.deepEqual(dpadAxes(10, -60, HALF_PX), { throttle: 1, steer: 0 });
  assert.deepEqual(dpadAxes(-60, 10, HALF_PX), { throttle: 0, steer: 1 });
  assert.deepEqual(dpadAxes(30, -60, HALF_PX), { throttle: 1, steer: -1 });
});

test('the dead zone ends where DPAD_DEAD_ZONE says', () => {
  const edgePx = DPAD_DEAD_ZONE * HALF_PX;
  assert.deepEqual(dpadAxes(0, -(edgePx - 0.5), HALF_PX), { throttle: 0, steer: 0 });
  assert.deepEqual(dpadAxes(0, -(edgePx + 0.5), HALF_PX), { throttle: 1, steer: 0 });
});

test('a thumb dragged far outside the cross still counts by direction', () => {
  assert.deepEqual(dpadAxes(-400, -380, HALF_PX), { throttle: 1, steer: 1 });
  assert.deepEqual(dpadAxes(900, 10, HALF_PX), { throttle: 0, steer: -1 });
});

test('no axis comes out as negative zero', () => {
  const axes = dpadAxes(0, -60, HALF_PX);
  assert.ok(Object.is(axes.steer, 0));
});
