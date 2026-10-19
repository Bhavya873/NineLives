// Game API tests for the controllers (SPEC §8, §11.1, §11.3): Off fires nothing, PD saves
// air on its wheels, Manual fires the nozzle whose key is held, and unknown names are
// refused.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  ControllerCode,
  Game,
  HudField,
  controllersToml,
  modesToml,
  tickMany,
  vehicleToml,
} from './load-wasm.mjs';

/** An input frame holding the manual button for `nozzle`. */
function holdingNozzle(nozzle) {
  const buttons = 1 << (Game.first_manual_nozzle_bit() + nozzle);
  return new Uint8Array([0, 0, buttons & 0xff, buttons >> 8]);
}

test('the game starts with the Off controller, which ignores the nozzle keys', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);

  tickMany(game, holdingNozzle(4), 30);

  assert.equal(game.hud()[HudField.Controller], ControllerCode.Off);
  assert.deepEqual(Array.from(game.nozzle_throttles()), new Array(10).fill(0));
});

test('Manual fires only the nozzle whose key is held', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  game.set_controller('manual');
  // In the air: on the ground, steady, the thruster interlock cuts every controller.
  game.drop_test(0n);

  tickMany(game, holdingNozzle(4), 30);

  const throttles = game.nozzle_throttles();
  assert.equal(game.hud()[HudField.Controller], ControllerCode.Manual);
  assert.ok(throttles[4] > 0.5, `S_FL throttle ${throttles[4]}`);
  assert.equal(throttles[0], 0);
});

test('switching back to Off closes the valves Manual left open', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  game.set_controller('manual');
  tickMany(game, holdingNozzle(0), 30);

  game.set_controller('off');
  tickMany(game, holdingNozzle(0), 60);

  assert.ok(game.nozzle_throttles()[0] < 0.01);
});

test('an unknown controller name is refused with a helpful message', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);

  assert.throws(() => game.set_controller('autopilot'), /use one of: off, pd, ai, manual/);
});

test('choosing the AI before a policy is loaded is refused with a helpful message', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);

  assert.throws(() => game.set_controller('ai'), /needs a policy/);
});

test('without a policy, cycling the controller goes Off, PD, Manual and back to Off', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);

  game.cycle_controller();
  assert.equal(game.hud()[HudField.Controller], ControllerCode.Pd);
  game.cycle_controller();
  assert.equal(game.hud()[HudField.Controller], ControllerCode.Manual);
  game.cycle_controller();

  assert.equal(game.hud()[HudField.Controller], ControllerCode.Off);
});

test('PD fires nothing while the car sits on its wheels, to save air', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  game.set_controller('pd');

  tickMany(game, new Uint8Array(4), 240);

  assert.equal(game.hud()[HudField.Controller], ControllerCode.Pd);
  assert.deepEqual(Array.from(game.nozzle_throttles()), new Array(10).fill(0));
});

test('a nozzle tap that lands between two control steps still fires', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  game.set_controller('manual');
  game.drop_test(0n); // in the air, where the interlock never cuts the controller
  game.tick(new Uint8Array(4)); // control step; the next tick is not one

  game.tick(holdingNozzle(4)); // the tap
  tickMany(game, new Uint8Array(4), 3);

  assert.ok(game.nozzle_throttles()[4] > 0, 'the tap reached the Manual controller');
});

/** The highest nozzle throttle over `ticks` idle ticks after one Chaos knock. */
function peakThrottleAfterKnock(game, ticks) {
  const chaos = Game.button_chaos();
  game.tick(new Uint8Array([0, 0, chaos & 0xff, chaos >> 8]));
  let peak = 0;
  for (let tick = 0; tick < ticks; tick += 1) {
    game.tick(new Uint8Array(4));
    peak = Math.max(peak, ...game.nozzle_throttles());
  }
  return peak;
}

test('after a Chaos knock PD fires to steady the car, where Off fires nothing', () => {
  // Seed 2's knock lifts wheels off the ground; the same knock for both controllers.
  const peaks = {};
  for (const kind of ['off', 'pd']) {
    const game = new Game(vehicleToml, modesToml, controllersToml, 2n);
    game.set_controller(kind);
    tickMany(game, new Uint8Array(4), 120);
    peaks[kind] = peakThrottleAfterKnock(game, 240);
  }

  assert.equal(peaks.off, 0);
  assert.ok(peaks.pd > 0.1, `PD peak throttle ${peaks.pd}`);
});
