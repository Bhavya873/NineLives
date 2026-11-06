// Entry point for the web game. It wires the pieces together:
// keyboard → wasm physics (Game) at 120 Hz → the scene draws blended poses each frame.
import controllersToml from '../../../configs/controllers.toml?raw';
import modesToml from '../../../configs/modes.toml?raw';
import playgroundJson from '../../../courses/playground.json?raw';
import vehicleToml from '../../../configs/vehicle.toml?raw';

import { Vector3 } from 'three';

import { prefersReducedMotion, requireElement } from '../shared/dom.ts';
import { createCourseSession, showCourse } from './course-session.ts';
import { startGameLoop } from './game-loop.ts';
import { saveRun, sessionStore, takeSavedRun } from './run-storage.ts';
import { countOutOfBoundsCrashes, hasRealCrash } from './out-of-bounds.ts';
import { createAirGauge, updateAirGauge } from '../ui/air-gauge.ts';
import { findHud, updateHud } from '../ui/hud.ts';
import { newZoneHolds, updateZones } from '../ui/hud-zones.ts';
import {
  createLives,
  grantFreeRespawns,
  LIVES,
  livesLeft,
  restartLives,
  restoreLives,
  updateLives,
} from '../ui/lives.ts';
import { createNozzleMap, updateNozzleMap } from '../ui/nozzle-map.ts';
import { createTitle, titleWanted } from '../ui/title.ts';
import {
  createPerfOverlay,
  recordFrame,
  recordTick,
  togglePerfOverlay,
} from '../ui/perf-overlay.ts';
import {
  createToasts,
  showNoAirIfNew,
  showToast,
  toastForEvents,
  updateToasts,
} from '../ui/toasts.ts';
import { isNozzleKeyHeld, onKeyPress, readInputFrame, startKeyboard } from '../ui/keyboard.ts';
import { followCar, snapCamera, widenWithSpeed } from '../render/chase-camera.ts';
import {
  cameraShake,
  createExplosion,
  startExplosion,
  stopExplosion,
  updateExplosion,
} from '../render/explosion.ts';
import { createFlames, updateFlames } from '../render/flames.ts';
import { type BlendedPose, FLOATS_PER_POSE, newBlendedPose } from '../render/pose-blend.ts';
import {
  createGameScene,
  type GameScene,
  type PhysicsFrames,
  placeCar,
  setCarVisible,
} from '../render/scene.ts';
import { loadPolicy } from '../wasm-bridge/policy.ts';
import {
  createScreenEffects,
  renderScreenEffects,
  nextWindLevel,
  resetScreenEffects,
  startingWindLevel,
  type WindLevel,
} from '../render/screen-effects.ts';
import { containsEvent } from '../wasm-bridge/sim-events.ts';
import { requestStuntInSim } from '../wasm-bridge/stunt-adapter.ts';
import {
  STUNT_KEYS,
  applyStuntEvents,
  createStuntQueue,
  expireStunt,
  pressStunt,
  stuntToastForEvents,
  submitPendingStunt,
} from './stunt-requests.ts';
import { fillHudValues, newHudValues } from '../wasm-bridge/hud-values.ts';
import { checkWasmLayout } from '../wasm-bridge/layout-check.ts';
import { readNozzleLayout } from '../wasm-bridge/nozzle-layout.ts';
import initWasm, { ControllerCode, EventKind, Game, HudField, ModeCode } from '../wasm/nl_wasm.js';

/** Reused each frame so the render loop allocates nothing for these. */
const shake = new Vector3();
const crashPosition = new Vector3();
const hudValues = newHudValues();

