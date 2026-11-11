// Entry point for the web game. It wires the pieces together:
// keyboard → wasm physics (Game) at 120 Hz → the scene draws blended poses each frame.
import controllersToml from '../../../configs/controllers.toml?raw';
import modesToml from '../../../configs/modes.toml?raw';
import playgroundJson from '../../../courses/playground.json?raw';
import vehicleToml from '../../../configs/vehicle.toml?raw';

import { focusDialog, prefersReducedMotion, requireElement } from '../shared/dom.ts';
import { type ExtraInputs, type GameActions, startExtraInputs } from './extra-inputs.ts';
import { startGameLoop } from './game-loop.ts';
import { createLoopCallbacks, type LoopContext, type RunState } from './frame-steps.ts';
import { createHudParts, type HudParts } from './hud-parts.ts';
import { saveRun, sessionStore, takeSavedRun } from './run-storage.ts';
import { enableRefractionIfSupported } from '../ui/refraction.ts';
import {
  createLives,
  grantFreeRespawns,
  LIVES,
  livesLeft,
  restartLives,
  restoreLives,
  savesThisRun,
} from '../ui/lives.ts';
import { createTitle, titleWanted } from '../ui/title.ts';
import { createPerfOverlay, togglePerfOverlay } from '../ui/perf-overlay.ts';
import { onKeyPress, startKeyboard } from '../ui/keyboard.ts';
import { nextWindLevel, startingWindLevel, type WindLevel } from '../render/screen-effects.ts';
import { loadPolicy } from '../wasm-bridge/policy.ts';
import { createStuntQueue, pressStunt, STUNT_KEYS, type StuntCodeValue } from './stunt-requests.ts';
import { fillHudValues, newHudValues } from '../wasm-bridge/hud-values.ts';
import { checkWasmLayout } from '../wasm-bridge/layout-check.ts';
import initWasm, { ControllerCode, Game, HudField, ModeCode } from '../wasm/nl_wasm.js';
import { createCourseControls, createDropTest, createWorld, snapViewToGame } from './world.ts';
import type { CourseControls, World } from './world.ts';

const hudValues = newHudValues();

type LivesState = ReturnType<typeof createLives>;

/** Loads the wasm physics, checks it matches this page and creates the game. */
async function createGame(): Promise<Game> {
  await initWasm();
  checkWasmLayout();
  // A new random seed each visit; the game itself draws nothing from it.
  const seed = BigInt(Math.floor(Math.random() * 2 ** 32));
  return new Game(vehicleToml, modesToml, controllersToml, seed);
}

/** The M key and a tap on the air-mode readout. Returns the toggle for the touch buttons. */
function setupAirModeToggle(game: Game): () => void {
  const toggleAirMode = () => {
    const isArcade = game.hud()[HudField.Mode] === ModeCode.Arcade;
    game.set_mode(isArcade ? 'realistic' : 'arcade');
  };
  onKeyPress('KeyM', toggleAirMode);
  return toggleAirMode;
}

/**
 * The game flies the Hybrid (PD + the shipped r5 policy) by default: with the 120 degree
 * hand-over it gains 17 to 20 points on Stage F flip plates and is within about 1.5 points of
 * PD elsewhere (DECISIONS 2026-11-10). PD still flies below 120 degrees of tilt and above the
 * 55 m/s envelope. ?controller=pd keeps PD alone and skips loading the policy. If the policy
 * fails to load, the game stays on PD.
 */
function startPolicy(game: Game, params: URLSearchParams): void {
  if (params.get('controller') !== 'pd') {
    void loadPolicy(game).then((loaded) => {
      if (loaded) {
        game.set_controller('hybrid');
      }
    });
  }
}

/** Stunt keys only queue a request; the sim's planner decides (see stunt-requests.ts). */
function setupStunts(state: RunState): {
  stuntQueue: ReturnType<typeof createStuntQueue>;
  requestStunt: (stunt: StuntCodeValue) => void;
} {
  const stuntQueue = createStuntQueue();
  const requestStunt = (stunt: StuntCodeValue) => {
    state.stuntRequests += 1;
    pressStunt(stuntQueue, stunt, performance.now() / 1000);
  };
  for (const [code, stunt] of Object.entries(STUNT_KEYS)) {
    onKeyPress(code, () => requestStunt(stunt));
  }
  return { stuntQueue, requestStunt };
}

