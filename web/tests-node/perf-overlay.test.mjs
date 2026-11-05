// Unit tests for the F3 readout (SPEC §11.7): frames per second and the mean
// milliseconds per tick and per drawn frame. Plain Node, no browser.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { perfText } from '../src/ui/perf-overlay.ts';

test('30 frames in half a second read as 60 fps with mean tick and render times', () => {
  // 60 ticks took 30 ms in all (0.5 ms each); 30 frames took 120 ms (4 ms each).
  assert.equal(perfText(30, 500, 60, 30, 120), '60 fps, tick 0.50 ms, render 4.0 ms');
});

test('a window without ticks shows zero ms per tick, not NaN', () => {
  assert.equal(perfText(1, 1000, 0, 0, 5), '1 fps, tick 0.00 ms, render 5.0 ms');
});
