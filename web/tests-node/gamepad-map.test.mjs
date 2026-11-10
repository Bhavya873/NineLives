// Unit tests for the gamepad mapping: triggers drive, the left stick steers with a 0.15 dead
// zone, and a held button asks once. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { StuntCode } from '../src/game/stunt-requests.ts';
import {
  PAD_STICK_DEAD_ZONE,
  PAD_STUNT_BUTTONS,
  PadButton,
  applyDeadZone,
  heldButtons,
  newlyPressed,
  padAxes,
} from '../src/ui/gamepad-map.ts';

/** A pad with the given trigger pulls, left stick X and pressed buttons. */
function pad({ rt = 0, lt = 0, stickX = 0, pressed = [] } = {}) {
  const buttons = Array.from({ length: 17 }, () => ({ pressed: false, value: 0 }));
  buttons[PadButton.RightTrigger] = { pressed: rt > 0.5, value: rt };
  buttons[PadButton.LeftTrigger] = { pressed: lt > 0.5, value: lt };
  for (const index of pressed) {
    buttons[index] = { pressed: true, value: 1 };
  }
  return { axes: [stickX, 0, 0, 0], buttons };
}

test('the right trigger is gas and the left trigger is brake', () => {
  assert.equal(padAxes(pad({ rt: 1 })).throttle, 1);
  assert.equal(padAxes(pad({ lt: 1 })).throttle, -1);
  assert.ok(Math.abs(padAxes(pad({ rt: 0.5 })).throttle - 0.4737) < 0.001);
});

test('both triggers together cancel', () => {
  assert.equal(padAxes(pad({ rt: 1, lt: 1 })).throttle, 0);
});

test('the left stick steers: right is negative, left is positive', () => {
  assert.equal(padAxes(pad({ stickX: 1 })).steer, -1);
  assert.equal(padAxes(pad({ stickX: -1 })).steer, 1);
});

test('the stick has a 0.15 dead zone', () => {
  assert.equal(PAD_STICK_DEAD_ZONE, 0.15);
  assert.ok(Object.is(padAxes(pad({ stickX: 0.14 })).steer, 0));
  assert.ok(Object.is(padAxes(pad({ stickX: -0.15 })).steer, 0));
  assert.ok(padAxes(pad({ stickX: 0.3 })).steer < 0);
  assert.equal(applyDeadZone(1, 0.15), 1);
});

test('the stunt buttons map to the stunts', () => {
  assert.equal(PAD_STUNT_BUTTONS[PadButton.LeftBumper], StuntCode.BarrelLeft);
  assert.equal(PAD_STUNT_BUTTONS[PadButton.RightBumper], StuntCode.BarrelRight);
  assert.equal(PAD_STUNT_BUTTONS[PadButton.Y], StuntCode.FrontFlip);
  assert.equal(PAD_STUNT_BUTTONS[PadButton.X], StuntCode.BackFlip);
});

test('a button held across polls is pressed once', () => {
  const up = heldButtons(pad(), 10);
  const down = heldButtons(pad({ pressed: [PadButton.Y] }), 10);
  assert.deepEqual(newlyPressed(up, down), [PadButton.Y]);
  assert.deepEqual(newlyPressed(down, down), []);
  assert.deepEqual(newlyPressed(down, up), []);
  // the first poll after a pad connects has no previous state
  assert.deepEqual(newlyPressed([], down), [PadButton.Y]);
});

test('A hop, B boost and Start pause are the buttons 0, 1 and 9', () => {
  assert.equal(PadButton.A, 0);
  assert.equal(PadButton.B, 1);
  assert.equal(PadButton.Start, 9);
  assert.equal(PadButton.LeftBumper, 4);
  assert.equal(PadButton.RightBumper, 5);
});