/** Wind (SPEC §7.2): ?wind=breeze|storm picks the starting level, V cycles it. */
function setupWind(game: Game, params: URLSearchParams): () => void {
  let windLevel = startingWindLevel(params);
  const setWind = (level: WindLevel) => {
    windLevel = level;
    game.set_wind_level(level);
  };
  setWind(windLevel);
  const cycleWind = () => setWind(nextWindLevel(windLevel));
  onKeyPress('KeyV', cycleWind);
  return cycleWind;
}

/** Nine hearts, one lost per crash; at zero the game-over screen offers a restart. */
function setupLives(game: Game, params: URLSearchParams): LivesState {
  const lives = createLives(
    requireElement('hud-hearts'),
    requireElement('game-over'),
    requireElement('game-over-summary'),
  );
  fillHudValues(game.hud(), hudValues);
  // Back from the viewer (?play=1&resume=1): the lives lost before leaving stay lost.
  if (params.has('play') && params.has('resume')) {
    const saved = takeSavedRun(sessionStore());
    restoreLives(lives, hudValues, saved.lostLives, saved.saves);
  }
  // Before a watch link leaves the page, remember the lives lost so far.
  for (const link of document.querySelectorAll<HTMLAnchorElement>('a[href$="viewer.html"]')) {
    link.addEventListener('click', () => {
      const left = livesLeft(lives, hudValues);
      const run = { lostLives: LIVES - left, saves: savesThisRun(lives, hudValues) };
      saveRun(sessionStore(), run, lives.over ? 0 : left);
    });
  }
  return lives;
}

/** Restart: a fresh playground run, which also sets the sim's counts back to zero. */
function setupRestart(game: Game, lives: LivesState, courses: CourseControls): void {
  requireElement('restart').addEventListener('click', () => {
    courses.startFreeDrive();
    game.drain_events();
    fillHudValues(game.hud(), hudValues);
    restartLives(lives, hudValues);
  });
}

/**
 * Esc pauses and resumes; nothing moves while paused or after the last life.
 * `releaseHeldInputs` lets go of touch controls when the game pauses under a finger.
 */
function setupPause(state: RunState, lives: LivesState, releaseHeldInputs: () => void): () => void {
  const pausedPanel = requireElement('paused');
  let restorePauseFocus: (() => void) | null = null;
  const togglePause = () => {
    if (!state.started || lives.over) {
      return;
    }
    state.paused = !state.paused;
    if (state.paused) {
      releaseHeldInputs();
    }
    pausedPanel.hidden = !state.paused;
    // The pause card is a dialog: the page behind it is inert, and Esc gives focus back.
    if (state.paused) {
      restorePauseFocus = focusDialog(pausedPanel, pausedPanel.querySelector('a') ?? pausedPanel);
    } else {
      restorePauseFocus?.();
      restorePauseFocus = null;
    }
  };
  onKeyPress('Escape', togglePause);
  return togglePause;
}

/** The key card starts folded to a chip; the chip and the H key unfold and fold it. */
function setupKeyCard(state: RunState): void {
  const keysPanel = requireElement('hud-keys');
  const keysToggle = requireElement('hud-keys-toggle');
  const toggleKeyCard = () => {
    const wasFolded = keysPanel.dataset['collapsed'] === 'true';
    keysPanel.dataset['collapsed'] = String(!wasFolded);
    keysToggle.setAttribute('aria-expanded', String(wasFolded));
  };
  keysToggle.addEventListener('click', toggleKeyCard);
  onKeyPress('KeyH', () => {
    if (state.started && !state.paused) {
      toggleKeyCard();
    }
  });
}

/** A small read-only hook so automated tests can see the car, the clock and the lives. */
function installDebugHook(
  ctx: LoopContext,
  params: URLSearchParams,
  courses: CourseControls,
): void {
  const { game, world, state, lives } = ctx;
  Object.assign(window, {
    ninelivesDebug: {
      chassisPose: () => Array.from(game.chassis_pose()),
      simTimeS: () => game.hud()[HudField.TimeS] ?? 0,
      controllerCode: () => game.hud()[HudField.Controller] ?? ControllerCode.Off,
      courseMeshCount: () => world.course.view?.children.length ?? 0,
      checkpointCount: () => game.hud()[HudField.CheckpointCount] ?? 0,
      // Tests start a course of their own (the playground has no piece that needs speed).
      loadCourse: (courseJson: string) => {
        courses.startCourse(game.load_course(courseJson), courseJson);
      },
      started: () => state.started,
      stuntRequestCount: () => state.stuntRequests,
      // where the car mesh and the camera are drawn (not the sim pose)
      drawnCarPosition: () => plainPoint(world.view.carBody.position),
      cameraPosition: () => plainPoint(world.view.camera.position),
      heartsLeft: () => livesLeft(lives, hudValues),
      // tests only (?testhooks): takes a life as a crash would
      ...(params.has('testhooks') ? { loseLife: () => grantFreeRespawns(lives, -1) } : {}),
    },
  });
}

