// Checks courses through the wasm Game API (SPEC §7.11, M7): loading a course returns
// its shapes for the renderer, and driving over its lines fills the HUD's course fields
// and reports the events. Run `npm run wasm` first; `npm run test:wasm` runs it.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

import {
  EventKind,
  Game,
  HudField,
  controllersToml,
  modesToml,
  tickMany,
  vehicleToml,
} from './load-wasm.mjs';

const FULL_THROTTLE = new Uint8Array([127, 0, 0, 0]);
const gauntletJson = readFileSync(new URL('../../courses/gauntlet.json', import.meta.url), 'utf8');

/** A short straight: a checkpoint 10 m out and the finish 20 m out. */
const SPRINT_JSON = JSON.stringify({
  version: 1,
  name: 'Sprint',
  start: { pos: [0, 0.7, 0] },
  bounds: { min: [-20, -20, -20], max: [200, 50, 20] },
  pieces: [
    { type: 'checkpoint', pos: [10, 0, 0] },
    { type: 'finish', pos: [20, 0, 0] },
  ],
});

function newGame() {
  return new Game(vehicleToml, modesToml, controllersToml, 0n);
}

/** The event kinds in a drained event array. */
function eventKinds(values) {
  const kinds = [];
  for (let start = 0; start < values.length; start += Game.floats_per_event()) {
    kinds.push(values[start]);
  }
  return kinds;
}

test('loading the Gauntlet returns its shapes and starts at checkpoint 0 of 6', () => {
  const game = newGame();

  const shapes = JSON.parse(game.load_course(gauntletJson));

  assert.ok(shapes.length > 20, `only ${shapes.length} shapes`);
  const surfaces = new Set(shapes.map((shape) => shape.surface));
  for (const wanted of ['floor', 'ramp', 'obstacle', 'checkpoint', 'finish']) {
    assert.ok(surfaces.has(wanted), `no ${wanted} shape`);
  }
  const hud = game.hud();
  assert.equal(hud[HudField.CheckpointCount], 6);
  assert.equal(hud[HudField.CheckpointsReached], 0);
  assert.equal(hud[HudField.Finished], 0);
});

test('driving a short course reaches the checkpoint and the finish', () => {
  const game = newGame();
  game.load_course(SPRINT_JSON);

  tickMany(game, FULL_THROTTLE, 6 * 120);

  const kinds = eventKinds(game.drain_events());
  assert.ok(kinds.includes(EventKind.CheckpointReached), `events: ${kinds}`);
  assert.ok(kinds.includes(EventKind.Finished), `events: ${kinds}`);
  const hud = game.hud();
  assert.equal(hud[HudField.CheckpointsReached], 1);
  assert.equal(hud[HudField.Finished], 1);
  assert.ok(hud[HudField.FinishTimeS] > 0.5, `finish time ${hud[HudField.FinishTimeS]}`);
});

test('reloading a course starts the run over', () => {
  const game = newGame();
  game.load_course(SPRINT_JSON);
  tickMany(game, FULL_THROTTLE, 6 * 120);
  game.drain_events();

  game.load_course(SPRINT_JSON);

  const hud = game.hud();
  assert.equal(hud[HudField.Finished], 0);
  assert.equal(hud[HudField.FinishTimeS], 0);
  assert.equal(hud[HudField.TimeS], 0);
  assert.ok(Math.abs(game.chassis_pose()[0]) < 0.1);
});

test('going back to flat ground clears the course', () => {
  const game = newGame();
  game.load_course(gauntletJson);

  game.load_flat_ground();

  assert.equal(game.hud()[HudField.CheckpointCount], 0);
});

test('a broken course is refused with the piece and field to fix', () => {
  const game = newGame();
  const broken = SPRINT_JSON.replace('"finish"', '"ramp"');

  assert.throws(() => game.load_course(broken), /piece 1 'ramp'/);
});

test('with full throttle and the PD controller the Gauntlet can be finished', () => {
  // Guards the course geometry: a ramp foot, kicker or bump that stops or crash-loops
  // the car (each happened while building M7) would keep it from the finish.
  const game = newGame();
  game.set_controller('pd');
  game.load_course(gauntletJson);

  for (let second = 0; second < 60 && game.hud()[HudField.Finished] !== 1; second += 1) {
    tickMany(game, FULL_THROTTLE, 120);
    game.drain_events();
  }

  const hud = game.hud();
  assert.equal(hud[HudField.Finished], 1, `stuck at x = ${game.chassis_pose()[0]}`);
  assert.equal(hud[HudField.CheckpointsReached], 6);
});
