// Unit tests for when the bottom-centre g-meter and nozzle map show. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { gMeterWanted, newZoneHolds, nozzleMapWanted } from '../src/ui/hud-zones.ts';

test('the g-meter shows in the air or under load, and holds a moment after', () => {
  const holds = newZoneHolds();
  assert.equal(gMeterWanted(holds, false, 1, 0), false);
  assert.equal(gMeterWanted(holds, true, 1, 100), true);
  // Landed and calm: still shown inside the hold, gone after it.
  assert.equal(gMeterWanted(holds, false, 1, 1000), true);
  assert.equal(gMeterWanted(holds, false, 1, 5000), false);
  assert.equal(gMeterWanted(holds, false, 3.2, 6000), true);
});

test('the nozzle map shows while a nozzle fires, and holds a moment after', () => {
  const holds = newZoneHolds();
  assert.equal(nozzleMapWanted(holds, new Float32Array(10), 0), false);
  const firing = new Float32Array(10);
  firing[3] = 0.6;
  assert.equal(nozzleMapWanted(holds, firing, 100), true);
  assert.equal(nozzleMapWanted(holds, new Float32Array(10), 800), true);
  assert.equal(nozzleMapWanted(holds, new Float32Array(10), 3000), false);
});
