// Checks the wasm Game API that the browser uses: building, ticking, poses, snapshots.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

import { loadWasm } from './load-wasm.mjs';

const vehicleToml = readFileSync(new URL('../../configs/vehicle.toml', import.meta.url), 'utf8');
const FULL_THROTTLE = new Uint8Array([127, 0, 0, 0]);

test('full throttle moves the car along +X', () => {
  const { Game } = loadWasm();
  const game = new Game(vehicleToml);

  for (let step = 0; step < 240; step += 1) {
    game.tick(FULL_THROTTLE);
  }

  const pose = game.chassis_pose();
  assert.equal(pose.length, 7);
  assert.ok(pose[0] > 5, `x after 2 s: ${pose[0]}`);
  assert.equal(game.wheel_poses().length, 28);
});

test('restoring a snapshot rewinds the car', () => {
  const { Game } = loadWasm();
  const game = new Game(vehicleToml);
  const saved = game.snapshot();

  for (let step = 0; step < 120; step += 1) {
    game.tick(FULL_THROTTLE);
  }
  game.restore(saved);

  assert.equal(game.hud()[1], 0, 'time is back to 0 s');
});

test('a bad config is rejected with the field name', () => {
  const { Game } = loadWasm();
  const broken = vehicleToml.replace('dry_mass_kg = 1800.0', 'dry_mass_kg = -1.0');

  assert.throws(() => new Game(broken), /chassis\.dry_mass_kg/);
});

test('tick needs exactly 4 input bytes', () => {
  const { Game } = loadWasm();
  const game = new Game(vehicleToml);

  assert.throws(() => game.tick(new Uint8Array([1, 2])), /4 input bytes/);
});
