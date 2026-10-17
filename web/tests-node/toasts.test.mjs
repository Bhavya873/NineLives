// Unit tests for the centre toasts (SPEC §11.5): which events get a toast, and when the
// tank counts as run dry. Plain Node, no browser.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { tankJustRanDry, toastForEvents } from '../src/hud/toasts.ts';
import { CrashCode, EventKind } from './load-wasm.mjs';

test('a save toast shows the air used', () => {
  const events = new Float32Array([EventKind.Save, 1.2, 14.25, 2.0]);

  assert.equal(toastForEvents(events), 'SAVE · 14.3 kg air');
});

test('a crash after a save in the same frame wins, with its cause', () => {
  const events = new Float32Array([
    EventKind.Save,
    1.2,
    14.25,
    2.0,
    EventKind.Crash,
    CrashCode.UpsideDown,
    0,
    0,
  ]);

  assert.equal(toastForEvents(events), 'CRASH · landed on the roof');
});

test('airborne and respawn events get no toast', () => {
  const events = new Float32Array([EventKind.AirborneStart, 0, 0, 0, EventKind.Respawn, 0, 0, 0]);

  assert.equal(toastForEvents(events), null);
});

test('the tank runs dry once, and never with unlimited air', () => {
  const toasts = { element: null, hideAtMs: 0, lastAirFraction: 1 };

  assert.equal(tankJustRanDry(toasts, 0.5, false), false);
  assert.equal(tankJustRanDry(toasts, 0.001, false), true);
  assert.equal(tankJustRanDry(toasts, 0.0005, false), false);
  assert.equal(tankJustRanDry({ ...toasts, lastAirFraction: 1 }, 0, true), false);
});