/** Builds the world and the HUD parts, then shows the first picture. */
function setupWorld(game: Game): { world: World; hudParts: HudParts } {
  const world = createWorld(game);
  const hudParts = createHudParts(game, world);
  startKeyboard(window);
  enableRefractionIfSupported();
  snapViewToGame(world);
  return { world, hudParts };
}

/** The pause, key-card, drop-test and restart controls. Returns what the touch inputs call. */
function setupControls(
  game: Game,
  world: World,
  state: RunState,
  lives: LivesState,
  courses: CourseControls,
  releaseHeldInputs: () => void,
): { togglePause: () => void; dropTest: () => void } {
  setupRestart(game, lives, courses);
  const togglePause = setupPause(state, lives, releaseHeldInputs);
  setupKeyCard(state);
  const dropTest = createDropTest(world);
  onKeyPress('KeyX', dropTest);
  return { togglePause, dropTest };
}

/** Touch, motion and gamepad call the same actions the keys do (ui/input-merge.ts). */
function startInputs(
  params: URLSearchParams,
  state: RunState,
  lives: LivesState,
  actions: Pick<
    GameActions,
    'requestStunt' | 'togglePause' | 'dropTest' | 'cycleWind' | 'toggleAirMode'
  >,
): ExtraInputs {
  const extra = startExtraInputs({
    ...actions,
    isPaused: () => state.paused,
    isAirborne: () =>
      state.started && !lives.over && !hudValues.crashed && hudValues.wheelsTouching === 0,
  });
  // The title keeps the game's clock still until the player starts (?play skips it).
  createTitle(requireElement('title'), requireElement('title-play'), titleWanted(params), () => {
    state.started = true;
    state.lastRenderMs = performance.now();
    extra.onPlay();
  });
  return extra;
}

/** Copies a three.js vector into a plain `{x, y, z}` object (for the test hook). */
function plainPoint(point: { x: number; y: number; z: number }): {
  x: number;
  y: number;
  z: number;
} {
  return { x: point.x, y: point.y, z: point.z };
}

/** Wires the sim, the world, the HUD and every input, then starts the game loop. */
async function start(): Promise<void> {
  const game = await createGame();
  const toggleAirMode = setupAirModeToggle(game);
  const params = new URLSearchParams(window.location.search);
  startPolicy(game, params);
  const perf = createPerfOverlay(requireElement('hud-perf'), performance.now());
  onKeyPress('F3', () => togglePerfOverlay(perf));
  const state: RunState = { started: false, paused: false, lastRenderMs: 0, stuntRequests: 0 };
  const { stuntQueue, requestStunt } = setupStunts(state);

  const { world, hudParts } = setupWorld(game);
  const cycleWind = setupWind(game, params);
  const courses = createCourseControls(world, params, playgroundJson);
  courses.startFreeDrive();
  const lives = setupLives(game, params);
  state.lastRenderMs = performance.now();
  const { togglePause, dropTest } = setupControls(game, world, state, lives, courses, () =>
    extra.releaseHeld(),
  );
  const extra = startInputs(params, state, lives, {
    requestStunt,
    togglePause,
    dropTest,
    cycleWind,
    toggleAirMode,
  });
  const reducedMotion = prefersReducedMotion();
  const ctx: LoopContext = {
    game,
    world,
    hudParts,
    hudValues,
    lives,
    state,
    perf,
    stuntQueue,
    extra,
    reducedMotion,
  };
  // the game loop: physics ticks at 120 hz, one render per browser frame
  startGameLoop(Game.physics_dt_s(), createLoopCallbacks(ctx));
  installDebugHook(ctx, params, courses);
}

start().catch((error: unknown) => {
  console.error('Nine Lives failed to start:', error);
});
