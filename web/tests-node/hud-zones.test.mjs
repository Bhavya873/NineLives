// Unit tests for when the bottom-centre g-meter and nozzle map show. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { gMeterWanted, newZoneHolds, nozzleMapWanted, zoneOpacity } from '../src/ui/hud-zones.ts';

test('the g-meter stays hidden in the air and while driving calmly', () => {
  const holds = newZoneHolds();
  assert.equal(gMeterWanted(holds, false, 1, 0), false);
  assert.equal(gMeterWanted(holds, false, 2.4, 100), false);
});

test('the g-meter shows on a thruster key or a hard landing, then goes quickly', () => {
  const holds = newZoneHolds();
  assert.equal(gMeterWanted(holds, true, 1, 100), true);
  assert.equal(gMeterWanted(holds, false, 1, 900), true, 'inside the hold');
  assert.equal(gMeterWanted(holds, false, 1, 3000), false, 'gone after it');
  assert.equal(gMeterWanted(holds, false, 3.2, 4000), true, 'a hard landing');
});

test('the nozzle map follows the same two reasons', () => {
  const holds = newZoneHolds();
  assert.equal(nozzleMapWanted(holds, false, 1, 0), false);
  assert.equal(nozzleMapWanted(holds, true, 1, 100), true);
  assert.equal(nozzleMapWanted(holds, false, 1, 700), true);
  assert.equal(nozzleMapWanted(holds, false, 1, 3000), false);
  assert.equal(nozzleMapWanted(holds, false, 4, 5000), true);
});

test('a tile fades over the end of its hold', () => {
  assert.equal(zoneOpacity(1000, 0), 1);
  assert.equal(zoneOpacity(1000, 850), 0.5);
  assert.equal(zoneOpacity(1000, 1000), 0);
  assert.equal(zoneOpacity(1000, 2000), 0);
});
