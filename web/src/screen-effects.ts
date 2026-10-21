// Wires the HUD's screen effects to the game: the g-force blackout (hud/g-blackout.ts,
// fed by the sim's seat g) and the wind readout and streaks (hud/wind-indicator.ts,
// render/wind-streaks.ts), plus the wind level setting. main.ts calls it once per
// frame with values from `Game.hud()`. Rendering and HUD only.
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
  cameraYawRad,
  createWindIndicator,
  updateWindIndicator,
  type WindIndicator,
} from './hud/wind-indicator.ts';
import { createWindStreaks, updateWindStreaks, type WindStreaks } from './render/wind-streaks.ts';

/** The wind settings `Game.set_wind_level` takes, in the order V cycles them. */
export const WIND_LEVELS = ['off', 'breeze', 'storm'] as const;

/** One of the `WIND_LEVELS`. */
export type WindLevel = (typeof WIND_LEVELS)[number];

/** Everything the screen effects keep between frames. */
export interface ScreenEffects {
  blackout: GBlackoutState;
  blackoutView: GBlackoutView;
  windIndicator: WindIndicator;
  windStreaks: WindStreaks;
  /** Reused each frame for the camera's look direction. */
  lookDirection: Vector3;
}

/** Finds the effect elements in the page and adds the wind streaks to `scene`. */
export function createScreenEffects(scene: Scene, reducedMotion: boolean): ScreenEffects {
  return {
    blackout: newGBlackoutState(),
    blackoutView: createGBlackoutView(requireElement('g-blackout'), reducedMotion),
    windIndicator: createWindIndicator(
      requireElement('hud-wind'),
      requireElement('hud-wind-arrow'),
      requireElement('hud-wind-label'),
    ),
    windStreaks: createWindStreaks(scene),
    lookDirection: new Vector3(),
  };
}

/** Clears the blackout after a jump (respawn, drop test, new course). */
export function resetScreenEffects(effects: ScreenEffects): void {
  effects.blackout = newGBlackoutState();
}

/** What one frame of effects needs to know. */
export interface EffectsFrame {
  /** The sim's passenger g at the seat. */
  gLoad: number;
  /** The sim's wind now, [x, y, z] in m/s in the world frame. */
  windWorldM_S: [number, number, number];
  /** True while the wreck waits to respawn; the view stays clear then. */
  crashed: boolean;
  camera: PerspectiveCamera;
  carPosition: Vector3;
  frameS: number;
  nowS: number;
}

/** Updates the blackout, the wind readout and the streaks for one rendered frame. */
export function renderScreenEffects(effects: ScreenEffects, frame: EffectsFrame): void {
  const gLoad = frame.crashed ? 1 : frame.gLoad;
  const darkness = updateGBlackout(effects.blackout, gLoad, frame.frameS);
  applyGBlackout(effects.blackoutView, darkness, frame.nowS);

  frame.camera.getWorldDirection(effects.lookDirection);
  const yaw = cameraYawRad(effects.lookDirection.x, effects.lookDirection.z);
  updateWindIndicator(effects.windIndicator, frame.windWorldM_S, yaw);
  updateWindStreaks(effects.windStreaks, frame.windWorldM_S, frame.carPosition, frame.frameS);
}

/** The wind level from the page address (`?wind=breeze`); anything else gives Off. */
export function startingWindLevel(params: URLSearchParams): WindLevel {
  const asked = params.get('wind');
  return WIND_LEVELS.find((level) => level === asked) ?? 'off';
}

/** The level after `level` when V is pressed: Off → Breeze → Storm → Off. */
export function nextWindLevel(level: WindLevel): WindLevel {
  const next = (WIND_LEVELS.indexOf(level) + 1) % WIND_LEVELS.length;
  return WIND_LEVELS[next] ?? 'off';
}
