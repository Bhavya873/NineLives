// Checks the wasm Game API that the browser uses: building, ticking, poses, snapshots.
// It loads the wasm build from web/src/wasm, so run `npm run wasm` first.
// Run with `npm run test:wasm` (CI runs it in the web job).
import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  EventKind,
  Game,
  HopBlocked,
  HudField,
  ModeCode,
  modesToml,
  vehicleToml,
} from './load-wasm.mjs';

const FULL_THROTTLE = new Uint8Array([127, 0, 0, 0]);

function tickMany(game, input, count) {
  for (let step = 0; step < count; step += 1) {
    game.tick(input);
  }
}

test('full throttle moves the car along +X', () => {
  const game = new Game(vehicleToml, modesToml, 0n);

  tickMany(game, FULL_THROTTLE, 240);

  const pose = game.chassis_pose();
  assert.equal(pose.length, Game.floats_per_pose());
  assert.ok(pose[0] > 5, `x after 2 s: ${pose[0]}`);
  assert.equal(game.wheel_poses().length, 4 * Game.floats_per_pose());
  assert.deepEqual(Array.from(game.nozzle_throttles()), new Array(Game.nozzle_count()).fill(0));
});

test('the mode comes back with a restored snapshot', () => {
  const game = new Game(vehicleToml, modesToml, 0n);
  game.set_mode('arcade');
  const saved = game.snapshot();

  game.set_mode('realistic');
  game.restore(saved);

  assert.equal(game.hud()[HudField.Mode], ModeCode.Arcade);
});

test('restoring a snapshot puts the car and the clock back exactly', () => {
  const game = new Game(vehicleToml, modesToml, 0n);
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

  assert.throws(() => new Game(broken, modesToml, 0n), /chassis\.dry_mass_kg/);
});

test('tick needs exactly 4 input bytes', () => {
  const game = new Game(vehicleToml, modesToml, 0n);

  assert.throws(() => game.tick(new Uint8Array([1, 2])), /4 input bytes/);
});

test('switching to arcade gives unlimited air; unknown modes are refused', () => {
  const game = new Game(vehicleToml, modesToml, 0n);

  game.set_mode('arcade');

  assert.equal(game.hud()[HudField.Mode], ModeCode.Arcade);
  assert.equal(game.hud()[HudField.UnlimitedAir], 1);
  assert.equal(game.hud()[HudField.AirFraction], 1);
  assert.throws(() => game.set_mode('turbo'), /unknown mode/);
});

test('space-bar hop lifts the car once it has settled', () => {
  const game = new Game(vehicleToml, modesToml, 0n);
  tickMany(game, new Uint8Array([0, 0, 0, 0]), 180);
  assert.equal(game.hud()[HudField.HopBlocked], HopBlocked.No);

  game.tick(new Uint8Array([0, 0, 1, 0]));
  tickMany(game, new Uint8Array([0, 0, 0, 0]), 60);

  assert.ok(game.chassis_pose()[1] > 1.2, `height ${game.chassis_pose()[1]}`);
});

/** Splits the flat drain_events() array into [kind, a, b, c] groups. */
function eventGroups(values) {
  const groups = [];
  for (let start = 0; start < values.length; start += Game.floats_per_event()) {
    groups.push(Array.from(values.slice(start, start + Game.floats_per_event())));
  }
  return groups;
}

test('a hop reports airborne, upset and save events', () => {
  const game = new Game(vehicleToml, modesToml, 0n);
  tickMany(game, new Uint8Array([0, 0, 0, 0]), 180);
  game.drain_events();

  game.tick(new Uint8Array([0, 0, 1, 0]));
  tickMany(game, new Uint8Array([0, 0, 0, 0]), 360);
  const kinds = eventGroups(game.drain_events()).map((group) => group[0]);

  assert.deepEqual(kinds, [
    EventKind.AirborneStart,
    EventKind.UpsetStart,
    EventKind.AirborneEnd,
    EventKind.Save,
  ]);
  assert.equal(game.is_crashed(), false);
});

test('the Reset button respawns at the start with a full tank and keeps the mode', () => {
  const game = new Game(vehicleToml, modesToml, 0n);
  game.set_mode('arcade');
  tickMany(game, new Uint8Array([127, 0, 2, 0]), 240);
  game.set_mode('realistic');
  tickMany(game, new Uint8Array([127, 0, 2, 0]), 60);
  game.drain_events();

  game.tick(new Uint8Array([0, 0, Game.button_reset(), 0]));

  // The start pose is a little above resting height, so an AirborneStart may follow.
  const kinds = eventGroups(game.drain_events()).map((group) => group[0]);
  assert.equal(kinds[0], EventKind.Respawn);
  assert.ok(Math.abs(game.chassis_pose()[0]) < 0.5, `x ${game.chassis_pose()[0]}`);
  assert.equal(game.hud()[HudField.AirFraction], 1);
  assert.equal(game.hud()[HudField.Mode], ModeCode.Realistic);
  assert.equal(game.hud()[HudField.Crashes], 0);
});
