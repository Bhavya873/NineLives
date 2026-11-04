// Unit tests for the all-stages training viewer's pure parts (plan: all-stages viewer): the
// panel layout, the snapshot clock, the round seeds, the camera field of view and the arena
// choice (no Gauntlet). Plain Node, no browser.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { ARENAS, arenaFromUrl, arenaLabel, searchWithArena } from '../src/viewer/arena-choice.ts';
import { fovForAspect } from '../src/viewer/panel-camera.ts';
import { layoutPanels } from '../src/viewer/panel-layout.ts';
import { advanceClock, newSnapshotClock, onSnapshot } from '../src/viewer/snapshot-clock.ts';
import { rowSeeds } from '../src/viewer/stage-protocol.ts';

const AREA = { x: 0, y: 0, w: 1000, h: 800 };

test('every stage has a panel and the Gauntlet is gone', () => {
  assert.deepEqual(ARENAS, ['flat', 'side', 'obstacles', 'driving', 'flight']);
  assert.equal(arenaFromUrl('?arena=gauntlet'), null);
  assert.equal(arenaFromUrl('?arena=driving'), 'driving');
  assert.equal(arenaFromUrl(''), null);
  assert.ok(ARENAS.every((kind) => !/gauntlet/i.test(arenaLabel(kind))));
});

test('the URL keeps the focus and drops it for all stages', () => {
  assert.equal(searchWithArena('', 'flight'), '?arena=flight');
  assert.equal(searchWithArena('?arena=flight&camera=leader', null), '?camera=leader');
  assert.equal(searchWithArena('?arena=flight', null), '');
});

test('with no focus the five panels tile the area without overlapping', () => {
  const panels = layoutPanels(AREA, null);
  assert.deepEqual([...panels.keys()].sort(), [...ARENAS].sort());
  const rects = [...panels.values()];
  for (const rect of rects) {
    assert.ok(rect.x >= 0 && rect.y >= 0);
    assert.ok(rect.x + rect.w <= AREA.w + 1e-9 && rect.y + rect.h <= AREA.h + 1e-9);
    assert.ok(rect.w > 100 && rect.h > 100);
  }
  for (let a = 0; a < rects.length; a += 1) {
    for (let b = a + 1; b < rects.length; b += 1) {
      const p = rects[a];
      const q = rects[b];
      const apart =
        p.x + p.w <= q.x + 1e-9 ||
        q.x + q.w <= p.x + 1e-9 ||
        p.y + p.h <= q.y + 1e-9 ||
        q.y + q.h <= p.y + 1e-9;
      assert.ok(apart, `panels ${a} and ${b} overlap`);
    }
  }
});

test('Stage D, the long course, is twice as wide as Stage F', () => {
  const panels = layoutPanels(AREA, null);
  const driving = panels.get('driving');
  const flight = panels.get('flight');
  assert.ok(Math.abs(driving.w - 2 * flight.w) < 1e-9);
});

test('a focused stage fills the area alone', () => {
  const panels = layoutPanels(AREA, 'obstacles');
  assert.deepEqual([...panels.keys()], ['obstacles']);
  assert.deepEqual(panels.get('obstacles'), AREA);
});

test('the clock blends between the last two snapshots and waits at the latest', () => {
  const clock = newSnapshotClock();
  onSnapshot(clock, 0, true);
  onSnapshot(clock, 0.016, false);
  assert.equal(advanceClock(clock, 0), 0);
  assert.ok(Math.abs(advanceClock(clock, 0.008) - 0.5) < 1e-9);
  // Starved of snapshots, the picture holds at the latest one.
  assert.equal(advanceClock(clock, 1), 1);
  assert.equal(advanceClock(clock, 1), 1);
  // A fresh snapshot moves the span on.
  onSnapshot(clock, 0.032, false);
  assert.ok(advanceClock(clock, 0.004) < 1);
});

test('a restart (new round) shows the first snapshot as it is', () => {
  const clock = newSnapshotClock();
  onSnapshot(clock, 5, false);
  onSnapshot(clock, 5.5, false);
  advanceClock(clock, 10);
  onSnapshot(clock, 0, true);
  assert.equal(advanceClock(clock, 0.1), 1);
  assert.equal(clock.displayS, 0);
});

test('a display time that fell behind catches up to the previous snapshot', () => {
  const clock = newSnapshotClock();
  onSnapshot(clock, 0, true);
  onSnapshot(clock, 0.5, false);
  onSnapshot(clock, 1, false);
  assert.ok(clock.displayS >= clock.previousS);
});

test('rows share seeds on the drop arenas and stages, and get a block each on the obstacle arena', () => {
  assert.deepEqual(rowSeeds(40, true, 1, 3, 8), [48n, 48n, 48n]);
  assert.deepEqual(rowSeeds(0, false, 2, 3, 8), [48n, 56n, 64n]);
});

test('a narrow panel widens its field of view; a wide one keeps it', () => {
  assert.equal(fovForAspect(2), 60);
  assert.ok(fovForAspect(0.8) > 60);
  assert.ok(fovForAspect(0.2) <= 100);
});
