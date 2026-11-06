// Unit tests for the g-meter and the approach HUD logic. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  FAST_SPEED_M_S,
  isFast,
  newTooSlowState,
  outsideEnvelope,
  scrapeSpeeds,
  approachHintText,
  tooSlowWarning,
  updateTooSlow,
} from '../src/ui/approach-hud.ts';
import { formatG, gLevel, gNeedleFraction } from '../src/ui/g-meter.ts';
import { burstCount, emitBurst, newSparkField, stepSparks } from '../src/render/sparks.ts';

test('the needle is clamped and levels follow 3 g and 6 g', () => {
  assert.equal(gNeedleFraction(-1), 0);
  assert.equal(gNeedleFraction(Number.NaN), 0);
  assert.equal(gNeedleFraction(4), 0.5);
  assert.equal(gNeedleFraction(100), 1);
  assert.deepEqual([1, 3, 6].map(gLevel), ['calm', 'warn', 'high']);
  assert.equal(formatG(3.44), '3.4 g');
  assert.equal(formatG(Number.NaN), '0.0 g');
});

test('speed reads as fast above 200 km/h, either direction', () => {
  assert.ok(Math.abs(FAST_SPEED_M_S - 55.56) < 0.01);
  assert.equal(isFast(55.5), false);
  assert.equal(isFast(56), true);
  assert.equal(isFast(-60), true);
});

test('the AI badge shows only for flag 1', () => {
  assert.equal(outsideEnvelope(1), true);
  assert.equal(outsideEnvelope(0), false);
});

test('too-slow warning comes from event 16 and times out', () => {
  assert.equal(tooSlowWarning(new Float32Array([15, 2, 0, 0, 0])), null);
  const events = new Float32Array([16, 3, 10, 0, 0]);
  assert.equal(tooSlowWarning(events), 'too slow');
  const state = newTooSlowState();
  assert.match(updateTooSlow(state, events, 5), /too slow/);
  assert.match(updateTooSlow(state, new Float32Array(0), 7), /too slow/);
  assert.equal(updateTooSlow(state, new Float32Array(0), 9), '');
});

test('the approach hint names the needed speed before the piece and is empty at 0', () => {
  assert.equal(approachHintText(0), '');
  assert.equal(approachHintText(Number.NaN), '');
  assert.equal(approachHintText(14), 'need 51 km/h');
});

test('scrape events give their normal speeds; sparks burst, fall and die', () => {
  assert.deepEqual(
    scrapeSpeeds(new Float32Array([15, 2.5, 0, 0, 0, 16, 1, 1, 0, 0, 15, 4, 0, 0, 0])),
    [2.5, 4],
  );
  assert.ok(burstCount(4) > burstCount(0));
  const field = newSparkField();
  emitBurst(field, 0, 1, 0, 4);
  assert.equal(stepSparks(field, 0.1), burstCount(4));
  assert.equal(stepSparks(field, 1), 0);
});
