// Wires the HUD's screen effects to the game: the g-meter and g record (hud/g-meter.ts,
// hud/g-record.ts), the approach HUD (hud/approach-hud.ts, render/sparks.ts) and the wind readout and streaks (hud/wind-indicator.ts,
// render/wind-streaks.ts), plus the wind level setting. main.ts calls it once per
// frame with values from `Game.hud()`. Rendering and HUD only.
import { type PerspectiveCamera, type Scene, Vector3 } from 'three';

import { requireElement } from './dom.ts';
import {
  type TooSlowState,
  newTooSlowState,
  outsideEnvelope,
  scrapeSpeeds,
  updateTooSlow,
} from './hud/approach-hud.ts';
import { type GMeter, createGMeter, updateGMeter } from './hud/g-meter.ts';
import {
  type GRecord,
  browserStorage,
  createGRecord,
  recordLandings,
  recordLine,
  recordPeak,
} from './hud/g-record.ts';
import {
  cameraYawRad,
  createWindIndicator,
  updateWindIndicator,
  type WindIndicator,
} from './hud/wind-indicator.ts';
import { type Sparks, createSparks, updateSparks } from './render/sparks.ts';
import { createWindStreaks, updateWindStreaks, type WindStreaks } from './render/wind-streaks.ts';

/** The wind settings `Game.set_wind_level` takes, in the order V cycles them. */
export const WIND_LEVELS = ['off', 'breeze', 'storm'] as const;

/** One of the `WIND_LEVELS`. */
export type WindLevel = (typeof WIND_LEVELS)[number];

/** Everything the screen effects keep between frames. */
export interface ScreenEffects {
  gMeter: GMeter;
  gRecord: GRecord;
  gRecordElement: HTMLElement;
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
    gRecord: createGRecord(browserStorage()),
    gRecordElement: requireElement('hud-g-record'),
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
export interface EffectsFrame {
  /** The sim's passenger g at the seat (smoothed over 0.1 s). */
  gLoad: number;
  /** The raw peak g of the current stunt window (HudField PeakGLoad). */
  peakG: number;
  /** HudField PolicyOutOfEnvelope. */
  policyOutOfEnvelope: number;
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
  updateGMeter(effects.gMeter, frame.crashed ? 1 : frame.gLoad, frame.peakG);
  recordPeak(effects.gRecord, frame.peakG);
  recordLandings(effects.gRecord, frame.events, frame.nowS);
  const line = recordLine(effects.gRecord, frame.nowS);
  effects.gRecordElement.textContent = line;
  effects.gRecordElement.hidden = line === '';

  const tooSlow = updateTooSlow(effects.tooSlow, frame.events, frame.nowS);
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
