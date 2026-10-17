// Checks the wasm Game API that the browser uses: building, ticking, poses, snapshots.
// It loads the wasm build from web/src/wasm, so run `npm run wasm` first.
// Run with `npm run test:wasm` (CI runs it in the web job).
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { Game, HudField, vehicleToml } from './load-wasm.mjs';

const FULL_THROTTLE = new Uint8Array([127, 0, 0, 0]);

function tickMany(game, input, count) {
  for (let step = 0; step < count; step += 1) {
    game.tick(input);
  }
}

test('full throttle moves the car along +X', () => {
  const game = new Game(vehicleToml);

  tickMany(game, FULL_THROTTLE, 240);

  const pose = game.chassis_pose();
  assert.equal(pose.length, Game.floats_per_pose());
  assert.ok(pose[0] > 5, `x after 2 s: ${pose[0]}`);
  assert.equal(game.wheel_poses().length, 4 * Game.floats_per_pose());
  assert.deepEqual(Array.from(game.nozzle_throttles()), new Array(Game.nozzle_count()).fill(0));
});

test('restoring a snapshot puts the car and the clock back exactly', () => {
  const game = new Game(vehicleToml);
  tickMany(game, FULL_THROTTLE, 60);
  const saved = game.snapshot();
  const savedPose = Array.from(game.chassis_pose());
  const savedWheels = Array.from(game.wheel_poses());
  const savedTime = game.hud()[HudField.TimeS];

  tickMany(game, FULL_THROTTLE, 120);
  game.restore(saved);

  assert.deepEqual(Array.from(game.chassis_pose()), savedPose);
  assert.deepEqual(Array.from(game.wheel_poses()), savedWheels);
  assert.equal(game.hud()[HudField.TimeS], savedTime);
});

test('a bad config is rejected with the field name', () => {
  const broken = vehicleToml.replace('dry_mass_kg = 1800.0', 'dry_mass_kg = -1.0');

  assert.throws(() => new Game(broken), /chassis\.dry_mass_kg/);
});

test('tick needs exactly 4 input bytes', () => {
  const game = new Game(vehicleToml);

  assert.throws(() => game.tick(new Uint8Array([1, 2])), /4 input bytes/);
});
