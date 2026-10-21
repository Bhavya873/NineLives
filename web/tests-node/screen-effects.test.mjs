// Unit tests for the HUD screen effects: the g-force blackout's timing, the stand-in g
// estimate, the wind arrow's angle and label, and the wind streaks' wrapping. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { URLSearchParams } from 'node:url';

import {
  applyGBlackout,
  blackoutLook,
  createGBlackoutView,
  newGBlackoutState,
  updateGBlackout,
} from '../src/hud/g-blackout.ts';
import { newGLoadEstimator, updateGLoadEstimate } from '../src/hud/g-load-estimate.ts';
import { cameraYawRad, windArrowAngleRad, windLabel } from '../src/hud/wind-indicator.ts';
import { wrap } from '../src/render/wind-streaks.ts';
import { previewWind } from '../src/screen-effects.ts';

const FRAME_S = 1 / 60;

/** Runs the blackout at `gLoad` for `seconds` and returns the final darkness. */
function holdG(state, gLoad, seconds) {
  let darkness = state.darkness;
  for (let step = 0; step < Math.round(seconds / FRAME_S); step += 1) {
    darkness = updateGBlackout(state, gLoad, FRAME_S);
  }
  return darkness;
}

test('the view stays clear at or below 3 g', () => {
  const state = newGBlackoutState();

  assert.equal(holdG(state, 3, 2), 0);
  assert.equal(holdG(state, 1, 2), 0);
});

test('the edges darken at once above 3 g, more for more g', () => {
  const mild = updateGBlackout(newGBlackoutState(), 4, FRAME_S);
  const hard = updateGBlackout(newGBlackoutState(), 6, FRAME_S);

  assert.ok(mild > 0.2, `4 g darkens at once (${mild})`);
  assert.ok(hard > mild, '6 g darkens more than 4 g');
  assert.ok(hard < 1, 'one frame of g alone never blacks out fully');
});

test('holding just over 3 g builds to full black in about a second', () => {
  const state = newGBlackoutState();

  assert.ok(holdG(state, 3.1, 0.5) < 0.7, 'not black after half a second');
  assert.equal(holdG(state, 3.1, 0.6), 1, 'black after about a second');
});

test('below 3 g the view recovers from black over 1.5 s', () => {
  const state = { darkness: 1 };

  const afterOneSecond = holdG(state, 1, 1);
  const afterTwoSeconds = holdG(state, 1, 1);

  assert.ok(afterOneSecond > 0.2 && afterOneSecond < 0.5, `partly back (${afterOneSecond})`);
  assert.equal(afterTwoSeconds, 0);
});

test('the vignette is clear in the middle when light and solid black when full', () => {
  const light = blackoutLook(0.2, 0);
  const full = blackoutLook(1, 0);

  assert.equal(light.fillOpacity, 0);
  assert.match(light.gradient, /rgb\(0 0 0 \/ 0\) 56\.0%/);
  assert.equal(full.fillOpacity, 1);
});

test('with reduced motion the overlay darkens but never throbs', () => {
  const element = { hidden: true, style: { backgroundImage: '', backgroundColor: '' } };
  const view = createGBlackoutView(element, true);

  applyGBlackout(view, 0.8, 0.0);
  const first = element.style.backgroundImage;
  applyGBlackout(view, 0.8, 0.3);

  assert.equal(element.hidden, false);
  assert.equal(element.style.backgroundImage, first);
});

test('the overlay throbs while dark without reduced motion, and hides when clear', () => {
  const element = { hidden: true, style: { backgroundImage: '', backgroundColor: '' } };
  const view = createGBlackoutView(element, false);

  applyGBlackout(view, 0.8, 0.0);
  const first = element.style.backgroundImage;
  applyGBlackout(view, 0.8, 0.15);
  const later = element.style.backgroundImage;
  applyGBlackout(view, 0, 0.3);

  assert.notEqual(later, first);
  assert.equal(element.hidden, true);
});

test('the g estimate reads 1 g at rest and about 0 g in free fall', () => {
  const dtS = 1 / 120;
  const resting = newGLoadEstimator();
  const falling = newGLoadEstimator();

  for (let tick = 0; tick < 120; tick += 1) {
    const timeS = tick * dtS;
    updateGLoadEstimate(resting, [5, 0.6, 2], dtS);
    updateGLoadEstimate(falling, [0, 100 - 0.5 * 9.81 * timeS * timeS, 0], dtS);
  }

  assert.ok(Math.abs(resting.gLoad - 1) < 1e-6);
  assert.ok(falling.gLoad < 0.05, `free fall reads ${falling.gLoad} g`);
});

test('a respawn jump is not read as g', () => {
  const dtS = 1 / 120;
  const estimator = newGLoadEstimator();
  for (let tick = 0; tick < 30; tick += 1) {
    updateGLoadEstimate(estimator, [0, 0.6, 0], dtS);
  }

  updateGLoadEstimate(estimator, [40, 3, 0], dtS);

  assert.equal(estimator.gLoad, 1);
});

test('camera yaw is 0 looking along +X and a quarter turn looking along -Z', () => {
  assert.ok(Math.abs(cameraYawRad(1, 0)) < 1e-9);
  assert.ok(Math.abs(cameraYawRad(0, -1) - Math.PI / 2) < 1e-9);
});

test('the wind arrow points up for a tailwind and right for wind to the camera right', () => {
  const lookingAlongX = 0;

  // Air moving the way the camera looks: away from the viewer, straight up the screen.
  assert.equal(windArrowAngleRad(8, 0, lookingAlongX), 0);
  // Looking along +X, the right is +Z.
  assert.ok(Math.abs(windArrowAngleRad(0, 5, lookingAlongX) - Math.PI / 2) < 1e-9);
  // Toward the camera: straight down the screen.
  assert.ok(Math.abs(Math.abs(windArrowAngleRad(-3, 0, lookingAlongX)) - Math.PI) < 1e-9);
});

test('the wind arrow turns against the camera: the same wind looks different after a turn', () => {
  // The camera turns a quarter left to look along -Z; wind along +X is now on its right.
  const yaw = cameraYawRad(0, -1);

  assert.ok(Math.abs(windArrowAngleRad(8, 0, yaw) - Math.PI / 2) < 1e-9);
});

test('the wind label says Off, Breeze or Storm with the speed', () => {
  assert.equal(windLabel(0), 'Off');
  assert.equal(windLabel(6.2), 'Breeze 6 m/s');
  assert.equal(windLabel(20), 'Storm 20 m/s');
});

test('streaks leaving the box come back on the other side', () => {
  assert.equal(wrap(19, -18, 18), -17);
  assert.equal(wrap(-19, -18, 18), 17);
  assert.equal(wrap(3, -18, 18), 3);
});

test('the wind preview reads two numbers and ignores anything else', () => {
  assert.deepEqual(previewWind(new URLSearchParams('wind=8,-3')), [8, 0, -3]);
  assert.deepEqual(previewWind(new URLSearchParams('wind=500,0')), [60, 0, 0]);
  assert.deepEqual(previewWind(new URLSearchParams('wind=abc')), [0, 0, 0]);
  assert.deepEqual(previewWind(new URLSearchParams('')), [0, 0, 0]);
});
