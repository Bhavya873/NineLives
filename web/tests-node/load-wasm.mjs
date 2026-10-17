// Loads the wasm build from web/src/wasm in Node, without a browser or a server.
// Shared by the Node tests and scripts/measure-tick.mjs. Run `npm run wasm` (or
// `npm run wasm:release`) first.
import { readFileSync } from 'node:fs';

import * as wasm from '../src/wasm/nl_wasm.js';

wasm.initSync({ module: readFileSync(new URL('../src/wasm/nl_wasm_bg.wasm', import.meta.url)) });

/** The wasm module's exports (Game, HudField, ...), ready to use. */
export const {
  ControllerCode,
  CrashCode,
  EventKind,
  Game,
  HopBlocked,
  HudField,
  ModeCode,
  eval_episodes: evalEpisodes,
  Crowd,
  CrowdOutcome,
} = wasm;

/** The text of the repo's configs/controllers.toml. */
export const controllersToml = readFileSync(
  new URL('../../configs/controllers.toml', import.meta.url),
  'utf8',
);

/** The text of the repo's configs/modes.toml. */
export const modesToml = readFileSync(new URL('../../configs/modes.toml', import.meta.url), 'utf8');

/** The text of the repo's configs/vehicle.toml. */
export const vehicleToml = readFileSync(
  new URL('../../configs/vehicle.toml', import.meta.url),
  'utf8',
);

/** The text of the repo's trained policy, models/policy.json. */
export const policyJson = readFileSync(
  new URL('../../models/policy.json', import.meta.url),
  'utf8',
);

/** A game with the repo's configs and the given seed. */
export function newGame(seed = 0n) {
  return new Game(vehicleToml, modesToml, controllersToml, seed);
}

/** Runs `count` ticks of the same packed input frame. */
export function tickMany(game, input, count) {
  for (let tick = 0; tick < count; tick += 1) {
    game.tick(input);
  }
}

/** An episode row with every number rounded to float32, the precision the sim works in
 * (the results files were written by Python, which prints each float32 as a longer
 * double, so the text differs while the value is the same). */
export function asFloat32(row) {
  const rounded = {};
  for (const [key, value] of Object.entries(row)) {
    rounded[key] = typeof value === 'number' ? Math.fround(value) : value;
  }
  return rounded;
}
