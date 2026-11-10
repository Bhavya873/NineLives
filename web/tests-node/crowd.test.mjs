// The training viewer's crowd (SPEC §11.6, M6, D1): one policy flies many episodes, each a
// full sim episode, every car flown by the Hybrid, and every car ends exactly as the native
// Hybrid eval says it does.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

import { Crowd, CrowdOutcome, asFloat32, policyJson } from './load-wasm.mjs';

/** Steps `crowd` until every car's episode has ended (`limit` steps at most; a drop's
 * time limit is 8 s, 960 steps). */
function runToEnd(crowd, limit = 2000) {
  let steps = 0;
  while (crowd.step()) {
    steps += 1;
    assert.ok(steps < limit, 'the crowd never finished');
  }
}

test('each car ends exactly as the native Hybrid eval of its seed does', () => {
  // The widened Stage A draws (scripted flips and rolls) are the default, so the reference is
  // the widened eval; results/eval_hybrid_A_realistic.json holds the older drops.
  const committed = JSON.parse(
    readFileSync(
      new URL(
        '../../results/widening_2026-11/widened/eval_hybrid_A_realistic.json',
        import.meta.url,
      ),
      'utf8',
    ),
  ).episodes.slice(0, 8);
  const crowd = new Crowd(policyJson, 'realistic', 0n, 8);

  runToEnd(crowd);

  const rows = JSON.parse(crowd.results_json());
  assert.deepEqual(rows.map(asFloat32), committed.map(asFloat32));
  const landed = Array.from(crowd.outcomes()).map((code) => code === CrowdOutcome.Landed);
  assert.deepEqual(
    landed,
    // Landed: a success, or a time-out after a save (a crash ends the episode).
    committed.map((row) => row.success || (row.saves > 0 && row.crash_cause === null)),
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

test('every car is flown by the Hybrid', () => {
  for (const stage of ['A', 'B', 'arena', 'D', 'F', 'gauntlet']) {
    assert.equal(
      Crowd.for_stage(policyJson, 'realistic', stage, 0n, 2).controller_name(),
      'hybrid',
    );
  }
  assert.equal(new Crowd(policyJson, 'realistic', 0n, 1).controller_name(), 'hybrid');
});

test('the crowd covers Stages A and B, the arena, Stage D, Stage F and the Gauntlet', () => {
  for (const stage of ['A', 'B', 'arena', 'D', 'F', 'gauntlet']) {
    const crowd = Crowd.for_stage(policyJson, 'realistic', stage, 0n, 3);

    crowd.step();

    assert.equal(crowd.count(), 3, stage);
    assert.equal(crowd.course_shapes() === undefined, stage === 'A' || stage === 'B', stage);
    assert.equal(crowd.chassis_poses().length, 3 * 7, stage);
  }
});

test('a generated course is drawn from its shapes and the cars on it finish', () => {
  const crowd = Crowd.for_stage(policyJson, 'realistic', 'D', 0n, 4);

  const shapes = JSON.parse(crowd.course_shapes());
  runToEnd(crowd, 120 * 120);

  assert.ok(shapes.length > 3);
  const landed = Array.from(crowd.outcomes()).filter((code) => code === CrowdOutcome.Landed);
  assert.ok(landed.length >= 3, `${landed.length} of 4 reached the finish`);
});

test('an unknown stage is refused with the valid names', () => {
  assert.throws(
    () => Crowd.for_stage(policyJson, 'realistic', 'Z', 0n, 2),
    /stage must be one of A, B, arena, D, F, gauntlet/,
  );
});

test('Stage F cars each get their own short plate label, the other stages none', () => {
  const crowd = Crowd.for_stage(policyJson, 'realistic', 'F', 16n, 8);

  const labels = crowd.car_labels();

  assert.equal(labels.length, 8);
  assert.ok(new Set(labels).size >= 3, `one round shows ${new Set(labels).size} labels`);
  for (const label of labels) {
    assert.match(label, /^[a-z+ ]+( [LR])?$/i);
    assert.ok(label.split(' ').length <= 3, label);
  }
  assert.ok(
    Crowd.for_stage(policyJson, 'realistic', 'A', 0n, 3)
      .car_labels()
      .every((l) => l === ''),
  );
});