async function start(): Promise<void> {
  // load the wasm physics and check it matches this page
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
  // f3 performance overlay
  const perf = createPerfOverlay(requireElement('hud-perf'), performance.now());
  onKeyPress('F3', () => togglePerfOverlay(perf));
  // Stunt keys only queue a request; the sim's planner decides (see stunt-requests.ts).
  const stuntQueue = createStuntQueue();
  for (const [code, stunt] of Object.entries(STUNT_KEYS)) {
    onKeyPress(code, () => pressStunt(stuntQueue, stunt, performance.now() / 1000));
  }

  // the last two physics states, blended each frame for smooth motion
  const frames: PhysicsFrames = {
    previousChassis: game.chassis_pose(),
    latestChassis: game.chassis_pose(),
    previousWheels: game.wheel_poses(),
    latestWheels: game.wheel_poses(),
  };
  const wheelCount = frames.latestWheels.length / FLOATS_PER_POSE;
  // scene, hud parts and effects
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
  // keyboard and the first picture
  startKeyboard(window);

  const carPose = newBlendedPose();
  snapViewToGame(game, frames, view, carPose);
  const reducedMotion = prefersReducedMotion();
  const effects = createScreenEffects(view.scene);
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
  const startCourse = (shapesJson: string | null, courseJson?: string) => {
    showCourse(course, shapesJson, courseJson);
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
      startCourse(game.load_course(playgroundJson), playgroundJson);
    }
  };
  startFreeDrive();
  // Nine hearts, one lost per crash; at zero the game-over screen offers a restart.
  const lives = createLives(
    requireElement('hud-hearts'),
    requireElement('game-over'),
    requireElement('game-over-summary'),
  );
  let lastRenderMs = performance.now();
  fillHudValues(game.hud(), hudValues);
  // Back from the viewer (?play=1&resume=1): the lives lost before leaving stay lost.
  if (params.has('play') && params.has('resume')) {
    restoreLives(lives, hudValues, takeSavedRun(sessionStore()));
  }
  // Before a watch link leaves the page, remember the lives lost so far.
  for (const link of document.querySelectorAll<HTMLAnchorElement>('a[href$="viewer.html"]')) {
    link.addEventListener('click', () => {
      const left = livesLeft(lives, hudValues);
      saveRun(sessionStore(), LIVES - left, lives.over ? 0 : left);
    });
  }
  const zoneTiles = {
    gTile: requireElement('hud-g-tile'),
    nozzleTile: requireElement('hud-nozzle-tile'),
  };
  const zoneHolds = newZoneHolds();
  // The title keeps the game's clock still until the player starts (?play skips it).
  let started = false;
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
    if (!started || lives.over) {
      return;
    }
    paused = !paused;
    pausedPanel.hidden = !paused;
  });
  // The key card folds with its Keys button; it starts folded on small screens.
  const keysPanel = requireElement('hud-keys');
  const keysToggle = requireElement('hud-keys-toggle');
  const smallScreen = window.matchMedia('(max-width: 640px)');
  keysPanel.dataset['collapsed'] = String(smallScreen.matches);
  keysToggle.setAttribute('aria-expanded', String(!smallScreen.matches));
  keysToggle.addEventListener('click', () => {
    const collapsed = keysPanel.dataset['collapsed'] === 'true';
    keysPanel.dataset['collapsed'] = String(!collapsed);
    keysToggle.setAttribute('aria-expanded', String(collapsed));
  });
  createTitle(requireElement('title'), requireElement('title-play'), titleWanted(params), () => {
    started = true;
    lastRenderMs = performance.now();
  });
  // X: a drop test (SPEC §9.6 Stage A) from a fresh random start, to watch a save.
  onKeyPress('KeyX', () => {
    game.drop_test(BigInt(Math.floor(Math.random() * 2 ** 32)));
    // A drop during the respawn wait clears the crash without a Respawn event.
    stopExplosion(explosion);
    snapViewToGame(game, frames, view, carPose);
    resetScreenEffects(effects);
  });

  /**
   * Reacts to this frame's sim events: a crash starts the explosion where the wreck is
   * frozen (SPEC §7.9), a respawn ends it and snaps the view to the checkpoint, and
   * saves and crashes get a toast. The sim decides all of it; the view only follows.
   * Flying out of bounds is no wreck: no explosion, and the life is given back above.
   */
  const reactToEvents = (events: Float32Array, nowMs: number) => {
    if (hasRealCrash(events)) {
      startExplosion(explosion, crashPosition);
    }
    if (containsEvent(events, EventKind.Respawn)) {
      stopExplosion(explosion);
      // Jump straight to the checkpoint: no blending from the crash site, no camera swing.
      snapViewToGame(game, frames, view, carPose);
      resetScreenEffects(effects);
    }
    applyStuntEvents(stuntQueue, events);
    expireStunt(stuntQueue, nowMs / 1000);
    submitPendingStunt(
      stuntQueue,
      (code) => !paused && !lives.over && requestStuntInSim(game, code),
    );
    const toast = stuntToastForEvents(events) ?? toastForEvents(events);
    // Nothing covers the game-over screen.
    if (toast && !lives.over) {
      showToast(toasts, toast, nowMs);
    }
  };

  // the game loop: physics ticks at 120 hz, one render per browser frame
  startGameLoop(Game.physics_dt_s(), {
    tick: () => {
      if (!started || paused || lives.over) {
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
      // An out-of-bounds crash is a free respawn: give the life back before the hearts draw.
      grantFreeRespawns(lives, countOutOfBoundsCrashes(events));
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
        peakG: hudValues.peakG,
        policyOutOfEnvelope: hudValues.policyOutOfEnvelope,
        approachHintMS: hudValues.approachHintMS,
        events,
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
      updateZones(zoneTiles, zoneHolds, {
        thrusterKeyHeld: isNozzleKeyHeld(),
        gLoad: hudValues.crashed ? 1 : hudValues.gLoad,
        nowMs,
      });
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
      // Tests start a course of their own (the playground has no piece that needs speed).
      loadCourse: (courseJson: string) => {
        startCourse(game.load_course(courseJson), courseJson);
      },
      started: () => started,
      heartsLeft: () => livesLeft(lives, hudValues),
      // tests only: takes a life as a crash would
      loseLife: () => grantFreeRespawns(lives, -1),
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

start().catch((error: unknown) => {
  console.error('Nine Lives failed to start:', error);
});
