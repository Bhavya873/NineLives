// What happens on each physics tick and each rendered frame: step the sim, react to its
// events, draw the scene and update the HUD. main.ts builds the context and passes it in.
import { Vector3 } from 'three';

import { setText } from '../shared/dom.ts';
import { updateAirGauge } from '../ui/air-gauge.ts';
import { nozzleWhoText, updateHud } from '../ui/hud.ts';
import { isAnyNozzleFiring, updateZones } from '../ui/hud-zones.ts';
import { grantFreeRespawns, savesThisRun, updateLives, updateSavesCounter } from '../ui/lives.ts';
import { updateNozzleMap } from '../ui/nozzle-map.ts';
import { readMergedFrame } from '../ui/input-merge.ts';
import { isNozzleKeyHeld } from '../ui/keyboard.ts';
import { recordFrame, recordTick } from '../ui/perf-overlay.ts';
import { showNoAirIfNew, showToast, toastForEvents, updateToasts } from '../ui/toasts.ts';
import { followCar, widenWithSpeed } from '../render/chase-camera.ts';
import {
  cameraShake,
  startExplosion,
  stopExplosion,
  updateExplosion,
} from '../render/explosion.ts';
import { updateFlames } from '../render/flames.ts';
import { placeCar, setCarVisible } from '../render/scene.ts';
import { renderScreenEffects, resetScreenEffects } from '../render/screen-effects.ts';
import type { HudValues } from '../ui/hud.ts';
import { containsEvent } from '../wasm-bridge/sim-events.ts';
import { fillHudValues } from '../wasm-bridge/hud-values.ts';
import { requestStuntInSim } from '../wasm-bridge/stunt-adapter.ts';
import { EventKind, type Game } from '../wasm/nl_wasm.js';
import type { ExtraInputs } from './extra-inputs.ts';
import type { HudParts } from './hud-parts.ts';
import { countOutOfBoundsCrashes, hasRealCrash } from './out-of-bounds.ts';
import {
  applyStuntEvents,
  expireStunt,
  type createStuntQueue,
  stuntToastForEvents,
  submitPendingStunt,
} from './stunt-requests.ts';
import { snapViewToGame, type World } from './world.ts';
import type { createLives } from '../ui/lives.ts';
import type { createPerfOverlay } from '../ui/perf-overlay.ts';

/** Reused each frame so the render loop allocates nothing for these. */
const shake = new Vector3();
const crashPosition = new Vector3();

/** The few values that change while the game runs. */
export interface RunState {
  /** False until the player leaves the title; the game's clock stays still. */
  started: boolean;
  paused: boolean;
  /** Browser time of the last rendered frame, in ms. */
  lastRenderMs: number;
  /** Counts the stunt requests, for the test hook. */
  stuntRequests: number;
}

/** Everything the tick and render steps read or update. */
export interface LoopContext {
  game: Game;
  world: World;
  hudParts: HudParts;
  hudValues: HudValues;
  lives: ReturnType<typeof createLives>;
  state: RunState;
  perf: ReturnType<typeof createPerfOverlay>;
  stuntQueue: ReturnType<typeof createStuntQueue>;
  extra: ExtraInputs;
  reducedMotion: boolean;
}

/**
 * Reacts to this frame's sim events: a crash starts the explosion where the wreck is
 * frozen (SPEC §7.9), a respawn ends it and snaps the view to the checkpoint, and
 * saves and crashes get a toast. The sim decides all of it; the view only follows.
 * Flying out of bounds is no wreck: no explosion, and the life is given back above.
 */
function reactToEvents(ctx: LoopContext, events: Float32Array, nowMs: number): void {
  const { game, world, lives, state, stuntQueue } = ctx;
  if (hasRealCrash(events)) {
    startExplosion(world.explosion, crashPosition);
  }
  if (containsEvent(events, EventKind.Respawn)) {
    stopExplosion(world.explosion);
    // Jump straight to the checkpoint: no blending from the crash site, no camera swing.
    snapViewToGame(world);
    resetScreenEffects(world.effects);
  }
  applyStuntEvents(stuntQueue, events);
  expireStunt(stuntQueue, nowMs / 1000);
  submitPendingStunt(
    stuntQueue,
    (code) => !state.paused && !lives.over && requestStuntInSim(game, code),
  );
  const toast = stuntToastForEvents(events) ?? toastForEvents(events);
  // Nothing covers the game-over screen.
  if (toast && !lives.over) {
    showToast(ctx.hudParts.toasts, toast, nowMs);
  }
}

/** One fixed physics step (120 Hz), skipped while the game is not running. */
function tickPhysics(ctx: LoopContext): void {
  const { game, state, lives, extra, perf } = ctx;
  const frames = ctx.world.frames;
  if (!state.started || state.paused || lives.over || extra.portraitBlocked()) {
    return;
  }
  const tickStartMs = performance.now();
  frames.previousChassis = frames.latestChassis;
  frames.previousWheels = frames.latestWheels;
  game.tick(readMergedFrame());
  frames.latestChassis = game.chassis_pose();
  frames.latestWheels = game.wheel_poses();
  // Remember where the wreck froze, even if it respawns before the next frame.
  if (game.is_crashed()) {
    crashPosition.fromArray(frames.latestChassis);
  }
  recordTick(perf, performance.now() - tickStartMs);
}

