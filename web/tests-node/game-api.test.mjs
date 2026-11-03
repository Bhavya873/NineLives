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
  controllersToml,
  modesToml,
  tickMany,
  vehicleToml,
} from './load-wasm.mjs';

const FULL_THROTTLE = new Uint8Array([127, 0, 0, 0]);

test('full throttle moves the car along +X', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);

  tickMany(game, FULL_THROTTLE, 240);

  const pose = game.chassis_pose();
  assert.equal(pose.length, Game.floats_per_pose());
  assert.ok(pose[0] > 5, `x after 2 s: ${pose[0]}`);
  assert.equal(game.wheel_poses().length, 4 * Game.floats_per_pose());
  // Not exactly 0: the valves are still closing from the settle at the start (the drag's
  // centre of pressure pitches the car a hair as it drops onto its wheels).
  for (const throttle of game.nozzle_throttles()) assert.ok(throttle < 1e-6, `${throttle}`);
});

test('the mode comes back with a restored snapshot', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  game.set_mode('arcade');
  const saved = game.snapshot();

  game.set_mode('realistic');
  game.restore(saved);

  assert.equal(game.hud()[HudField.Mode], ModeCode.Arcade);
});

test('restoring a snapshot puts the car and the clock back exactly', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
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

  assert.throws(() => new Game(broken, modesToml, controllersToml, 0n), /chassis\.dry_mass_kg/);
});

test('tick needs exactly 4 input bytes', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);

  assert.throws(() => game.tick(new Uint8Array([1, 2])), /4 input bytes/);
});

test('switching to arcade gives unlimited air; unknown modes are refused', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);

  game.set_mode('arcade');

  assert.equal(game.hud()[HudField.Mode], ModeCode.Arcade);
  assert.equal(game.hud()[HudField.UnlimitedAir], 1);
  assert.equal(game.hud()[HudField.AirFraction], 1);
  assert.throws(() => game.set_mode('turbo'), /unknown mode/);
});

test('space-bar hop lifts the car once it has settled', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  tickMany(game, new Uint8Array([0, 0, 0, 0]), 180);
  assert.equal(game.hud()[HudField.HopBlocked], HopBlocked.No);

  game.tick(new Uint8Array([0, 0, 1, 0]));
  tickMany(game, new Uint8Array([0, 0, 0, 0]), 60);

  assert.ok(game.chassis_pose()[1] > 1.2, `height ${game.chassis_pose()[1]}`);
});

/** Splits the flat drain_events() array into [kind, a, b, c, d] groups. */
function eventGroups(values) {
  const groups = [];
  for (let start = 0; start < values.length; start += Game.floats_per_event()) {
    groups.push(Array.from(values.slice(start, start + Game.floats_per_event())));
  }
  return groups;
}

test('a hop reports airborne, upset, scrape and save events, with one rebound', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  // Thrusters off, so the bare Hop's own events show (PD would soften the landing).
  game.set_controller('off');
  tickMany(game, new Uint8Array([0, 0, 0, 0]), 180);
  game.drain_events();

  game.tick(new Uint8Array([0, 0, 1, 0]));
  tickMany(game, new Uint8Array([0, 0, 0, 0]), 360);
  const kinds = eventGroups(game.drain_events()).map((group) => group[0]);

  assert.deepEqual(kinds, [
    EventKind.AirborneStart,
    EventKind.UpsetStart,
    EventKind.AirborneEnd,
    // Since the G2 bump stop, a full Hop from a standstill no longer bottoms the springs (no
    // HardLanding): the underbody touches (a Scrape) and the stiff stop rebounds the car once.
    EventKind.Scrape,
    EventKind.AirborneStart,
    EventKind.AirborneEnd,
    EventKind.Save,
  ]);
  assert.equal(game.is_crashed(), false);
});

test('the Reset button respawns at the start with a full tank and keeps the mode', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
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

test('the HUD reports not crashed and not out of air at the start', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  tickMany(game, new Uint8Array([127, 0, 0, 0]), 120);

  const hud = game.hud();

  assert.equal(hud[HudField.Crashed], 0);
  assert.equal(hud[HudField.OutOfAir], 0);
  assert.ok(hud[HudField.SpeedMS] > 1, `speed ${hud[HudField.SpeedMS]}`);
});

