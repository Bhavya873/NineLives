// Entry point for the web game. It wires the pieces together:
// keyboard → wasm physics (Game) at 120 Hz → the scene draws blended poses each frame.
// Each piece lives in its own folder: input/, render/, hud/; the loop is in game-loop.ts.
import modesToml from '../../configs/modes.toml?raw';
import vehicleToml from '../../configs/vehicle.toml?raw';

import { Vector3 } from 'three';

import { prefersReducedMotion, requireElement } from './dom';
import { startGameLoop } from './game-loop';
import { findHud, type HudValues, updateHud } from './hud/hud';
import {
  createToasts,
  showToast,
  tankJustRanDry,
  toastForEvents,
  type Toasts,
  updateToasts,
} from './hud/toasts';
import {
  BUTTON_BOOST,
  BUTTON_CHAOS,
  BUTTON_HOP,
  BUTTON_RESET,
  onKeyPress,
  readInputFrame,
  startKeyboard,
} from './input/keyboard';
import { followCar, snapCamera } from './render/chase-camera';
import {
  cameraShake,
  createExplosion,
  type Explosion,
  startExplosion,
  updateExplosion,
} from './render/explosion';
import { type BlendedPose, FLOATS_PER_POSE, newBlendedPose } from './render/pose-blend';
import {
  createGameScene,
  type GameScene,
  type PhysicsFrames,
  placeCar,
  setCarVisible,
} from './render/scene';
import { containsEvent, FLOATS_PER_EVENT } from './sim-events';
import initWasm, { EventKind, Game, HudField, ModeCode } from './wasm/nl_wasm.js';

/** What the per-frame event handling needs to reach. */
interface View {
  game: Game;
  frames: PhysicsFrames;
  scene: GameScene;
  carPose: BlendedPose;
  explosion: Explosion;
  toasts: Toasts;
}

/** Reused each frame so the render loop allocates nothing for these. */
const shake = new Vector3();
const crashPosition = new Vector3();

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
    Game.button_chaos() === BUTTON_CHAOS &&
    Game.button_reset() === BUTTON_RESET;
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
  const all: View = {
    game,
    frames,
    scene: view,
    carPose,
    explosion: createExplosion(view.scene, prefersReducedMotion()),
    toasts: createToasts(requireElement('hud-toast')),
  };

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
      const nowMs = performance.now();
      const frameS = (nowMs - lastRenderMs) / 1000;
      lastRenderMs = nowMs;
      // Events queue up in the sim, so reading them once per frame is enough.
      reactToEvents(all, game.drain_events(), nowMs);
      const hudValues = readHudValues(game);

      placeCar(view, frames, blend, carPose);
      setCarVisible(view, !hudValues.crashed);
      followCar(view.camera, carPose, frameS, hudValues.airborne);
      updateExplosion(all.explosion, frameS);
      // Shake only this frame's picture, so the camera's smoothing never sees it.
      view.camera.position.add(cameraShake(all.explosion, shake));
      view.renderer.render(view.scene, view.camera);
      view.camera.position.sub(shake);

      updateHud(hud, hudValues);
      if (tankJustRanDry(all.toasts, hudValues.airFraction, hudValues.unlimitedAir)) {
        showToast(all.toasts, 'NO AIR', nowMs);
      }
      updateToasts(all.toasts, nowMs);
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

/**
 * Reacts to this frame's sim events: a crash starts the explosion where the wreck is
 * frozen (SPEC §7.9), a respawn snaps the view to the checkpoint, and saves and crashes
 * get a toast. The sim decides all of it; the view only follows.
 */
function reactToEvents(all: View, events: Float32Array, nowMs: number): void {
  if (containsEvent(events, EventKind.Crash)) {
    crashPosition.fromArray(all.frames.latestChassis);
    startExplosion(all.explosion, crashPosition);
  }
  if (containsEvent(events, EventKind.Respawn)) {
    // Jump straight to the checkpoint: no blending from the crash site, no camera swing.
    snapViewToGame(all.game, all.frames, all.scene, all.carPose);
  }
  const toast = toastForEvents(events);
  if (toast) {
    showToast(all.toasts, toast, nowMs);
  }
}

/** Reads the HUD array once and names its values. */
function readHudValues(game: Game): HudValues & { airborne: boolean } {
  const values = game.hud();
  return {
    speedMS: values[HudField.SpeedMS] ?? 0,
    mode: values[HudField.Mode] ?? ModeCode.Custom,
    airFraction: values[HudField.AirFraction] ?? 1,
    unlimitedAir: values[HudField.UnlimitedAir] === 1,
    hopBlocked: values[HudField.HopBlocked] ?? 0,
    crashed: game.is_crashed(),
    saves: values[HudField.Saves] ?? 0,
    crashes: values[HudField.Crashes] ?? 0,
    timeS: values[HudField.TimeS] ?? 0,
    hopCooldownS: values[HudField.HopCooldownS] ?? 0,
    airborne: values[HudField.WheelsTouching] === 0,
  };
}

start().catch((error: unknown) => {
  console.error('Nine Lives failed to start:', error);
});
