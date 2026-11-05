// Unit tests for the title, headline g peak and stunt-ready hint. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { URLSearchParams } from 'node:url';

import { createGPeakTracker, formatG, updateGPeak } from '../src/ui/g-meter.ts';
import { stuntReadyText } from '../src/ui/stunt-ready.ts';
import { titleWanted } from '../src/ui/title.ts';
import { stuntReadyMaskFromSim } from '../src/wasm-bridge/stunt-adapter.ts';

test('the title shows unless the address asks to skip it', () => {
  assert.equal(titleWanted(new URLSearchParams('')), true);
  assert.equal(titleWanted(new URLSearchParams('world=flat')), true);
  assert.equal(titleWanted(new URLSearchParams('play=1')), false);
  assert.equal(titleWanted(new URLSearchParams('play')), false);
});

test('the headline peak is the smoothed peak, capped by the raw peak', () => {
  const tracker = createGPeakTracker();
  // A one-tick contact spike of 39 g that the 0.1 s smoothing barely shows.
  assert.equal(updateGPeak(tracker, 1, 0), 0);
  assert.equal(updateGPeak(tracker, 2.3, 39.1), 2.3);
  assert.equal(updateGPeak(tracker, 5.8, 39.1), 5.8);
  // Driving afterwards at 1 g keeps the stunt's peak, and never exceeds the raw peak.
  assert.equal(updateGPeak(tracker, 1, 39.1), 5.8);
  assert.equal(formatG(tracker.headlineG), '5.8 g');
});

test('a quiet landing is not inflated by the cap', () => {
  const tracker = createGPeakTracker();
  assert.equal(updateGPeak(tracker, 1.4, 1.2), 1.2);
});

test('the headline peak starts over when the raw peak resets', () => {
  const tracker = createGPeakTracker();
  updateGPeak(tracker, 6, 8);
  assert.equal(tracker.headlineG, 6);
  // New window: the raw peak drops to a small number.
  assert.equal(updateGPeak(tracker, 1, 0.5), 0.5);
  assert.equal(updateGPeak(tracker, 1, 0), 0);
  // Bad numbers read as 0 and do not throw.
  assert.equal(updateGPeak(tracker, Number.NaN, Number.NaN), 0);
});

test('the stunt-ready hint lists the keys a mask allows', () => {
  assert.equal(stuntReadyText(0), '');
  assert.equal(stuntReadyText(0b1111), 'Stunt ready: Q E Z B');
  assert.equal(stuntReadyText(0b0100), 'Stunt ready: Z');
});

test('an older wasm without stunt_ready_mask reads as nothing ready', () => {
  assert.equal(stuntReadyMaskFromSim({}), 0);
  assert.equal(stuntReadyMaskFromSim({ stunt_ready_mask: () => 5 }), 5);
});
