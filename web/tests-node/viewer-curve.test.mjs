// The training viewer's curve, leader camera and showcase seeds (plan G5): the pure logic,
// and that the committed showcase seeds still fly as recorded.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

import { flyBlock, loadRun } from '../scripts/choose-showcase-seeds.mjs';
import {
  CURVE_TITLE,
  bestCheckpoint,
  curveSeries,
  curveSvg,
  parseCurve,
} from '../src/viewer/curve.ts';
import { pickCrowdLeader, pickLeader } from '../src/viewer/leader.ts';
import {
  STAGE_TABS,
  arenaTab,
  pickShowcase,
  showcaseSeed,
  tabArena,
} from '../src/viewer/showcase.ts';

const dir = new URL('../public/checkpoints/flight-r4/', import.meta.url);
const curve = parseCurve(JSON.parse(readFileSync(new URL('curve.json', dir), 'utf8')));
const showcase = JSON.parse(readFileSync(new URL('showcase.json', dir), 'utf8'));

test('the committed curve is titled as the plan requires and carries its hashes', () => {
  assert.equal(CURVE_TITLE, 'Checkpoint evaluation under current physics');
  assert.equal(curve.title, CURVE_TITLE);
  assert.equal(curve.checkpoints.length, 8);
  assert.match(curve.physics_hash, /^[0-9a-f]{16}$/);
  assert.match(curve.controller_config_hash, /^[0-9a-f]{16}$/);
  for (const entry of curve.checkpoints) {
    assert.match(entry.policy_hash, /^[0-9a-f]{16}$/);
    assert.deepEqual(Object.keys(entry.stages), ['A', 'B', 'C', 'D', 'E', 'F']);
  }
});

test('parseCurve rejects a file that is not the curve', () => {
  assert.throws(() => parseCurve({ title: 'Training curve', checkpoints: [] }));
  assert.throws(() => parseCurve(null));
});

test('curveSeries and bestCheckpoint read one stage', () => {
  const fake = {
    ...curve,
    checkpoints: [
      { file: '1.json', steps: 1e6, policy_hash: 'a', stages: { A: { success_rate: 0.5 } } },
      { file: '2.json', steps: 2e6, policy_hash: 'b', stages: { A: { success_rate: 0.9 } } },
      { file: '3.json', steps: 3e6, policy_hash: 'c', stages: { A: { success_rate: 0.9 } } },
    ],
  };
  assert.deepEqual(
    curveSeries(fake, 'A').map((point) => point.rate),
    [0.5, 0.9, 0.9],
  );
  assert.equal(bestCheckpoint(fake, 'A')?.file, '3.json');
  assert.deepEqual(curveSeries(fake, 'Z'), []);
  assert.equal(bestCheckpoint(fake, 'Z'), null);
});

test('curveSvg draws one dot per checkpoint', () => {
  const svg = curveSvg(curve, 'F');
  assert.equal(svg.match(/<circle/g)?.length, 8);
  assert.ok(svg.includes('<polyline'));
  assert.ok(svg.includes('#ffd166'), 'the best point is ringed');
});

test('the leader is the furthest running car, then landed, then crashed', () => {
  const stride = 7;
  const poses = new Float32Array(stride * 3);
  poses[0] = 50; // car 0, x = 50, crashed
  poses[stride] = 10; // car 1, x = 10, running
  poses[2 * stride] = 30; // car 2, x = 30, running
  poses[2 * stride + 2] = 4;
  assert.deepEqual(pickLeader(poses, Uint8Array.of(2, 0, 0), stride), { car: 2, x: 30, z: 4 });
  assert.equal(pickLeader(poses, Uint8Array.of(2, 1, 2), stride)?.car, 1);
  assert.equal(pickLeader(poses, Uint8Array.of(2, 2, 2), stride)?.car, 0);
  assert.equal(pickLeader(new Float32Array(), new Uint8Array(), stride), null);
});

test('the crowd leader names the row of the best car', () => {
  const stride = 3;
  const rowA = { poses: Float32Array.of(5, 0, 0), outcomes: Uint8Array.of(0) };
  const rowB = { poses: Float32Array.of(9, 0, 1), outcomes: Uint8Array.of(0) };
  const rowC = { poses: Float32Array.of(99, 0, 2), outcomes: Uint8Array.of(2) };
  assert.deepEqual(pickCrowdLeader([rowA, rowB, rowC], stride), { car: 0, x: 9, z: 1, row: 1 });
  assert.equal(pickCrowdLeader([], stride), null);
});

test('pickShowcase: most landed, then biggest gain, then lowest seed', () => {
  const block = (firstSeed, bestLanded, firstLanded) => ({
    firstSeed,
    size: 8,
    bestLanded,
    firstLanded,
  });
  assert.equal(pickShowcase([]), null);
  assert.equal(pickShowcase([block(0, 7, 0), block(8, 8, 8), block(16, 8, 3)])?.firstSeed, 16);
  assert.equal(pickShowcase([block(0, 8, 3), block(8, 8, 3)])?.firstSeed, 0);
});

test('tabs map to arenas the crowd can fly', () => {
  assert.deepEqual([...STAGE_TABS], ['A', 'B', 'C', 'D', 'E', 'F']);
  assert.equal(tabArena('A'), 'flat');
  assert.equal(tabArena('D'), 'driving');
  assert.equal(tabArena('F'), 'flight');
  assert.equal(tabArena('B'), null);
  assert.equal(arenaTab('obstacles'), 'A');
  assert.equal(arenaTab('gauntlet'), 'D');
});

test('showcaseSeed starts on the committed seed and steps by blocks', () => {
  assert.equal(showcaseSeed(null, 'flat', 3, 8), 24);
  const file = { seeds: { flat: { first_seed: 40 } } };
  assert.equal(showcaseSeed(file, 'flat', 0, 8), 40);
  assert.equal(showcaseSeed(file, 'flat', 2, 8), 56);
  assert.equal(showcaseSeed(file, 'driving', 1, 8), 8);
});

test('the committed showcase seeds fly as recorded (reproducible)', () => {
  const { first, best } = loadRun();
  const stages = { flat: 'A', driving: 'D', flight: 'F' };
  assert.deepEqual(Object.keys(showcase.seeds).sort(), Object.keys(stages).sort());
  for (const [arena, stage] of Object.entries(stages)) {
    const entry = showcase.seeds[arena];
    assert.equal(flyBlock(best, stage, entry.first_seed), entry.best_landed, arena);
    assert.equal(flyBlock(first, stage, entry.first_seed), entry.first_landed, arena);
  }
});
