// The training viewer's crowd (SPEC §11.6, M6): one policy flies many Stage A drops, each
// a full sim episode, and every car ends exactly as the native eval says it does.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

import { Crowd, CrowdOutcome, asFloat32, policyJson } from './load-wasm.mjs';

/** Steps `crowd` until every car's episode has ended (8 s limit: 960 steps at most). */
function runToEnd(crowd) {
  let steps = 0;
  while (crowd.step()) {
    steps += 1;
    assert.ok(steps < 2000, 'the crowd never finished');
  }
}

test('each car ends exactly as the native eval of its seed does', () => {
  const committed = JSON.parse(
    readFileSync(new URL('../../results/eval_ai_A_realistic.json', import.meta.url), 'utf8'),
  ).episodes.slice(0, 8);
  const crowd = new Crowd(policyJson, 'realistic', 0n, 8);

  runToEnd(crowd);

  const rows = JSON.parse(crowd.results_json());
  assert.deepEqual(rows.map(asFloat32), committed.map(asFloat32));
  const landed = Array.from(crowd.outcomes()).map((code) => code === CrowdOutcome.Landed);
  assert.deepEqual(
    landed,
    committed.map((row) => row.success),
  );
});

test('poses come out as 7 numbers per car and 4 wheels per car', () => {
  const crowd = new Crowd(policyJson, 'arcade', 100n, 5);

  crowd.step();

  assert.equal(crowd.count(), 5);
  assert.equal(crowd.chassis_poses().length, 5 * 7);
  assert.equal(crowd.wheel_poses().length, 5 * 4 * 7);
});

test('a policy for another observation layout is refused', () => {
  const stale = JSON.parse(policyJson);
  stale.obs_schema_hash = '0000000000000000';

  assert.throws(() => new Crowd(JSON.stringify(stale), 'realistic', 0n, 2), /obs_schema_hash/);
});

/** The text of the repo's obstacle arena, courses/arena.json. */
const arenaJson = readFileSync(new URL('../../courses/arena.json', import.meta.url), 'utf8');

test('a crowd on the obstacle arena returns its shapes and every drop ends', () => {
  const crowd = Crowd.on_course(policyJson, 'realistic', 0n, 16, arenaJson);

  const shapes = JSON.parse(crowd.course_shapes());
  runToEnd(crowd);

  const surfaces = new Set(shapes.map((shape) => shape.surface));
  assert.ok(surfaces.has('floor') && surfaces.has('ramp') && surfaces.has('obstacle'));
  const outcomes = Array.from(crowd.outcomes());
  assert.ok(outcomes.every((code) => code !== CrowdOutcome.Running));
  assert.ok(outcomes.includes(CrowdOutcome.Landed), 'no car landed');
  assert.equal(JSON.parse(crowd.results_json()).length, 16);
});

test('a crowd on flat ground has no course shapes', () => {
  const crowd = new Crowd(policyJson, 'realistic', 0n, 2);

  assert.equal(crowd.course_shapes(), undefined);
});

test('a bad course is refused with the piece named', () => {
  const broken = JSON.parse(arenaJson);
  delete broken.pieces[1].length;

  assert.throws(
    () => Crowd.on_course(policyJson, 'realistic', 0n, 2, JSON.stringify(broken)),
    /piece 1 'ramp'/,
  );
});
