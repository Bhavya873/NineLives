// Unit tests for the swipe fallback: 40 px minimum, then the longer direction picks the stunt
// the way the A S D F keys would. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { StuntCode } from '../src/game/stunt-requests.ts';
import { MIN_SWIPE_PX, classifySwipe } from '../src/ui/swipe-stunts.ts';

test('a swipe shorter than 40 px is a tap', () => {
  assert.equal(MIN_SWIPE_PX, 40);
  assert.equal(classifySwipe(39, 0), null);
  assert.equal(classifySwipe(-20, 30), null);
});

test('left and right are barrel rolls, up is the front flip and down is the back flip', () => {
  assert.equal(classifySwipe(-60, 5), StuntCode.BarrelLeft);
  assert.equal(classifySwipe(60, -5), StuntCode.BarrelRight);
  assert.equal(classifySwipe(5, -60), StuntCode.FrontFlip);
  assert.equal(classifySwipe(-5, 60), StuntCode.BackFlip);
});

test('a swipe of exactly 40 px counts', () => {
  assert.equal(classifySwipe(40, 0), StuntCode.BarrelRight);
});