test('holding Boost in tight circles runs the Realistic tank out of air', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  const boostAndTurn = new Uint8Array([0, 127, Game.button_boost(), 0]);
  let ticks = 0;
  while (game.hud()[HudField.OutOfAir] === 0 && ticks < 120 * 600) {
    game.tick(boostAndTurn);
    ticks += 1;
  }

  assert.equal(game.hud()[HudField.OutOfAir], 1, `air ${game.hud()[HudField.AirFraction]}`);
});

test('the HUD array and the event width match the contract', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);

  // docs/contracts/events-hud.md: HudField 27 is the last one so far (24 to 26 read 0 on
  // flat ground at rest); events are 5 floats.
  assert.equal(HudField.PeakGLoad, 23);
  assert.equal(HudField.ApproachHint, 26);
  assert.equal(HudField.PolicyOutOfEnvelope, 27);
  assert.equal(game.hud().length, HudField.PolicyOutOfEnvelope + 1);
  assert.deepEqual(Array.from(game.hud().slice(24, 27)), [0, 0, 0]);
  assert.equal(game.hud()[HudField.PolicyOutOfEnvelope], 0);
  assert.equal(Game.floats_per_event(), 5);
});

test('a Hop reports the raw peak g in the HUD and in the Save event', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  game.set_controller('off');
  tickMany(game, new Uint8Array([0, 0, 0, 0]), 180);
  game.drain_events();
  // (The start pose is a little above resting height, so a small first drop already ran.)

  game.tick(new Uint8Array([0, 0, 1, 0]));
  // The stiff bump stop rebounds the car once, and that take-off opens a fresh window
  // (docs/contracts/g-meter.md), so the peaks are read tick by tick, not once at the end.
  let rawPeak = 0;
  let displayedPeak = 0;
  let save = null;
  let hudPeakAtSave = null;
  for (let i = 0; i < 360; i += 1) {
    game.tick(new Uint8Array([0, 0, 0, 0]));
    const hud = game.hud();
    rawPeak = Math.max(rawPeak, hud[HudField.PeakGLoad]);
    displayedPeak = Math.max(displayedPeak, hud[HudField.GLoad]);
    for (const group of eventGroups(game.drain_events())) {
      if (group[0] === EventKind.Save) {
        save = group;
        hudPeakAtSave = hud[HudField.PeakGLoad];
      }
    }
  }

  assert.ok(save, 'a Save event');
  // The landing is in the peak, and the raw peak is at least the smoothed one.
  assert.ok(rawPeak > 1, `peak ${rawPeak}`);
  assert.ok(rawPeak >= displayedPeak - 1e-3, `raw ${rawPeak} displayed ${displayedPeak}`);
  assert.equal(save[4], hudPeakAtSave);
});

test('the approach hint names the needed speed until the car is fast enough', () => {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  game.load_course(
    JSON.stringify({
      version: 1,
      name: 'hint',
      start: { pos: [0, 0.7, 0], yaw: 0 },
      bounds: { min: [-50, -20, -50], max: [400, 60, 50] },
      pieces: [
        {
          type: 'ramp',
          pos: [120, 0, 0],
          length: 11.5,
          width: 8,
          angle_deg: 6,
          v_complete_min_m_s: 40,
        },
      ],
    }),
  );

  // At rest, 120 m before the ramp (past the 80 m look-ahead): no hint yet.
  assert.equal(game.hud()[HudField.ApproachHint], 0);
  // Driving up to it, slower than 40 m/s: the hint is the needed speed.
  let hinted = false;
  for (let i = 0; i < 12 * 120 && !hinted; i += 1) {
    tickMany(game, FULL_THROTTLE, 1);
    hinted = game.hud()[HudField.ApproachHint] === 40;
  }
  assert.ok(hinted, 'the hint shows once the ramp is inside the look-ahead');
  // Once fast enough it clears, still before the ramp.
  let cleared = false;
  for (let i = 0; i < 6 * 120 && !cleared; i += 1) {
    tickMany(game, FULL_THROTTLE, 1);
    const x = game.chassis_pose()[0];
    cleared = x < 119 && game.hud()[HudField.ApproachHint] === 0;
  }
  assert.ok(cleared, 'the hint clears at 40 m/s');
});
