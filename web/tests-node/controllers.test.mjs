// Game API tests for the controllers (SPEC §8, §11.1, §11.3): Off fires nothing, Manual
// fires the nozzle whose key is held, and unknown names are refused.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { ControllerCode, Game, HudField, modesToml, tickMany, vehicleToml } from './load-wasm.mjs';

/** An input frame holding the manual button for `nozzle`. */
function holdingNozzle(nozzle) {
  const buttons = 1 << (Game.first_manual_nozzle_bit() + nozzle);
  return new Uint8Array([0, 0, buttons & 0xff, buttons >> 8]);
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

  assert.throws(() => game.set_controller('autopilot'), /use one of: off, manual/);
});

test('cycling the controller goes Off, Manual and back to Off', () => {
  const game = new Game(vehicleToml, modesToml, 0n);

  game.cycle_controller();
  assert.equal(game.hud()[HudField.Controller], ControllerCode.Manual);
  game.cycle_controller();

  assert.equal(game.hud()[HudField.Controller], ControllerCode.Off);
});

test('a nozzle tap that lands between two control steps still fires', () => {
  const game = new Game(vehicleToml, modesToml, 0n);
  game.set_controller('manual');
  game.tick(new Uint8Array(4)); // control step; the next tick is not one

  game.tick(holdingNozzle(4)); // the tap
  tickMany(game, new Uint8Array(4), 3);

  assert.ok(game.nozzle_throttles()[4] > 0, 'the tap reached the Manual controller');
});
