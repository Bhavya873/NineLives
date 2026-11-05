// Unit tests for the fixed-step accumulator (SPEC §11.2). Plain Node, no browser.
// Vite is not involved, so this test reads the TypeScript through Node's type stripping.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { planTicks } from '../src/game/game-loop.ts';

const TICK_S = 1 / 120;

test('one 60 Hz frame runs two physics ticks', () => {
  const plan = planTicks(0, 1 / 60, TICK_S);

  assert.equal(plan.ticks, 2);
  assert.ok(Math.abs(plan.leftoverS) < 1e-9);
});

test('leftover time carries into the next frame', () => {
  const first = planTicks(0, 0.013, TICK_S);
  const second = planTicks(first.leftoverS, 0.013, TICK_S);

  assert.equal(first.ticks, 1);
  assert.equal(second.ticks, 2);
});

test('a long pause runs at most 8 ticks and drops the rest', () => {
  const plan = planTicks(0, 5, TICK_S);

  assert.equal(plan.ticks, 8);
  assert.equal(plan.leftoverS, 0);
});
