// Wires the HUD's screen effects to the game: the g-force blackout (hud/g-blackout.ts,
// fed by the sim's g when it has one, else by hud/g-load-estimate.ts) and the wind
// readout and streaks (hud/wind-indicator.ts, render/wind-streaks.ts). main.ts calls
// it once per physics tick and once per frame. Rendering and HUD only.
import { type PerspectiveCamera, type Scene, Vector3 } from 'three';

import { requireElement } from './dom.ts';
import {
  applyGBlackout,
  createGBlackoutView,
  type GBlackoutState,
  type GBlackoutView,
  newGBlackoutState,
  updateGBlackout,
} from './hud/g-blackout.ts';
import {
  type GLoadEstimator,
  newGLoadEstimator,
  resetGLoadEstimator,
  updateGLoadEstimate,
} from './hud/g-load-estimate.ts';
import {
  cameraYawRad,
  createWindIndicator,
  updateWindIndicator,
  type WindIndicator,
} from './hud/wind-indicator.ts';
import { createWindStreaks, updateWindStreaks, type WindStreaks } from './render/wind-streaks.ts';

/** Strongest wind the `?wind=` preview accepts, in m/s. */
const MAX_PREVIEW_WIND_M_S = 60;

/** Everything the screen effects keep between frames. */
export interface ScreenEffects {
  blackout: GBlackoutState;
  blackoutView: GBlackoutView;
  gEstimate: GLoadEstimator;
  windIndicator: WindIndicator;
  windStreaks: WindStreaks;
  /** The wind, [x, y, z] in m/s in the world frame: where the air moves to. */
  windWorldM_S: [number, number, number];
  /** Reused each frame for the camera's look direction. */
  lookDirection: Vector3;
}

/** Finds the effect elements in the page and adds the wind streaks to `scene`. */
export function createScreenEffects(
  scene: Scene,
  reducedMotion: boolean,
  params: URLSearchParams,
): ScreenEffects {
  return {
    blackout: newGBlackoutState(),
    blackoutView: createGBlackoutView(requireElement('g-blackout'), reducedMotion),
    gEstimate: newGLoadEstimator(),
    windIndicator: createWindIndicator(
      requireElement('hud-wind'),
      requireElement('hud-wind-arrow'),
      requireElement('hud-wind-label'),
    ),
    windStreaks: createWindStreaks(scene),
    // TODO: read the wind from the sim once `Game` exposes it; zero wind until then.
    windWorldM_S: previewWind(params),
    lookDirection: new Vector3(),
  };
}

/** Feeds one physics tick's chassis pose (position first) to the g estimate. */
export function tickScreenEffects(
  effects: ScreenEffects,
  chassis: Float32Array,
  dtS: number,
): void {
  const position: [number, number, number] = [chassis[0] ?? 0, chassis[1] ?? 0, chassis[2] ?? 0];
  updateGLoadEstimate(effects.gEstimate, position, dtS);
}

/** Forgets the car's motion history after a jump (respawn, drop test, new course). */
export function resetScreenEffects(effects: ScreenEffects): void {
  resetGLoadEstimator(effects.gEstimate);
}

/** What one frame of effects needs to know. */
export interface EffectsFrame {
  /** The sim's seat g when it reports one (TODO: `Game` HUD field `g_load`). */
  simGLoad: number | undefined;
  /** True while the wreck waits to respawn; the view stays clear then. */
  crashed: boolean;
  camera: PerspectiveCamera;
  carPosition: Vector3;
  frameS: number;
  nowS: number;
}

/** Updates the blackout, the wind readout and the streaks for one rendered frame. */
export function renderScreenEffects(effects: ScreenEffects, frame: EffectsFrame): void {
  const gLoad = frame.crashed ? 1 : (frame.simGLoad ?? effects.gEstimate.gLoad);
  const darkness = updateGBlackout(effects.blackout, gLoad, frame.frameS);
  applyGBlackout(effects.blackoutView, darkness, frame.nowS);

  frame.camera.getWorldDirection(effects.lookDirection);
  const yaw = cameraYawRad(effects.lookDirection.x, effects.lookDirection.z);
  updateWindIndicator(effects.windIndicator, effects.windWorldM_S, yaw);
  updateWindStreaks(effects.windStreaks, effects.windWorldM_S, frame.carPosition, frame.frameS);
}

/**
 * A preview wind from the page address, e.g. `?wind=8,-3` for 8 m/s along world +X and
 * 3 m/s along −Z, so the readout can be seen before the sim has wind. Anything that is
 * not two numbers gives no wind.
 */
export function previewWind(params: URLSearchParams): [number, number, number] {
  const parts = (params.get('wind') ?? '').split(',').map(Number);
  const [x = NaN, z = NaN] = parts;
  if (parts.length !== 2 || !Number.isFinite(x) || !Number.isFinite(z)) {
    return [0, 0, 0];
  }
  const clamp = (value: number) =>
    Math.max(-MAX_PREVIEW_WIND_M_S, Math.min(MAX_PREVIEW_WIND_M_S, value));
  return [clamp(x), 0, clamp(z)];
}
