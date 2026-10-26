// Entry point for the web game. It wires the pieces together:
// keyboard → wasm physics (Game) at 120 Hz → the scene draws blended poses each frame.
import controllersToml from '../../configs/controllers.toml?raw';
import modesToml from '../../configs/modes.toml?raw';
import playgroundJson from '../../courses/playground.json?raw';
import vehicleToml from '../../configs/vehicle.toml?raw';

import { Vector3 } from 'three';

import { prefersReducedMotion, requireElement } from './dom.ts';
import { createCourseSession, showCourse } from './course-session.ts';
import { startGameLoop } from './game-loop.ts';
import { createAirGauge, updateAirGauge } from './hud/air-gauge.ts';
import { findHud, type HudValues, updateHud } from './hud/hud.ts';
import { createLives, restartLives, updateLives } from './hud/lives.ts';
import { createNozzleMap, updateNozzleMap } from './hud/nozzle-map.ts';
import {
  createPerfOverlay,
  recordFrame,
  recordTick,
  togglePerfOverlay,
} from './hud/perf-overlay.ts';
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
  FIRST_MANUAL_NOZZLE_BIT,
  MANUAL_NOZZLE_COUNT,
  onKeyPress,
  readInputFrame,
  startKeyboard,
} from './input/keyboard.ts';
import { followCar, snapCamera, widenWithSpeed } from './render/chase-camera.ts';
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
import { loadPolicy } from './policy.ts';
import {
  createScreenEffects,
  renderScreenEffects,
  nextWindLevel,
  resetScreenEffects,
  startingWindLevel,
  type WindLevel,
} from './screen-effects.ts';
import { containsEvent, FLOATS_PER_EVENT } from './sim-events.ts';
import { FLOATS_PER_NOZZLE, readNozzleLayout } from './nozzle-layout.ts';
import initWasm, { ControllerCode, EventKind, Game, HudField, ModeCode } from './wasm/nl_wasm.js';

/** Reused each frame so the render loop allocates nothing for these. */
const shake = new Vector3();
const crashPosition = new Vector3();
const hudValues: HudValues = {
  speedMS: 0,
  mode: ModeCode.Realistic,
  controller: ControllerCode.Off,
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
  airUsedKg: 0,
  checkpointsReached: 0,
  checkpointCount: 0,
  finished: false,
  finishTimeS: 0,
  gLoad: 1,
  windWorldM_S: [0, 0, 0],
};