/** Reads the sim's events and HUD values once, then updates the lives and reacts. */
function readSimState(ctx: LoopContext, nowMs: number): Float32Array {
  const { game, hudValues, lives } = ctx;
  // Events queue up in the sim, so reading them once per frame is enough.
  // The HUD's save and crash counts update as events drain, so drain first.
  const events = game.drain_events();
  fillHudValues(game.hud(), hudValues);
  // An out-of-bounds crash is a free respawn: give the life back before the hearts draw.
  grantFreeRespawns(lives, countOutOfBoundsCrashes(events));
  updateLives(lives, hudValues, nowMs);
  reactToEvents(ctx, events, nowMs);
  return events;
}

/** Places the car, moves the camera and draws the 3D scene. */
function drawScene(ctx: LoopContext, blend: number, throttles: Float32Array, frameS: number): void {
  const { world, hudValues } = ctx;
  const { view, carPose } = world;
  placeCar(view, world.frames, blend, carPose);
  setCarVisible(view, !hudValues.crashed);
  updateFlames(ctx.hudParts.flames, throttles);
  followCar(view.camera, carPose, frameS, hudValues.wheelsTouching === 0);
  if (!ctx.reducedMotion) {
    widenWithSpeed(view.camera, hudValues.speedMS, frameS);
  }
  updateExplosion(world.explosion, frameS);
  // Shake only this frame's picture, so the camera's smoothing never sees it.
  view.camera.position.add(cameraShake(world.explosion, shake));
  view.renderer.render(view.scene, view.camera);
  view.camera.position.sub(shake);
}

/** Draws the screen-space effects over the scene (g-load, wind, hints). */
function drawScreenEffects(
  ctx: LoopContext,
  events: Float32Array,
  frameS: number,
  nowMs: number,
): void {
  const { hudValues, world } = ctx;
  renderScreenEffects(world.effects, {
    gLoad: hudValues.gLoad,
    peakG: hudValues.peakG,
    policyOutOfEnvelope: hudValues.policyOutOfEnvelope,
    approachHintMS: hudValues.approachHintMS,
    events,
    windWorldM_S: hudValues.windWorldM_S,
    crashed: hudValues.crashed,
    camera: world.view.camera,
    carPosition: world.carPose.position,
    frameS,
    nowS: nowMs / 1000,
  });
}

/** Updates the HUD readouts, the nozzle map, the zone tiles and the toasts. */
function updateHudWidgets(ctx: LoopContext, throttles: Float32Array, nowMs: number): void {
  const { hudParts, hudValues, lives } = ctx;
  updateHud(hudParts.hud, hudValues);
  updateAirGauge(hudParts.airGauge, hudValues.airFraction, hudValues.unlimitedAir);
  updateNozzleMap(hudParts.nozzleDots, throttles);
  setText(hudParts.nozzleWho, nozzleWhoText(hudValues.controller));
  updateSavesCounter(hudParts.savesCounter, savesThisRun(lives, hudValues));
  updateZones(hudParts.zoneTiles, hudParts.zoneHolds, {
    thrusterKeyHeld: isNozzleKeyHeld(),
    anyNozzleFiring: isAnyNozzleFiring(throttles),
    gLoad: hudValues.crashed ? 1 : hudValues.gLoad,
    nowMs,
  });
  ctx.extra.renderFrame(nowMs);
  updateToasts(hudParts.toasts, nowMs);
  showNoAirIfNew(hudParts.toasts, hudValues.outOfAir, hudValues.airFraction, nowMs);
}

/** One rendered frame: read the sim, draw the scene, then the effects and the HUD. */
function renderFrame(ctx: LoopContext, blend: number): void {
  const { state } = ctx;
  const nowMs = performance.now();
  const frameS = (nowMs - state.lastRenderMs) / 1000;
  state.lastRenderMs = nowMs;
  const events = readSimState(ctx, nowMs);
  // Valve lag already smooths the throttles, so one read per frame is enough.
  const throttles = ctx.game.nozzle_throttles();
  drawScene(ctx, blend, throttles, frameS);
  drawScreenEffects(ctx, events, frameS, nowMs);
  updateHudWidgets(ctx, throttles, nowMs);
  const frameEndMs = performance.now();
  recordFrame(ctx.perf, frameEndMs - nowMs, frameEndMs);
}

/** The two callbacks the game loop calls: a physics tick and a rendered frame. */
export function createLoopCallbacks(ctx: LoopContext): {
  tick: () => void;
  render: (blend: number) => void;
} {
  return {
    tick: () => tickPhysics(ctx),
    render: (blend) => renderFrame(ctx, blend),
  };
}
