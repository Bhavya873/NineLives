// The HUD's text (SPEC §11.5): the status line, a short alert and the speed. The air gauge,
// nozzle map and toasts have their own files. It reads plain numbers and writes text;
// it never touches physics.

import { requireElement, setText } from '../shared/dom.ts';
import { isFast } from './approach-hud.ts';
import { ControllerCode, HopBlocked, ModeCode } from '../wasm/nl_wasm.js';

const KM_PER_H_PER_M_PER_S = 3.6;

/** What the alert says when a Hop is unavailable, by wasm `HopBlocked` value. */
export const HOP_BLOCKED_TEXT: Record<number, string> = {
  [HopBlocked.No]: '',
  [HopBlocked.NotOnGround]: 'wheels down first',
  [HopBlocked.Cooldown]: 'hop ready soon',
  [HopBlocked.NoAir]: 'no air to hop',
};

/** Mode names, by wasm `ModeCode` value. */
export const MODE_TEXT: Record<number, string> = {
  [ModeCode.Realistic]: 'Limited air',
  [ModeCode.Arcade]: 'Unlimited air',
  [ModeCode.Custom]: 'Custom',
};

/** Under the nozzle map: who fires the nozzles, from a wasm `ControllerCode` value. */
export function nozzleWhoText(controller: number): string {
  const policyFlying = controller === ControllerCode.Ai || controller === ControllerCode.HybridAi;
  return policyFlying ? 'AI flying' : 'Thrusters';
}

/** The numbers the HUD shows, already read from the game. */
export interface HudValues {
  speedMS: number;
  /** A wasm `ModeCode` value. */
  mode: number;
  /** A wasm `ControllerCode` value. */
  controller: number;
  /** Air left, 0..1. unlimited air mode has no limit. */
  airFraction: number;
  unlimitedAir: boolean;
  /** A HopBlocked value: 0 means a Hop is allowed. */
  hopBlocked: number;
  /** True from a crash until the respawn. */
  crashed: boolean;
  /** Saves so far. */
  saves: number;
  /** Crashes so far; each one costs a heart. */
  crashes: number;
  /** Simulated time since the game started. */
  timeS: number;
  /** Seconds until the next Hop is allowed. */
  hopCooldownS: number;
  /** How many wheels touch the ground (0 to 4); the camera pulls back at 0. */
  wheelsTouching: number;
  /** True while the tank is so low that thrust fades (the sim decides). */
  outOfAir: boolean;
  /** All the air used since the run started, in kg. */
  airUsedKg: number;
  /** Course checkpoints behind the car and on the course (both 0 on flat ground). */
  checkpointsReached: number;
  checkpointCount: number;
  /** True once the car crossed the course's finish line. */
  finished: boolean;
  /** Course time at the finish, in s. */
  finishTimeS: number;
  /** Passenger load at the driver's seat, in g (1 sitting still). */
  gLoad: number;
  /** Raw peak g of the current stunt window. */
  peakG: number;
  /** HudField PolicyOutOfEnvelope: 1 when the AI is outside where it was validated. */
  policyOutOfEnvelope: number;
  /** HudField ApproachHint: the entry speed the piece ahead needs, in m/s (0 = none). */
  approachHintMS: number;
  /** The wind now, [x, y, z] in m/s in the world frame: where the air moves to. */
  windWorldM_S: [number, number, number];
}

/** The HUD's elements, looked up once at start-up. */
interface Hud {
  /** The speed number (its "km/h" unit is fixed in the page). */
  speed: HTMLElement;
  status: HTMLElement;
  alert: HTMLElement;
}

/** Finds the HUD elements in the page. Throws if the page is missing one. */
export function findHud(): Hud {
  return {
    speed: requireElement('hud-speed-value'),
    status: requireElement('hud-status'),
    alert: requireElement('hud-alert'),
  };
}

/** Updates the HUD from the game's latest values. */
export function updateHud(hud: Hud, values: HudValues): void {
  const speedKmH = Math.abs(values.speedMS) * KM_PER_H_PER_M_PER_S;
  setText(hud.speed, speedKmH.toFixed(0));
  hud.speed.parentElement?.classList.toggle('fast', isFast(values.speedMS));
  setText(hud.status, statusText(values));
  const alert = alertText(values);
  setText(hud.alert, alert);
  hud.alert.hidden = alert === '';
}

/** The status line: the mode and the course section. The thrusters always fly
 * themselves, so the controller is not shown. */
function statusText(values: HudValues): string {
  const mode = MODE_TEXT[values.mode] ?? '';
  return [mode, courseText(values)].filter(Boolean).join(', ');
}

/** A short note while something is unavailable: respawning, or why Hop won't fire. */
function alertText(values: HudValues): string {
  if (values.crashed) {
    return 'respawning';
  }
  if (values.hopBlocked === HopBlocked.Cooldown && values.hopCooldownS > 0) {
    return `hop ready in ${Math.ceil(values.hopCooldownS)} s`;
  }
  return HOP_BLOCKED_TEXT[values.hopBlocked] ?? '';
}

/** The course section (SPEC §11.5), e.g. "Checkpoint 2/6"; empty on flat ground. */
function courseText(values: HudValues): string {
  if (values.finished) {
    return 'Finished';
  }
  if (values.checkpointCount === 0) {
    return '';
  }
  return `Checkpoint ${values.checkpointsReached}/${values.checkpointCount}`;
}
