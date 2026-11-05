// Unit tests for the centre toasts (SPEC §11.5): which events get a toast and what it
// says. Plain Node, no browser.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { createToasts, showNoAirIfNew, showToast, toastForEvents } from '../src/ui/toasts.ts';
import { CrashCode, EventKind } from './load-wasm.mjs';

/** The sim's flipped mark (150°), in radians. */
const INVERTED_TILT_RAD = (150 * Math.PI) / 180;

test('a save toast shows the air used', () => {
  const events = new Float32Array([EventKind.Save, 1.2, 14.25, 2.0, 3.5]);

  assert.equal(toastForEvents(events, INVERTED_TILT_RAD), 'Saved with 14.3 kg of air');
});

test('a save past upside down is a flip save', () => {
  const events = new Float32Array([EventKind.Save, 1.8, 9.0, 3.0, 3.5]);

  assert.equal(toastForEvents(events, INVERTED_TILT_RAD), 'Flip saved with 9.0 kg of air');
});

test('a crash after a save in the same frame wins, with its cause', () => {
  const events = new Float32Array([
    EventKind.Save,
    1.2,
    14.25,
    2.0,
    3.5,
    EventKind.Crash,
    CrashCode.UpsideDown,
    0,
    0,
    0,
  ]);

  assert.equal(toastForEvents(events, INVERTED_TILT_RAD), 'Crashed: landed on the roof');
});

test('airborne and respawn events get no toast', () => {
  const events = new Float32Array([
    EventKind.AirborneStart,
    0,
    0,
    0,
    0,
    EventKind.Respawn,
    0,
    0,
    0,
    0,
  ]);

  assert.equal(toastForEvents(events, INVERTED_TILT_RAD), null);
});

/** A stand-in for the toast's page element: only the two fields the module uses. */
function fakeElement() {
  return { hidden: true, textContent: '' };
}

test('Out of air shows once, and again only after the tank was full', () => {
  const toasts = createToasts(fakeElement());

  showNoAirIfNew(toasts, true, 0.01, 0);
  assert.equal(toasts.element.textContent, 'Out of air');
  toasts.element.hidden = true;
  showNoAirIfNew(toasts, false, 0.3, 100);
  showNoAirIfNew(toasts, true, 0.01, 200);
  assert.equal(toasts.element.hidden, true, 'not re-armed before the tank was full');
  showNoAirIfNew(toasts, false, 1, 300);
  showNoAirIfNew(toasts, true, 0.01, 400);

  assert.equal(toasts.element.hidden, false);
});

test('Out of air never covers a toast that is already showing', () => {
  const toasts = createToasts(fakeElement());
  showToast(toasts, 'Saved with 3.0 kg of air', 0);

  showNoAirIfNew(toasts, true, 0.01, 10);

  assert.equal(toasts.element.textContent, 'Saved with 3.0 kg of air');
});
