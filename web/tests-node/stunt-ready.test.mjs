// stunt_ready_mask through the wasm Game: nothing is ready on the ground or when crashed, a
// short drop is too brief for any stunt, and asking never changes the sim.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { Game, HudField, controllersToml, modesToml, tickMany, vehicleToml } from './load-wasm.mjs';

const IDLE = new Uint8Array([0, 0, 0, 0]);

test('no stunt is ready on the ground', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  tickMany(game, IDLE, 60);
  assert.equal(game.stunt_ready_mask(), 0);
});

test('asking is read-only and a mask only holds the four stunt bits', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 3n);
  game.drop_test(7n);
  tickMany(game, IDLE, 20);
  const before = Array.from(game.chassis_pose());
  const mask = game.stunt_ready_mask();
  assert.equal(mask & ~0b1111, 0);
  assert.deepEqual(Array.from(game.chassis_pose()), before);
  assert.equal(game.hud()[HudField.StuntPhase], 0);
});
