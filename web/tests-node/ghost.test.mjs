// Unit tests for the ghost run: recording, storing the better run, and replay. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  GHOST_FLOATS,
  courseScope,
  ghostKey,
  GHOST_MAX_S,
  createGhostRecorder,
  finishGhostRun,
  isBetterRun,
  loadGhost,
  recordGhostSample,
  resetGhostRecorder,
  sampleGhost,
  saveGhostIfBetter,
} from '../src/ghost.ts';

function memoryStorage(initial = {}) {
  const data = new Map(Object.entries(initial));
  return {
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => data.set(key, value),
  };
}

const IDENTITY = [0, 0, 0, 1];

function recordLine(recorder, seconds) {
  // Frames at 20 Hz; the car is at x = 10 * t.
  for (let step = 0; step <= seconds * 20; step += 1) {
    const t = step / 20;
    recordGhostSample(recorder, t, [t * 10, 1, 0, ...IDENTITY]);
  }
}

test('samples are taken at 10 Hz and rounded to a millimetre', () => {
  const recorder = createGhostRecorder();
  recordGhostSample(recorder, 0, [1.23456, 0, 0, ...IDENTITY]);
  recordGhostSample(recorder, 0.05, [9, 9, 9, ...IDENTITY]);
  recordGhostSample(recorder, 0.1, [2, 0, 0, ...IDENTITY]);

  assert.equal(recorder.poses.length, 2 * GHOST_FLOATS);
  assert.equal(recorder.poses[0], 1.235);
  assert.equal(recorder.poses[GHOST_FLOATS], 2);
});

test('slots stay on the 10 Hz grid when frames come at 60 Hz, and late frames hold the pose', () => {
  const recorder = createGhostRecorder();
  for (let frame = 0; frame <= 60; frame += 1) {
    recordGhostSample(recorder, frame / 60, [frame, 0, 0, ...IDENTITY]);
  }
  // One second at 10 Hz: slots 0 to 1.0 s, 11 samples, and the run's duration says 1.0 s.
  assert.equal(recorder.poses.length / GHOST_FLOATS, 11);
  assert.ok(Math.abs(recorder.lastS - 1) < 1e-9);

  const late = createGhostRecorder();
  recordGhostSample(late, 0, [1, 0, 0, ...IDENTITY]);
  recordGhostSample(late, 0.35, [2, 0, 0, ...IDENTITY]);
  // The slots at 0.1, 0.2 and 0.3 s repeat the late frame's pose.
  assert.deepEqual(
    [0, 1, 2, 3].map((slot) => late.poses[slot * GHOST_FLOATS]),
    [1, 2, 2, 2],
  );
});

test('nothing is recorded past the length cap, and short recordings give no run', () => {
  const recorder = createGhostRecorder();
  recordGhostSample(recorder, GHOST_MAX_S + 1, [0, 0, 0, ...IDENTITY]);
  assert.ok(recorder.poses.length / GHOST_FLOATS <= GHOST_MAX_S * 10 + 1);
  resetGhostRecorder(recorder);
  recordGhostSample(recorder, 0, [0, 0, 0, ...IDENTITY]);
  assert.equal(finishGhostRun(recorder, 0), null);
  resetGhostRecorder(recorder);
  assert.equal(recorder.poses.length, 0);
});

test('the better run has more saves, then more time', () => {
  const recorder = createGhostRecorder();
  recordLine(recorder, 3);
  const base = finishGhostRun(recorder, 2);
  assert.ok(base);
  assert.equal(isBetterRun(base, null), true);
  assert.equal(isBetterRun({ ...base, saves: 3 }, base), true);
  assert.equal(isBetterRun({ ...base, saves: 1, durationS: 99 }, base), false);
  assert.equal(isBetterRun({ ...base, durationS: base.durationS + 1 }, base), true);
  assert.equal(isBetterRun(base, base), false);
});

