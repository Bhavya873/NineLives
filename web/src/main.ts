// Entry point for the web game. It wires the pieces together:
// keyboard → wasm physics (Game) at 120 Hz → the scene draws blended poses each frame.
// Each piece lives in its own folder: input/, render/, hud/; the loop is in game-loop.ts.
import modesToml from '../../configs/modes.toml?raw';
import vehicleToml from '../../configs/vehicle.toml?raw';

import { Vector3 } from 'three';

import { prefersReducedMotion, requireElement } from './dom.ts';
import { startGameLoop } from './game-loop.ts';
import { createAirGauge, updateAirGauge } from './hud/air-gauge.ts';
import { findHud, type HudValues, updateHud } from './hud/hud.ts';
import { createNozzleMap, updateNozzleMap } from './hud/nozzle-map.ts';
import {
  createToasts,
  showNoAirIfNew,
  showToast,
  toastForEvents,
  updateToasts,
} from './hud/toasts.ts';
import {
  BUTTON_BOOST,
  BUTTON_CHAOS,
  BUTTON_HOP,
  BUTTON_RESET,
  onKeyPress,
  readInputFrame,
  startKeyboard,
} from './input/keyboard.ts';
import { followCar, snapCamera } from './render/chase-camera.ts';
import {
  cameraShake,
  createExplosion,
  startExplosion,
  stopExplosion,
  updateExplosion,
} from './render/explosion.ts';
import { createFlames, updateFlames } from './render/flames.ts';
import { type BlendedPose, FLOATS_PER_POSE, newBlendedPose } from './render/pose-blend.ts';
import {
  createGameScene,
  type GameScene,
  type PhysicsFrames,
  placeCar,
  setCarVisible,
} from './render/scene.ts';
import { containsEvent, FLOATS_PER_EVENT } from './sim-events.ts';
import { FLOATS_PER_NOZZLE, readNozzleLayout } from './nozzle-layout.ts';
import initWasm, { EventKind, Game, HudField, ModeCode } from './wasm/nl_wasm.js';

/** Reused each frame so the render loop allocates nothing for these. */
const shake = new Vector3();
const crashPosition = new Vector3();
const hudValues: HudValues = {
  speedMS: 0,
  mode: ModeCode.Realistic,
  airFraction: 1,
  unlimitedAir: false,
  hopBlocked: 0,
  crashed: false,
  saves: 0,
  crashes: 0,
  timeS: 0,
  hopCooldownS: 0,
  wheelsTouching: 4,
  outOfAir: false,
};

async function start(): Promise<void> {
  await initWasm();
  checkWasmLayout();
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
  const toasts = createToasts(requireElement('hud-toast'));
  const explosion = createExplosion(view.scene, prefersReducedMotion());
  const nozzles = readNozzleLayout(game.nozzle_layout());
  const flames = createFlames(view.carBody, nozzles);
  const nozzleDots = createNozzleMap(requireElement('hud-nozzles'), nozzles);
  const airGauge = createAirGauge(
    requireElement('hud-air'),
    requireElement('hud-air-fill'),
    requireElement('hud-air-label'),
  );
  startKeyboard(window);

  const carPose = newBlendedPose();
  snapViewToGame(game, frames, view, carPose);
  let lastRenderMs = performance.now();

  /**
   * Reacts to this frame's sim events: a crash starts the explosion where the wreck is
   * frozen (SPEC §7.9), a respawn ends it and snaps the view to the checkpoint, and
   * saves and crashes get a toast. The sim decides all of it; the view only follows.
   */
  const reactToEvents = (events: Float32Array, nowMs: number) => {
    if (containsEvent(events, EventKind.Crash)) {
      startExplosion(explosion, crashPosition);
    }
    if (containsEvent(events, EventKind.Respawn)) {
      stopExplosion(explosion);
      // Jump straight to the checkpoint: no blending from the crash site, no camera swing.
      snapViewToGame(game, frames, view, carPose);
    }
    const toast = toastForEvents(events);
    if (toast) {
      showToast(toasts, toast, nowMs);
    }
  };

  startGameLoop(Game.physics_dt_s(), {
    tick: () => {
      frames.previousChassis = frames.latestChassis;
      frames.previousWheels = frames.latestWheels;
      game.tick(readInputFrame());
      frames.latestChassis = game.chassis_pose();
      frames.latestWheels = game.wheel_poses();
      // Remember where the wreck froze, even if it respawns before the next frame.
      if (game.is_crashed()) {
        crashPosition.fromArray(frames.latestChassis);
      }
    },
    render: (blend) => {
      const nowMs = performance.now();
      const frameS = (nowMs - lastRenderMs) / 1000;
      lastRenderMs = nowMs;
      // Events queue up in the sim, so reading them once per frame is enough.
      reactToEvents(game.drain_events(), nowMs);
      fillHudValues(game.hud(), hudValues);

      // Valve lag already smooths the throttles, so one read per frame is enough.
      const throttles = game.nozzle_throttles();
      placeCar(view, frames, blend, carPose);
      setCarVisible(view, !hudValues.crashed);
      updateFlames(flames, throttles);
      followCar(view.camera, carPose, frameS, hudValues.wheelsTouching === 0);
      updateExplosion(explosion, frameS);
      // Shake only this frame's picture, so the camera's smoothing never sees it.
      view.camera.position.add(cameraShake(explosion, shake));
      view.renderer.render(view.scene, view.camera);
      view.camera.position.sub(shake);

      updateHud(hud, hudValues);
      updateAirGauge(airGauge, hudValues.airFraction, hudValues.unlimitedAir);
      updateNozzleMap(nozzleDots, throttles);
      updateToasts(toasts, nowMs);
      showNoAirIfNew(toasts, hudValues.outOfAir, hudValues.airFraction, nowMs);
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

/** Throws if this page and the wasm build disagree on a shared layout or bit. */
function checkWasmLayout(): void {
  if (Game.floats_per_pose() !== FLOATS_PER_POSE) {
    throw new Error('The wasm build and the renderer disagree on the pose layout');
  }
  if (Game.floats_per_nozzle() !== FLOATS_PER_NOZZLE) {
    throw new Error('The wasm build and the renderer disagree on the nozzle layout');
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

/** Copies the values the HUD shows out of `Game.hud()`'s array into `out`. */
function fillHudValues(values: Float32Array, out: HudValues): void {
  out.speedMS = values[HudField.SpeedMS] ?? 0;
  out.mode = values[HudField.Mode] ?? ModeCode.Custom;
  out.airFraction = values[HudField.AirFraction] ?? 1;
  out.unlimitedAir = values[HudField.UnlimitedAir] === 1;
  out.hopBlocked = values[HudField.HopBlocked] ?? 0;
  out.crashed = values[HudField.Crashed] === 1;
  out.saves = values[HudField.Saves] ?? 0;
  out.crashes = values[HudField.Crashes] ?? 0;
  out.timeS = values[HudField.TimeS] ?? 0;
  out.hopCooldownS = values[HudField.HopCooldownS] ?? 0;
  out.wheelsTouching = values[HudField.WheelsTouching] ?? 0;
  out.outOfAir = values[HudField.OutOfAir] === 1;
}

start().catch((error: unknown) => {
  console.error('Nine Lives failed to start:', error);
});