async function start(): Promise<void> {
  await initWasm();
  checkWasmLayout();
  // A new random seed each visit; the game itself draws nothing from it.
  const seed = BigInt(Math.floor(Math.random() * 2 ** 32));
  const game = new Game(vehicleToml, modesToml, controllersToml, seed);
  onKeyPress('KeyM', () => {
    const isArcade = game.hud()[HudField.Mode] === ModeCode.Arcade;
    game.set_mode(isArcade ? 'realistic' : 'arcade');
  });
  const params = new URLSearchParams(window.location.search);
  // The game flies PD alone, its best controller on every stage (DECISIONS 2026-10-27).
  // ?controller=hybrid loads the policy and flies PD + AI once it has loaded.
  if (params.get('controller') === 'hybrid') {
    void loadPolicy(game).then((loaded) => {
      if (loaded) {
        game.set_controller('hybrid');
      }
    });
  }
  const perf = createPerfOverlay(requireElement('hud-perf'), performance.now());
  onKeyPress('F3', () => togglePerfOverlay(perf));

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
  const reducedMotion = prefersReducedMotion();
  const effects = createScreenEffects(view.scene, reducedMotion);
  // Wind (SPEC §7.2): ?wind=breeze|storm picks the starting level, V cycles it.
  let windLevel = startingWindLevel(params);
  const setWind = (level: WindLevel) => {
    windLevel = level;
    game.set_wind_level(level);
  };
  setWind(windLevel);
  onKeyPress('KeyV', () => setWind(nextWindLevel(windLevel)));
  const course = createCourseSession(view.scene, view.ground);
  /** Puts the drawn course in step with a fresh run (null: the open arena). */
  const startCourse = (shapesJson: string | null) => {
    showCourse(course, shapesJson);
    stopExplosion(explosion);
    snapViewToGame(game, frames, view, carPose);
    resetScreenEffects(effects);
  };
  // Free driving happens in the open-world playground; ?world=flat keeps the empty arena.
  const startFreeDrive = () => {
    if (params.get('world') === 'flat') {
      game.load_flat_ground();
      startCourse(null);
    } else {
      startCourse(game.load_course(playgroundJson));
    }
  };
  startFreeDrive();
  // Nine hearts, one lost per crash; at zero the game-over screen offers a restart.
  const lives = createLives(
    requireElement('hud-hearts'),
    requireElement('game-over'),
    requireElement('game-over-summary'),
  );
  // Restart: a fresh playground run, which also sets the sim's counts back to zero.
  requireElement('restart').addEventListener('click', () => {
    startFreeDrive();
    game.drain_events();
    fillHudValues(game.hud(), hudValues);
    restartLives(lives, hudValues);
  });
  // Esc pauses and resumes; nothing moves while paused or after the last life.
  const pausedPanel = requireElement('paused');
  let paused = false;
  onKeyPress('Escape', () => {
    if (!lives.over) {
      paused = !paused;
      pausedPanel.hidden = !paused;
    }
  });
  // X: a drop test (SPEC §9.6 Stage A) from a fresh random start, to watch a save.
  onKeyPress('KeyX', () => {
    game.drop_test(BigInt(Math.floor(Math.random() * 2 ** 32)));
    // A drop during the respawn wait clears the crash without a Respawn event.
    stopExplosion(explosion);
    snapViewToGame(game, frames, view, carPose);
    resetScreenEffects(effects);
  });
  let lastRenderMs = performance.now();
  const invertedTiltRad = (Game.inverted_tilt_deg() * Math.PI) / 180;

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
      resetScreenEffects(effects);
    }
    const toast = toastForEvents(events, invertedTiltRad);
    // Nothing covers the game-over screen.
    if (toast && !lives.over) {
      showToast(toasts, toast, nowMs);
    }
  };

  startGameLoop(Game.physics_dt_s(), {
    tick: () => {
      if (paused || lives.over) {
        return;
      }
      const tickStartMs = performance.now();
      frames.previousChassis = frames.latestChassis;
      frames.previousWheels = frames.latestWheels;
      game.tick(readInputFrame());
      frames.latestChassis = game.chassis_pose();
      frames.latestWheels = game.wheel_poses();
      // Remember where the wreck froze, even if it respawns before the next frame.
      if (game.is_crashed()) {
        crashPosition.fromArray(frames.latestChassis);
      }
      recordTick(perf, performance.now() - tickStartMs);
    },
    render: (blend) => {
      const nowMs = performance.now();
      const frameS = (nowMs - lastRenderMs) / 1000;
      lastRenderMs = nowMs;
      // Events queue up in the sim, so reading them once per frame is enough.
      // The HUD's save and crash counts update as events drain, so drain first.
      const events = game.drain_events();
      fillHudValues(game.hud(), hudValues);
      updateLives(lives, hudValues, nowMs);
      reactToEvents(events, nowMs);

      // Valve lag already smooths the throttles, so one read per frame is enough.
      const throttles = game.nozzle_throttles();
      placeCar(view, frames, blend, carPose);
      setCarVisible(view, !hudValues.crashed);
      updateFlames(flames, throttles);
      followCar(view.camera, carPose, frameS, hudValues.wheelsTouching === 0);
      if (!reducedMotion) {
        widenWithSpeed(view.camera, hudValues.speedMS, frameS);
      }
      updateExplosion(explosion, frameS);
      // Shake only this frame's picture, so the camera's smoothing never sees it.
      view.camera.position.add(cameraShake(explosion, shake));
      view.renderer.render(view.scene, view.camera);
      view.camera.position.sub(shake);
      renderScreenEffects(effects, {
        gLoad: hudValues.gLoad,
        windWorldM_S: hudValues.windWorldM_S,
        crashed: hudValues.crashed,
        camera: view.camera,
        carPosition: carPose.position,
        frameS,
        nowS: nowMs / 1000,
      });

      updateHud(hud, hudValues);
      updateAirGauge(airGauge, hudValues.airFraction, hudValues.unlimitedAir);
      updateNozzleMap(nozzleDots, throttles);
      updateToasts(toasts, nowMs);
      showNoAirIfNew(toasts, hudValues.outOfAir, hudValues.airFraction, nowMs);
      const frameEndMs = performance.now();
      recordFrame(perf, frameEndMs - nowMs, frameEndMs);
    },
  });

  // A small read-only hook so automated tests can see where the car is and how much
  // simulated time has passed (slow test machines run fewer ticks per real second).
  Object.assign(window, {
    ninelivesDebug: {
      chassisPose: () => Array.from(game.chassis_pose()),
      simTimeS: () => game.hud()[HudField.TimeS] ?? 0,
      controllerCode: () => game.hud()[HudField.Controller] ?? ControllerCode.Off,
      courseMeshCount: () => course.view?.children.length ?? 0,
      checkpointCount: () => game.hud()[HudField.CheckpointCount] ?? 0,
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
    Game.button_reset() === BUTTON_RESET &&
    Game.first_manual_nozzle_bit() === FIRST_MANUAL_NOZZLE_BIT &&
    Game.nozzle_count() === MANUAL_NOZZLE_COUNT;
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
  out.controller = values[HudField.Controller] ?? ControllerCode.Off;
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
  out.airUsedKg = values[HudField.AirUsedKg] ?? 0;
  out.checkpointsReached = values[HudField.CheckpointsReached] ?? 0;
  out.checkpointCount = values[HudField.CheckpointCount] ?? 0;
  out.finished = values[HudField.Finished] === 1;
  out.finishTimeS = values[HudField.FinishTimeS] ?? 0;
  out.gLoad = values[HudField.GLoad] ?? 1;
  out.windWorldM_S[0] = values[HudField.WindXMS] ?? 0;
  out.windWorldM_S[2] = values[HudField.WindZMS] ?? 0;
}

start().catch((error: unknown) => {
  console.error('Nine Lives failed to start:', error);
});
