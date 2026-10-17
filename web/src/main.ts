// Entry point for the web game. It wires the pieces together:
// keyboard → wasm physics (Game) at 120 Hz → the scene draws blended poses each frame.
// Each piece lives in its own folder: input/, render/, hud/; the loop is in game-loop.ts.
import modesToml from '../../configs/modes.toml?raw';
import vehicleToml from '../../configs/vehicle.toml?raw';

import { requireElement } from './dom';
import { startGameLoop } from './game-loop';
import { findHud, updateHud } from './hud/hud';
import {
  BUTTON_BOOST,
  BUTTON_CHAOS,
  BUTTON_HOP,
  onKeyPress,
  readInputFrame,
  startKeyboard,
} from './input/keyboard';
import { followCar, snapCamera } from './render/chase-camera';
import { type BlendedPose, FLOATS_PER_POSE, newBlendedPose } from './render/pose-blend';
import { createGameScene, type GameScene, type PhysicsFrames, placeCar } from './render/scene';
import initWasm, { EventKind, Game, HudField, ModeCode } from './wasm/nl_wasm.js';

/** How long the car stays wrecked before it respawns (SPEC §7.9 as amended). */
const RESPAWN_DELAY_MS = 1500;
/** Numbers per event from `Game.drain_events`: the kind, then up to three values. */
const FLOATS_PER_EVENT = 4;

async function start(): Promise<void> {
  await initWasm();
  if (Game.floats_per_pose() !== FLOATS_PER_POSE) {
    throw new Error('The wasm build and the renderer disagree on the pose layout');
  }
  if (Game.floats_per_event() !== FLOATS_PER_EVENT) {
    throw new Error('The wasm build and the game disagree on the event layout');
  }
  const buttonsMatch =
    Game.button_hop() === BUTTON_HOP &&
    Game.button_boost() === BUTTON_BOOST &&
    Game.button_chaos() === BUTTON_CHAOS;
  if (!buttonsMatch) {
    throw new Error('The wasm build and the keyboard disagree on the button bits');
  }
  // A new random seed each visit; it only affects Chaos knocks.
  const seed = BigInt(Math.floor(Math.random() * 2 ** 32));
  const game = new Game(vehicleToml, modesToml, seed);
  onKeyPress('KeyM', () => {
    const isArcade = game.hud()[HudField.Mode] === ModeCode.Arcade;
    game.set_mode(isArcade ? 'realistic' : 'arcade');
  });

  const frames: PhysicsFrames = {
    previousChassis: game.chassis_pose(),
    latestChassis: game.chassis_pose(),
    previousWheels: game.wheel_poses(),
    latestWheels: game.wheel_poses(),
  };
  const wheelCount = frames.latestWheels.length / FLOATS_PER_POSE;
  const view = createGameScene(requireElement('game'), wheelCount);
  const hud = findHud();
  startKeyboard(window);

  const carPose = newBlendedPose();
  snapViewToGame(game, frames, view, carPose);

  let respawnTimer: number | undefined;
  const respawnNow = () => {
    window.clearTimeout(respawnTimer);
    respawnTimer = undefined;
    game.respawn();
    // Jump straight to the checkpoint: no blending from the crash site, no camera swing.
    snapViewToGame(game, frames, view, carPose);
  };
  onKeyPress('KeyR', respawnNow);
  let lastRenderMs = performance.now();

  startGameLoop(Game.physics_dt_s(), {
    tick: () => {
      frames.previousChassis = frames.latestChassis;
      frames.previousWheels = frames.latestWheels;
      game.tick(readInputFrame());
      frames.latestChassis = game.chassis_pose();
      frames.latestWheels = game.wheel_poses();
    },
    render: (blend) => {
      // Events queue up in the sim, so reading them once per frame is enough.
      if (containsCrash(game.drain_events()) && respawnTimer === undefined) {
        // The explosion effect arrives in the next PR; for now, wait and respawn.
        respawnTimer = window.setTimeout(respawnNow, RESPAWN_DELAY_MS);
      }
      const nowMs = performance.now();
      const hudValues = game.hud();
      const airborne = hudValues[HudField.WheelsTouching] === 0;

      placeCar(view, frames, blend, carPose);
      followCar(view.camera, carPose, (nowMs - lastRenderMs) / 1000, airborne);
      updateHud(hud, {
        speedMS: hudValues[HudField.SpeedMS] ?? 0,
        mode: hudValues[HudField.Mode] ?? ModeCode.Custom,
        airFraction: hudValues[HudField.AirFraction] ?? 1,
        unlimitedAir: hudValues[HudField.UnlimitedAir] === 1,
        hopBlocked: hudValues[HudField.HopBlocked] ?? 0,
        crashed: game.is_crashed(),
      });
      view.renderer.render(view.scene, view.camera);
      lastRenderMs = nowMs;
    },
  });

  // A small read-only hook so automated tests can see where the car is and how much
  // simulated time has passed (slow test machines run fewer ticks per real second).
  Object.assign(window, {
    ninelivesDebug: {
      chassisPose: () => Array.from(game.chassis_pose()),
      simTimeS: () => game.hud()[HudField.TimeS] ?? 0,
    },
  });
}

/** Shows the car exactly where the sim has it, with no blending or camera easing. */
function snapViewToGame(
  game: Game,
  frames: PhysicsFrames,
  view: GameScene,
  carPose: BlendedPose,
): void {
  frames.previousChassis = frames.latestChassis = game.chassis_pose();
  frames.previousWheels = frames.latestWheels = game.wheel_poses();
  placeCar(view, frames, 1, carPose);
  snapCamera(view.camera, carPose);
}

/** True if any of the events (from `Game.drain_events`) is a crash. */
function containsCrash(events: Float32Array): boolean {
  for (let start = 0; start < events.length; start += FLOATS_PER_EVENT) {
    if (events[start] === EventKind.Crash) {
      return true;
    }
  }
  return false;
}

start().catch((error: unknown) => {
  console.error('Nine Lives failed to start:', error);
});
