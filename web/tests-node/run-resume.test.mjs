// Unit tests for keeping lives across the viewer trip: restoreLives and the run storage.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { RUN_KEY, saveRun, takeSavedRun } from '../src/game/run-storage.ts';
import { LIVES, livesLeft, restoreLives, savesThisRun } from '../src/ui/lives.ts';

const none = { lostLives: 0, saves: 0 };

function memoryStorage() {
  const data = new Map();
  return {
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => data.set(key, value),
    removeItem: (key) => data.delete(key),
    data,
  };
}

test('restoreLives brings back the lost lives on a fresh sim', () => {
  const lives = { crashesAtStart: 0, freeRespawns: 3, savesAtStart: 0 };
  restoreLives(lives, { crashes: 0, saves: 0 }, 4, 7);
  assert.equal(savesThisRun(lives, { saves: 2 }), 9, 'the saves made before carry on');
  assert.equal(livesLeft(lives, { crashes: 0 }), LIVES - 4);
  // the next crash costs one more
  assert.equal(livesLeft(lives, { crashes: 1 }), LIVES - 5);
});

test('restoreLives ignores nonsense and never leaves zero lives', () => {
  const lives = { crashesAtStart: 0, freeRespawns: 0, savesAtStart: 0 };
  restoreLives(lives, { crashes: 0, saves: 0 }, Number.NaN, Number.NaN);
  assert.equal(livesLeft(lives, { crashes: 0 }), LIVES);
  restoreLives(lives, { crashes: 0, saves: 0 }, 99);
  assert.equal(livesLeft(lives, { crashes: 0 }), 1);
});

test('a saved run is read once; none or a finished run is fresh', () => {
  const storage = memoryStorage();
  saveRun(storage, { lostLives: 3, saves: 5 }, 6);
  assert.deepEqual(takeSavedRun(storage), { lostLives: 3, saves: 5 });
  assert.deepEqual(takeSavedRun(storage), none, 'taken out of storage');
  saveRun(storage, { lostLives: 9, saves: 4 }, 0);
  assert.equal(storage.data.has(RUN_KEY), false);
  assert.deepEqual(takeSavedRun(storage), none);
  assert.deepEqual(takeSavedRun(null), none);
});
