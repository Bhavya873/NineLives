// The stunt planner through the wasm Game (plan G4): request_stunt, the StuntPhase and
// StuntCode HUD fields, and the stunt events. A full authorized flip is covered natively
// (crates/nl-control/tests/stunt_flight.rs); here a drop is too short to flip, and a car
// on the ground never takes off, so both end in a refusal.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  EventKind,
  Game,
  HudField,
  StuntReasonCode,
  controllersToml,
  modesToml,
  tickMany,
  vehicleToml,
} from './load-wasm.mjs';

const IDLE = new Uint8Array([0, 0, 0, 0]);
const FRONT_FLIP = 3;
const BACK_FLIP = 4;

/** The stunt events (kind 11 to 14) in a drained event array, as arrays of 5 floats. */
function stuntEvents(events) {
  const found = [];
  for (let start = 0; start < events.length; start += Game.floats_per_event()) {
    const kind = events[start];
    if (kind >= EventKind.StuntStart && kind <= EventKind.StuntAborted) {
      found.push(Array.from(events.slice(start, start + Game.floats_per_event())));
    }
  }
  return found;
}

test('an unknown stunt code is not queued', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  assert.equal(game.request_stunt(0), false);
  assert.equal(game.request_stunt(5), false);
  assert.equal(game.hud()[HudField.StuntPhase], 0);
});

test('a request shows on the HUD, and a second one waits for the first', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  assert.equal(game.request_stunt(BACK_FLIP), true);
  assert.equal(game.hud()[HudField.StuntPhase], 1, 'Requested');
  assert.equal(game.hud()[HudField.StuntCode], BACK_FLIP);
  assert.equal(game.request_stunt(FRONT_FLIP), false);
});

test('a request on the ground lapses after 3 s with the expired reason', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  // The car starts a hair above the ground; let it settle on its wheels first.
  tickMany(game, IDLE, 240);
  assert.equal(game.hud()[HudField.WheelsTouching], 4);
  game.drain_events();
  game.request_stunt(BACK_FLIP);
  const seen = [];
  for (let step = 0; step < 400; step += 1) {
    game.tick(IDLE);
    seen.push(...stuntEvents(game.drain_events()));
  }
  assert.deepEqual(seen, [
    [EventKind.StuntRefused, BACK_FLIP, StuntReasonCode.RequestExpired, 0, 0],
  ]);
  assert.equal(game.hud()[HudField.StuntPhase], 0);
  assert.equal(game.hud()[HudField.StuntCode], 0);
});

test('a short drop refuses the flip for lack of air time and leaves PD flying', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  game.drop_test(3n);
  game.request_stunt(FRONT_FLIP);
  const seen = [];
  for (let step = 0; step < 240; step += 1) {
    game.tick(IDLE);
    seen.push(...stuntEvents(game.drain_events()));
  }
  assert.equal(seen.length, 1);
  assert.deepEqual(seen[0].slice(0, 3), [
    EventKind.StuntRefused,
    FRONT_FLIP,
    StuntReasonCode.NotEnoughAirTime,
  ]);
  tickMany(game, IDLE, 360);
  assert.equal(game.hud()[HudField.WheelsTouching], 4);
  assert.equal(game.hud()[HudField.StuntPhase], 0);
});