test('a run is stored only when it beats the stored one', () => {
  const storage = memoryStorage();
  const recorder = createGhostRecorder();
  recordLine(recorder, 3);
  const run = finishGhostRun(recorder, 2);
  assert.equal(saveGhostIfBetter(storage, run, 'playground'), true);
  assert.deepEqual(loadGhost(storage, 'playground')?.saves, 2);
  assert.equal(saveGhostIfBetter(storage, { ...run, saves: 1 }, 'playground'), false);
  assert.equal(loadGhost(storage, 'playground')?.saves, 2);
});

test('unknown, broken or blocked storage gives no ghost and never throws', () => {
  for (const raw of [
    'nope',
    'null',
    '{}',
    '{"version":0,"hz":10,"saves":1,"durationS":1,"poses":[]}',
    '{"version":1,"hz":10,"saves":1,"durationS":1,"poses":[1,2,3]}',
    '{"version":1,"hz":10,"saves":1,"durationS":1,"poses":[1,2,3,4,5,6,7,8,9,10,11,12,13,"x"]}',
  ]) {
    assert.equal(
      loadGhost(memoryStorage({ [ghostKey('playground')]: raw }), 'playground'),
      null,
      raw,
    );
  }
  assert.equal(loadGhost(null, 'playground'), null);
  const blocked = {
    getItem: () => {
      throw new Error('blocked');
    },
    setItem: () => {
      throw new Error('blocked');
    },
  };
  assert.equal(loadGhost(blocked, 'playground'), null);
  const recorder = createGhostRecorder();
  recordLine(recorder, 2);
  assert.equal(saveGhostIfBetter(blocked, finishGhostRun(recorder, 1), 'playground'), false);
});

test('replay blends positions and stops at the end of the run', () => {
  const recorder = createGhostRecorder();
  recordLine(recorder, 2);
  const run = finishGhostRun(recorder, 1);
  const out = new Array(GHOST_FLOATS).fill(0);

  assert.equal(sampleGhost(run, 0, out), true);
  assert.equal(out[0], 0);
  assert.equal(sampleGhost(run, 0.5, out), true);
  assert.ok(Math.abs(out[0] - 5) < 1e-9, `x ${out[0]}`);
  assert.equal(sampleGhost(run, 99, out), false);
  assert.equal(sampleGhost(run, -1, out), false);
  assert.equal(sampleGhost(run, Number.NaN, out), false);
});

test('replay rotates the short way round and keeps the quaternion unit length', () => {
  const run = {
    version: 1,
    hz: 10,
    saves: 0,
    durationS: 0.1,
    poses: [0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, -1],
  };
  const out = new Array(GHOST_FLOATS).fill(0);
  assert.equal(sampleGhost(run, 0.05, out), true);
  const length = Math.hypot(out[3], out[4], out[5], out[6]);
  assert.ok(Math.abs(length - 1) < 1e-6);
  assert.ok(Math.abs(out[6]) > 0.99, 'stays near the identity, not through zero');
});

test('a ghost is kept per course and never replays on another one', () => {
  const storage = memoryStorage();
  const recorder = createGhostRecorder();
  recordLine(recorder, 3);
  const run = finishGhostRun(recorder, 2);
  assert.equal(saveGhostIfBetter(storage, run, 'playground'), true);

  assert.ok(loadGhost(storage, 'playground'));
  assert.equal(loadGhost(storage, 'flat'), null);
  assert.equal(loadGhost(storage, courseScope('{"shapes":[]}')), null);
  // The old single global key is not read any more.
  assert.equal(
    loadGhost(memoryStorage({ 'ninelives.ghost.v1': JSON.stringify(run) }), 'playground'),
    null,
  );
  // A better run on another course does not touch this one.
  assert.equal(saveGhostIfBetter(storage, { ...run, saves: 9 }, 'flat'), true);
  assert.equal(loadGhost(storage, 'playground')?.saves, 2);
});

test('course scopes are stable and differ per course text', () => {
  assert.equal(courseScope('abc'), courseScope('abc'));
  assert.notEqual(courseScope('abc'), courseScope('abd'));
  assert.match(courseScope('abc'), /^course-[0-9a-f]+$/);
});
