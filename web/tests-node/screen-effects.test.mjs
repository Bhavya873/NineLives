// Unit tests for the HUD screen effects: the wind arrow's
// angle and label, the wind streaks' wrapping and the wind level setting. Plain Node.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { URLSearchParams } from 'node:url';

import { cameraYawRad, windArrowAngleRad, windLabel } from '../src/ui/wind-indicator.ts';
import { wrap } from '../src/render/wind-streaks.ts';
import { nextWindLevel, startingWindLevel } from '../src/render/screen-effects.ts';

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

test('the wind level comes from ?wind= and V cycles Off, Breeze, Storm', () => {
  assert.equal(startingWindLevel(new URLSearchParams('wind=storm')), 'storm');
  assert.equal(startingWindLevel(new URLSearchParams('wind=hurricane')), 'off');
  assert.equal(startingWindLevel(new URLSearchParams('')), 'off');
  assert.equal(nextWindLevel('off'), 'breeze');
  assert.equal(nextWindLevel('breeze'), 'storm');
  assert.equal(nextWindLevel('storm'), 'off');
});
