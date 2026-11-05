// Unit tests for the title, results, run stats, headline g peak and stunt-ready hint. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { URLSearchParams } from 'node:url';

import { createGPeakTracker, formatG, updateGPeak } from '../src/ui/g-meter.ts';
import {
  createRunStats,
  recordRunEvents,
  recordRunPeaks,
  resetRunStats,
  resultsRows,
  stuntBreakdown,
  stuntTotal,
} from '../src/ui/run-stats.ts';
import { stuntReadyText } from '../src/ui/stunt-ready.ts';
import { titleWanted } from '../src/ui/title.ts';
import { stuntReadyMaskFromSim } from '../src/wasm-bridge/stunt-adapter.ts';

const FLOATS = 5;
function event(kind, ...rest) {
  const out = new Float32Array(FLOATS);
  out[0] = kind;
  rest.forEach((value, index) => {
    out[index + 1] = value;
  });
  return out;
}
function concat(...events) {
  const out = new Float32Array(events.length * FLOATS);
  events.forEach((e, index) => out.set(e, index * FLOATS));
  return out;
}

function values(overrides = {}) {
  return { saves: 0, airUsedKg: 0, timeS: 0, ...overrides };
}

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

test('stunt landings are counted per stunt and named', () => {
  const stats = createRunStats(values());
  recordRunEvents(
    stats,
    concat(
      event(12, 3, 4.2, 6.28),
      event(12, 3, 3.1, 6.28),
      event(12, 4, 3, 6.28),
      event(13, 1, 1),
    ),
  );

  assert.equal(stuntTotal(stats), 3);
  assert.equal(stuntBreakdown(stats), '2 Front flip, 1 Back flip');
  assert.equal(stuntBreakdown(createRunStats(values())), 'none');
});

test('results rows count from the start of the run', () => {
  const stats = createRunStats(values({ saves: 4, airUsedKg: 10, timeS: 100 }));
  recordRunPeaks(stats, 5.8, 39.1);
  recordRunPeaks(stats, 2, 3);
  recordRunEvents(stats, concat(event(12, 3, 4.2, 6.28)));
  const rows = Object.fromEntries(
    resultsRows(stats, values({ saves: 7, airUsedKg: 12.5, timeS: 165 }), 39.1),
  );

  assert.equal(rows['Saves'], '3');
  assert.equal(rows['Air used'], '2.5 kg');
  assert.equal(rows['Peak g'], '5.8 g (contact spike 39.1 g)');
  assert.equal(rows['Stunts landed'], '1 (1 Front flip)');
  assert.match(rows['Time'], /1:05|01:05|65/);
  assert.equal(rows['Best spike ever'], '39.1 g');

  resetRunStats(stats, values({ saves: 7, airUsedKg: 12.5, timeS: 165 }));
  assert.equal(stuntTotal(stats), 0);
  assert.equal(stats.peakG, 0);
  assert.equal(resultsRows(stats, values({ saves: 7, airUsedKg: 12.5, timeS: 165 }), 0)[0][1], '0');
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
