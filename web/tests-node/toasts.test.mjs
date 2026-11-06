// Unit tests for the centre toasts (SPEC §11.5): which events get a toast and what it
// says. Plain Node, no browser.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  createToasts,
  showNoAirIfNew,
  showToast,
  toastForEvents,
  toastTone,
  updateToasts,
} from '../src/ui/toasts.ts';
import { CrashCode, EventKind } from './load-wasm.mjs';

test('a save toast says landed, with no numbers', () => {
  const events = new Float32Array([EventKind.Save, 1.2, 14.25, 2.0, 3.5]);

  assert.equal(toastForEvents(events), 'landed');
});

test('a flip save reads the same: the stunt toast names the flip', () => {
  const events = new Float32Array([EventKind.Save, 1.8, 9.0, 3.0, 3.5]);

  assert.equal(toastForEvents(events), 'landed');
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

  assert.equal(toastForEvents(events), 'crashed: roof');
});

test('each crash cause has its short words', () => {
  const say = (code) => toastForEvents(new Float32Array([EventKind.Crash, code, 0, 0, 0]));

  assert.equal(say(CrashCode.HardImpact), 'crashed: too hard');
  assert.equal(say(CrashCode.Stuck), 'crashed: on its side');
  assert.equal(say(CrashCode.OutOfBounds), 'back in bounds');
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

  assert.equal(toastForEvents(events), null);
});

/** A stand-in for the toast's page element: only the fields the module uses. */
function fakeElement() {
  return { hidden: true, textContent: '', dataset: {} };
}

test('no air shows once, and again only after the tank was full', () => {
  const toasts = createToasts(fakeElement());

  showNoAirIfNew(toasts, true, 0.01, 0);
  assert.equal(toasts.element.textContent, 'no air');
  toasts.element.hidden = true;
  showNoAirIfNew(toasts, false, 0.3, 100);
  showNoAirIfNew(toasts, true, 0.01, 200);
  assert.equal(toasts.element.hidden, true, 'not re-armed before the tank was full');
  showNoAirIfNew(toasts, false, 1, 300);
  showNoAirIfNew(toasts, true, 0.01, 400);

  assert.equal(toasts.element.hidden, false);
});

test('no air never covers a toast that is already showing', () => {
  const toasts = createToasts(fakeElement());
  showToast(toasts, 'landed', 0);

  showNoAirIfNew(toasts, true, 0.01, 10);

  assert.equal(toasts.element.textContent, 'landed');
});

test('a toast stays up about a second', () => {
  const toasts = createToasts(fakeElement());
  showToast(toasts, 'landed', 0);
  updateToasts(toasts, 900);
  assert.equal(toasts.element.hidden, false);
  updateToasts(toasts, 1100);
  assert.equal(toasts.element.hidden, true);
});

test('crashes and running out of air read as danger, everything else as plain', () => {
  assert.equal(toastTone('crashed: too hard'), 'danger');
  assert.equal(toastTone('no air'), 'danger');
  assert.equal(toastTone('landed'), 'plain');
  assert.equal(toastTone('back flip'), 'plain');
});
