// Draws the screen effects of the game and updates their HUD parts, once per frame:
// the g-meter (ui/g-meter.ts), the approach HUD (ui/approach-hud.ts,
// sparks.ts), the wind readout and streaks (ui/wind-indicator.ts, wind-streaks.ts) and the wind level.
// game/main.ts calls it with values from `Game.hud()`. Rendering and HUD only.
import { type PerspectiveCamera, type Scene, Vector3 } from 'three';

import { requireElement } from '../shared/dom.ts';
import {
  type TooSlowState,
  newTooSlowState,
  approachHintText,
  outsideEnvelope,
  scrapeSpeeds,
  updateTooSlow,
} from '../ui/approach-hud.ts';
import {
  type GMeter,
  type GPeakTracker,
  createGMeter,
  createGPeakTracker,
  updateGMeter,
  updateGPeak,
} from '../ui/g-meter.ts';
import {
  cameraYawRad,
  createWindIndicator,
  updateWindIndicator,
  type WindIndicator,
} from '../ui/wind-indicator.ts';
import { type Sparks, createSparks, updateSparks } from './sparks.ts';
import { createWindStreaks, updateWindStreaks, type WindStreaks } from './wind-streaks.ts';

/** The wind settings `Game.set_wind_level` takes, in the order V cycles them. */
const WIND_LEVELS = ['off', 'breeze', 'storm'] as const;

/** One of the `WIND_LEVELS`. */
export type WindLevel = (typeof WIND_LEVELS)[number];

/** Everything the screen effects keep between frames. */
interface ScreenEffects {
  gMeter: GMeter;
  /** The headline peak the meter shows (raw peak stays in the sim). */
  gPeak: GPeakTracker;
  sparks: Sparks;
  tooSlow: TooSlowState;
  tooSlowElement: HTMLElement;
  envelopeBadge: HTMLElement;
  windIndicator: WindIndicator;
  windStreaks: WindStreaks;
  /** Reused each frame for the camera's look direction. */
  lookDirection: Vector3;
}

/** Finds the effect elements in the page and adds the wind streaks to `scene`. */
export function createScreenEffects(scene: Scene): ScreenEffects {
  return {
    gMeter: createGMeter(
      requireElement('hud-g'),
      requireElement('hud-g-fill'),
      requireElement('hud-g-live'),
      requireElement('hud-g-peak'),
    ),
    gPeak: createGPeakTracker(),
    sparks: createSparks(scene),
    tooSlow: newTooSlowState(),
    tooSlowElement: requireElement('hud-too-slow'),
    envelopeBadge: requireElement('hud-envelope'),
    windIndicator: createWindIndicator(
      requireElement('hud-wind'),
      requireElement('hud-wind-arrow'),
      requireElement('hud-wind-label'),
    ),
    windStreaks: createWindStreaks(scene),
    lookDirection: new Vector3(),
  };
}

/** Clears the warnings after a jump (respawn, drop test, new course). */
export function resetScreenEffects(effects: ScreenEffects): void {
  effects.tooSlow = newTooSlowState();
  effects.tooSlowElement.hidden = true;
}

/** What one frame of effects needs to know. */
interface EffectsFrame {
  /** The sim's passenger g at the seat (smoothed over 0.1 s). */
  gLoad: number;
  /** The raw peak g of the current stunt window (HudField PeakGLoad). */
  peakG: number;
  /** HudField PolicyOutOfEnvelope. */
  policyOutOfEnvelope: number;
  /** HudField ApproachHint: the entry speed the piece ahead needs, in m/s (0 = none). */
  approachHintMS: number;
  /** This frame's drained events. */
  events: Float32Array;
  /** The sim's wind now, [x, y, z] in m/s in the world frame. */
  windWorldM_S: [number, number, number];
  /** True while the wreck waits to respawn; the view stays clear then. */
  crashed: boolean;
  camera: PerspectiveCamera;
  carPosition: Vector3;
  frameS: number;
  nowS: number;
}

/** Updates the g-meter, approach HUD, wind readout and streaks for one rendered frame. */
export function renderScreenEffects(effects: ScreenEffects, frame: EffectsFrame): void {
  const liveG = frame.crashed ? 1 : frame.gLoad;
  updateGMeter(effects.gMeter, liveG, updateGPeak(effects.gPeak, liveG, frame.peakG), frame.peakG);

  // The warning after a too-slow entry wins; before the piece the live hint shows.
  const tooSlow =
    updateTooSlow(effects.tooSlow, frame.events, frame.nowS) ||
    approachHintText(frame.approachHintMS);
  effects.tooSlowElement.textContent = tooSlow;
  effects.tooSlowElement.hidden = tooSlow === '';
  effects.envelopeBadge.hidden = !outsideEnvelope(frame.policyOutOfEnvelope);
  updateSparks(
    effects.sparks,
    scrapeSpeeds(frame.events),
    { x: frame.carPosition.x, y: frame.carPosition.y, z: frame.carPosition.z },
    frame.frameS,
  );

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
