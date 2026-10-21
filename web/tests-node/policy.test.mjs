// The AI in the browser build (SPEC §11.1, M6): load_policy checks the schema hash, the AI
// controller can be chosen once a policy is loaded, a drop test throws the car into the
// air, and wasm runs exactly the same episodes as the native eval (results/*.json).
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

import {
  ControllerCode,
  HudField,
  asFloat32,
  evalEpisodes,
  newGame,
  policyJson,
  tickMany,
} from './load-wasm.mjs';

test('the AI cannot be chosen until a policy is loaded, then it can', () => {
  const game = newGame();
  assert.equal(game.has_policy(), false);
  assert.throws(() => game.set_controller('ai'), /needs a policy/);

  game.load_policy(policyJson);
  game.set_controller('ai');

  assert.equal(game.has_policy(), true);
  assert.equal(game.hud()[HudField.Controller], ControllerCode.Ai);
});

test('with a policy loaded, T cycles PD, AI, Hybrid, Off from the PD start', () => {
  const game = newGame();
  game.load_policy(policyJson);

  const seen = [];
  for (let press = 0; press < 4; press += 1) {
    game.cycle_controller();
    seen.push(game.hud()[HudField.Controller]);
  }

  assert.deepEqual(seen, [
    ControllerCode.Ai,
    ControllerCode.HybridPd,
    ControllerCode.Off,
    ControllerCode.Pd,
  ]);
});

test('Hybrid leaves a parked car to PD and hands a dropped car to the AI', () => {
  const game = newGame();
  game.load_policy(policyJson);
  game.set_controller('hybrid');
  // A new car drops onto its wheels at the start; the AI hands back once it has settled.
  tickMany(game, new Uint8Array(4), 240); // 2 s
  assert.equal(game.hud()[HudField.Controller], ControllerCode.HybridPd);

  game.drop_test(5n);
  tickMany(game, new Uint8Array(4), 2); // one control step

  assert.equal(game.hud()[HudField.Controller], ControllerCode.HybridAi);
});

test('a policy made for another observation layout is refused, naming the hash', () => {
  const game = newGame();
  const stale = JSON.parse(policyJson);
  stale.obs_schema_hash = '0000000000000000';

  assert.throws(() => game.load_policy(JSON.stringify(stale)), /obs_schema_hash/);
  assert.equal(game.has_policy(), false);
});

test('a drop test throws the car into the air', () => {
  const game = newGame();

  game.drop_test(5n);

  const heightM = game.chassis_pose()[1];
  assert.ok(heightM > 1.0, `car centre at ${heightM} m`);
});

test('wasm runs the same AI episodes as the native eval, bit for bit', () => {
  const committed = JSON.parse(
    readFileSync(new URL('../../results/eval_ai_A_realistic.json', import.meta.url), 'utf8'),
  );

  const rows = JSON.parse(evalEpisodes('ai', 'realistic', 0n, 30, policyJson));

  assert.deepEqual(rows.map(asFloat32), committed.episodes.slice(0, 30).map(asFloat32));
});
