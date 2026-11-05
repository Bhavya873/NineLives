// Unit tests for the keyboard: arrows drive and steer, A S D F are stunt keys only, R respawns.
// Plain Node: a fake window collects the listeners.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  BUTTON_RESET,
  isNozzleKeyHeld,
  onKeyPress,
  readInputFrame,
  startKeyboard,
} from '../src/ui/keyboard.ts';

const listeners = {};
startKeyboard({ addEventListener: (name, listener) => (listeners[name] = listener) });

/** Presses a key and returns whether the page's default action was blocked. */
function down(code) {
  let blocked = false;
  listeners.keydown({ code, repeat: false, preventDefault: () => (blocked = true) });
  return blocked;
}
function up(code) {
  listeners.keyup({ code });
}

test('arrows drive and steer', () => {
  down('ArrowUp');
  down('ArrowLeft');
  const frame = readInputFrame();
  assert.equal(new Int8Array(frame.buffer)[0], 127);
  assert.equal(new Int8Array(frame.buffer)[1], 127);
  up('ArrowUp');
  up('ArrowLeft');
});

test('W A S D no longer drive or steer', () => {
  for (const code of ['KeyW', 'KeyA', 'KeyS', 'KeyD']) {
    down(code);
  }
  const frame = new Int8Array(readInputFrame().buffer);
  assert.equal(frame[0], 0);
  assert.equal(frame[1], 0);
  for (const code of ['KeyW', 'KeyA', 'KeyS', 'KeyD']) {
    up(code);
  }
});

test('the stunt keys are handled by the game, not the page', () => {
  for (const code of ['KeyA', 'KeyS', 'KeyD', 'KeyF']) {
    assert.equal(down(code), true, code);
    up(code);
  }
});

test('stunt keys fire their press action once', () => {
  let presses = 0;
  onKeyPress('KeyF', () => (presses += 1));
  down('KeyF');
  assert.equal(presses, 1);
  up('KeyF');
});

test('R still respawns', () => {
  down('KeyR');
  const frame = readInputFrame();
  assert.equal(frame[2] & BUTTON_RESET, BUTTON_RESET);
  up('KeyR');
});

test('a nozzle key counts as a thruster key while held', () => {
  assert.equal(isNozzleKeyHeld(), false);
  down('Digit3');
  assert.equal(isNozzleKeyHeld(), true);
  up('Digit3');
  assert.equal(isNozzleKeyHeld(), false);
});
