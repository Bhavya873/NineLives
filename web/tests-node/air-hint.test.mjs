// Unit tests for the first-jump hint: it waits for a real jump, shows once per jump, at most
// three times, and says the swipe version when motion is off. Plain Node, with a fake element.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  AIR_HINT_MAX_SHOWS,
  MOTION_HINT_TEXT,
  SWIPE_HINT_TEXT,
  createAirHint,
  updateAirHint,
} from '../src/ui/air-hint.ts';

function newHint() {
  return createAirHint({ hidden: true, textContent: '' });
}

/** One jump: airborne from `startMs` for `lengthMs`, then landed. */
function jump(hint, startMs, lengthMs, motionOn = true) {
  for (let ms = startMs; ms <= startMs + lengthMs; ms += 50) {
    updateAirHint(hint, true, true, motionOn, ms);
  }
  updateAirHint(hint, true, false, motionOn, startMs + lengthMs + 50);
}

test('a short hop shows no hint', () => {
  const hint = newHint();
  jump(hint, 0, 200);
  assert.equal(hint.shows, 0);
  assert.equal(hint.element.hidden, true);
});

test('a real jump shows the motion hint, and it hides on landing', () => {
  const hint = newHint();
  for (let ms = 0; ms <= 600; ms += 50) {
    updateAirHint(hint, true, true, true, ms);
  }
  assert.equal(hint.element.hidden, false);
  assert.equal(hint.element.textContent, MOTION_HINT_TEXT);
  updateAirHint(hint, true, false, true, 700);
  assert.equal(hint.element.hidden, true);
});

test('with motion off it says to swipe', () => {
  const hint = newHint();
  jump(hint, 0, 600, false);
  assert.equal(hint.element.textContent, SWIPE_HINT_TEXT);
});

test('it shows at most three times per session', () => {
  const hint = newHint();
  for (let index = 0; index < 6; index += 1) {
    jump(hint, index * 2000, 800);
  }
  assert.equal(hint.shows, AIR_HINT_MAX_SHOWS);
});

test('it never shows for a mouse and keyboard player', () => {
  const hint = newHint();
  for (let ms = 0; ms <= 1000; ms += 50) {
    updateAirHint(hint, false, true, true, ms);
  }
  assert.equal(hint.shows, 0);
});
