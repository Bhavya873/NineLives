// Unit tests for keeping lives across the viewer trip: restoreLives and the run storage.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { RUN_KEY, saveRun, takeSavedRun } from '../src/game/run-storage.ts';
import { LIVES, livesLeft, restoreLives } from '../src/ui/lives.ts';

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
  const lives = { crashesAtStart: 0, freeRespawns: 3 };
  restoreLives(lives, { crashes: 0 }, 4);
  assert.equal(livesLeft(lives, { crashes: 0 }), LIVES - 4);
  // the next crash costs one more
  assert.equal(livesLeft(lives, { crashes: 1 }), LIVES - 5);
});

test('restoreLives ignores nonsense and never leaves zero lives', () => {
  const lives = { crashesAtStart: 0, freeRespawns: 0 };
  restoreLives(lives, { crashes: 0 }, Number.NaN);
  assert.equal(livesLeft(lives, { crashes: 0 }), LIVES);
  restoreLives(lives, { crashes: 0 }, 99);
  assert.equal(livesLeft(lives, { crashes: 0 }), 1);
});

test('a saved run is read once; none or a finished run is fresh', () => {
  const storage = memoryStorage();
  saveRun(storage, 3, 6);
  assert.equal(takeSavedRun(storage), 3);
  assert.equal(takeSavedRun(storage), 0, 'taken out of storage');
  saveRun(storage, 9, 0);
  assert.equal(storage.data.has(RUN_KEY), false);
  assert.equal(takeSavedRun(storage), 0);
  assert.equal(takeSavedRun(null), 0);
});
