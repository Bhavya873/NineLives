// Unit tests for the viewer's resolution step-down. Plain Node, no browser.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  MIN_PIXEL_RATIO,
  QUALITY_WINDOW_FRAMES,
  newQuality,
  recordQualityFrame,
} from '../src/viewer/adaptive-quality.ts';

function feed(state, frameMs, frames) {
  let last = null;
  for (let i = 0; i < frames; i += 1) {
    last = recordQualityFrame(state, frameMs) ?? last;
  }
  return last;
}

test('fast frames never change the ratio', () => {
  const state = newQuality(1.5);
  assert.equal(feed(state, 16.7, QUALITY_WINDOW_FRAMES * 5), null);
  assert.equal(state.ratio, 1.5);
});

test('a slow window steps the ratio down once per window', () => {
  const state = newQuality(1.5);
  assert.equal(feed(state, 40, QUALITY_WINDOW_FRAMES - 1), null);
  assert.equal(recordQualityFrame(state, 40), 1.25);
  assert.equal(feed(state, 40, QUALITY_WINDOW_FRAMES), 1.0);
});

test('the ratio stops at the floor', () => {
  const state = newQuality(1.0);
  feed(state, 80, QUALITY_WINDOW_FRAMES * 10);
  assert.equal(state.ratio, MIN_PIXEL_RATIO);
  assert.equal(feed(state, 80, QUALITY_WINDOW_FRAMES * 2), null);
});

test('a long gap (hidden tab, pause) is not counted as slowness', () => {
  const state = newQuality(1.5);
  let last = null;
  for (let frame = 0; frame < 200; frame += 1) {
    last = recordQualityFrame(state, frame === 10 ? 5000 : 16) ?? last;
  }
  assert.equal(last, null);
  assert.equal(state.ratio, 1.5);
});
