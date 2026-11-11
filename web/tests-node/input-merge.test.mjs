// Unit tests for the input merge: keyboard, touch and gamepad become one frame. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { mergeFrame } from '../src/ui/input-merge.ts';
import { BUTTON_BOOST, BUTTON_HOP } from '../src/ui/keyboard.ts';

/** A keyboard-style frame: [throttle i8, steer i8, buttons low, buttons high]. */
function keyboardFrame(throttle, steer, buttons = 0) {
  const frame = new Uint8Array(4);
  const signed = new Int8Array(frame.buffer);
  signed[0] = throttle;
  signed[1] = steer;
  frame[2] = buttons & 0xff;
  frame[3] = buttons >> 8;
  return frame;
}
const none = { throttle: 0, steer: 0, buttons: 0 };

test('with no other source the keyboard frame comes through unchanged', () => {
  const out = mergeFrame(keyboardFrame(127, -127, BUTTON_HOP), [], new Uint8Array(4));
  assert.deepEqual([...new Int8Array(out.buffer)].slice(0, 2), [127, -127]);
  assert.equal(out[2], BUTTON_HOP);
});

test('analog values pass through as signed bytes', () => {
  const out = mergeFrame(
    keyboardFrame(0, 0),
    [{ ...none, throttle: 0.5, steer: -0.25 }],
    new Uint8Array(4),
  );
  assert.deepEqual([...new Int8Array(out.buffer)].slice(0, 2), [64, -32]);
});

test('each axis takes the source pushing hardest, so sources do not cancel or add up', () => {
  const out = mergeFrame(
    keyboardFrame(127, 0),
    [
      { ...none, throttle: -0.5, steer: 0.5 },
      { ...none, steer: -0.8 },
    ],
    new Uint8Array(4),
  );
  assert.deepEqual([...new Int8Array(out.buffer)].slice(0, 2), [127, -102]);
});

test('the buttons of every source are combined', () => {
  const out = mergeFrame(
    keyboardFrame(0, 0, BUTTON_HOP),
    [{ ...none, buttons: BUTTON_BOOST }, none],
    new Uint8Array(4),
  );
  assert.equal(out[2], BUTTON_HOP | BUTTON_BOOST);
});

test('a high button byte survives the merge (the manual nozzle buttons)', () => {
  const out = mergeFrame(keyboardFrame(0, 0, 1 << 12), [none], new Uint8Array(4));
  assert.equal(out[3], 1 << 4);
});

test('values past full scale are clamped', () => {
  const out = mergeFrame(
    keyboardFrame(0, 0),
    [{ ...none, throttle: 3, steer: -3 }],
    new Uint8Array(4),
  );
  assert.deepEqual([...new Int8Array(out.buffer)].slice(0, 2), [127, -127]);
});
