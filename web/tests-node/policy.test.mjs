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

test('Hybrid leaves a parked car to PD and hands a tumbling car to the AI', () => {
  const game = newGame();
  game.load_policy(policyJson);
  game.set_controller('hybrid');
  tickMany(game, new Uint8Array(4), 2); // one control step
  assert.equal(game.hud()[HudField.Controller], ControllerCode.HybridPd);

  // Drop tests start at a random tilt; only one past the upset angle (60°) hands over.
  const seen = new Set();
  for (let seed = 0n; seed < 20n; seed += 1n) {
    game.drop_test(seed);
    tickMany(game, new Uint8Array(4), 2);
    seen.add(game.hud()[HudField.Controller]);
  }

  assert.ok(seen.has(ControllerCode.HybridAi), 'a steep drop reached the AI');
  assert.ok(seen.has(ControllerCode.HybridPd), 'a gentle drop stayed with PD');
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

/** `row` as the rows were before the preset, strength and spin axis fields. */
function withoutNewFields(row) {
  const old = { ...row };
  for (const key of ['preset', 'strength', 'spin_axis']) {
    delete old[key];
  }
  return old;
}

test('wasm runs the same AI episodes as the native eval, bit for bit', () => {
  const committed = JSON.parse(
    readFileSync(new URL('../../results/eval_ai_A_realistic.json', import.meta.url), 'utf8'),
  );

  const rows = JSON.parse(evalEpisodes('ai', 'realistic', 0n, 30, policyJson));

  // The committed rows are the drops before the widening and have no preset fields; the
  // widened default scripts some drops, so compare the plain ones (the rest are new).
  const plain = rows.flatMap((row, index) => (row.preset === null ? [index] : []));
  assert.ok(plain.length >= 15, `${plain.length} plain drops of 30`);
  for (const index of plain) {
    assert.deepEqual(
      asFloat32(withoutNewFields(rows[index])),
      asFloat32(committed.episodes[index]),
    );
  }
});
