// Unit tests for when the bottom-centre g-meter and nozzle map show. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  gMeterWanted,
  isAnyNozzleFiring,
  newZoneHolds,
  nozzleMapWanted,
  zoneOpacity,
} from '../src/ui/hud-zones.ts';
import { nozzleWhoText } from '../src/ui/hud.ts';
import { ControllerCode } from './load-wasm.mjs';

/** A frame's reasons, all quiet unless overridden. */
function reasons(nowMs, overrides = {}) {
  return { thrusterKeyHeld: false, anyNozzleFiring: false, gLoad: 1, nowMs, ...overrides };
}

test('the g-meter stays hidden in the air and while driving calmly', () => {
  const holds = newZoneHolds();
  assert.equal(gMeterWanted(holds, reasons(0)), false);
  assert.equal(gMeterWanted(holds, reasons(100, { gLoad: 2.4 })), false);
});

test('the g-meter shows on a thruster key or a hard landing, then goes quickly', () => {
  const holds = newZoneHolds();
  assert.equal(gMeterWanted(holds, reasons(100, { thrusterKeyHeld: true })), true);
  assert.equal(gMeterWanted(holds, reasons(900)), true, 'inside the hold');
  assert.equal(gMeterWanted(holds, reasons(3000)), false, 'gone after it');
  assert.equal(gMeterWanted(holds, reasons(4000, { gLoad: 3.2 })), true, 'a hard landing');
});

test('the nozzle map follows the key and the hard landing too', () => {
  const holds = newZoneHolds();
  assert.equal(nozzleMapWanted(holds, reasons(0)), false);
  assert.equal(nozzleMapWanted(holds, reasons(100, { thrusterKeyHeld: true })), true);
  assert.equal(nozzleMapWanted(holds, reasons(700)), true);
  assert.equal(nozzleMapWanted(holds, reasons(3000)), false);
  assert.equal(nozzleMapWanted(holds, reasons(5000, { gLoad: 4 })), true);
});

test('both tiles show whenever any nozzle fires, and hold briefly after', () => {
  const holds = newZoneHolds();
  assert.equal(nozzleMapWanted(holds, reasons(100, { anyNozzleFiring: true })), true);
  assert.equal(gMeterWanted(holds, reasons(100, { anyNozzleFiring: true })), true);
  assert.equal(nozzleMapWanted(holds, reasons(600)), true, 'inside the hold');
  assert.equal(nozzleMapWanted(holds, reasons(1500)), false, 'gone after it');
});

test('a nozzle counts as firing only above the small threshold', () => {
  assert.equal(isAnyNozzleFiring(new Float32Array(10)), false);
  assert.equal(isAnyNozzleFiring(new Float32Array([0, 0.01, 0])), false);
  assert.equal(isAnyNozzleFiring(new Float32Array([0, 0, 0.3])), true);
});

test('the nozzle map names who flies: the policy or the thrusters', () => {
  assert.equal(nozzleWhoText(ControllerCode.Ai), 'AI flying');
  assert.equal(nozzleWhoText(ControllerCode.HybridAi), 'AI flying');
  assert.equal(nozzleWhoText(ControllerCode.Pd), 'Thrusters');
  assert.equal(nozzleWhoText(ControllerCode.HybridPd), 'Thrusters');
});

test('a tile fades over the end of its hold', () => {
  assert.equal(zoneOpacity(1000, 0), 1);
  assert.equal(zoneOpacity(1000, 850), 0.5);
  assert.equal(zoneOpacity(1000, 1000), 0);
  assert.equal(zoneOpacity(1000, 2000), 0);
});
