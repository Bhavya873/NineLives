// Game API tests for the controllers (SPEC §8, §11.1, §11.3): Off fires nothing, Manual
// fires the nozzle whose key is held, and unknown names are refused.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { ControllerCode, Game, HudField, modesToml, vehicleToml } from './load-wasm.mjs';

/** An input frame holding the manual button for `nozzle`. */
function holdingNozzle(nozzle) {
  const buttons = 1 << (Game.first_manual_nozzle_bit() + nozzle);
  return new Uint8Array([0, 0, buttons & 0xff, buttons >> 8]);
}

function tickMany(game, input, count) {
  for (let tick = 0; tick < count; tick += 1) {
    game.tick(input);
  }
}

test('the game starts with the Off controller, which ignores the nozzle keys', () => {
  const game = new Game(vehicleToml, modesToml, 0n);

  tickMany(game, holdingNozzle(4), 30);

  assert.equal(game.hud()[HudField.Controller], ControllerCode.Off);
  assert.deepEqual(Array.from(game.nozzle_throttles()), new Array(10).fill(0));
});

test('Manual fires only the nozzle whose key is held', () => {
  const game = new Game(vehicleToml, modesToml, 0n);
  game.set_controller('manual');

  tickMany(game, holdingNozzle(4), 30);

  const throttles = game.nozzle_throttles();
  assert.equal(game.hud()[HudField.Controller], ControllerCode.Manual);
  assert.ok(throttles[4] > 0.5, `S_FL throttle ${throttles[4]}`);
  assert.equal(throttles[0], 0);
});

test('switching back to Off closes the valves Manual left open', () => {
  const game = new Game(vehicleToml, modesToml, 0n);
  game.set_controller('manual');
  tickMany(game, holdingNozzle(0), 30);

  game.set_controller('off');
  tickMany(game, holdingNozzle(0), 60);

  assert.ok(game.nozzle_throttles()[0] < 0.01);
});

test('an unknown controller name is refused with a helpful message', () => {
  const game = new Game(vehicleToml, modesToml, 0n);

  assert.throws(() => game.set_controller('autopilot'), /use "off" or "manual"/);
});
