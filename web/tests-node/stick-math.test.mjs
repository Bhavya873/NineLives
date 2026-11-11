// Unit tests for the joystick maths: the knob stays in the ring, the dead zone is cut out, and
// up is gas, down is brake, right is a right turn (negative steer). Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { STICK_DEAD_ZONE, clampToRing, stickAxes } from '../src/ui/stick-math.ts';

const MAX_PX = 50;

test('a knob inside the ring is left alone and one outside is pulled back to the edge', () => {
  assert.deepEqual(clampToRing(10, -20, MAX_PX), { dxPx: 10, dyPx: -20 });
  const clamped = clampToRing(300, 400, MAX_PX);
  assert.ok(Math.abs(Math.hypot(clamped.dxPx, clamped.dyPx) - MAX_PX) < 1e-9);
  // the direction is kept: 3-4-5
  assert.ok(Math.abs(clamped.dxPx - 30) < 1e-9);
  assert.ok(Math.abs(clamped.dyPx - 40) < 1e-9);
});

test('the dead zone reads as centred and the edge of the ring as full', () => {
  const small = (STICK_DEAD_ZONE * MAX_PX) / 2;
  assert.deepEqual(stickAxes(small, small, MAX_PX), { throttle: 0, steer: 0 });
  assert.equal(stickAxes(0, -MAX_PX, MAX_PX).throttle, 1);
  assert.equal(stickAxes(0, -10 * MAX_PX, MAX_PX).throttle, 1);
});

test('just past the dead zone the stick starts from zero, not from a jump', () => {
  const justPast = (STICK_DEAD_ZONE + 0.01) * MAX_PX;
  assert.ok(stickAxes(0, -justPast, MAX_PX).throttle < 0.05);
});

test('up is gas and down is brake', () => {
  assert.ok(stickAxes(0, -40, MAX_PX).throttle > 0.5);
  assert.ok(stickAxes(0, 40, MAX_PX).throttle < -0.5);
});

test('right is a right turn (negative steer) and left is a left turn (positive)', () => {
  assert.ok(stickAxes(40, 0, MAX_PX).steer < -0.5);
  assert.ok(stickAxes(-40, 0, MAX_PX).steer > 0.5);
});

test('a stick pushed straight along one axis gives exactly zero on the other', () => {
  assert.ok(Object.is(stickAxes(0, -40, MAX_PX).steer, 0));
  assert.ok(Object.is(stickAxes(40, 0, MAX_PX).throttle, 0));
});
