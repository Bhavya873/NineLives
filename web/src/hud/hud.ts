// The on-screen display's text (SPEC §11.5): the status line, a short alert, the speed
// and the menu's stats line. The air gauge, nozzle map and toasts have their own files.
// It reads plain numbers and writes text; it never touches physics.

import { requireElement, setText } from '../dom.ts';
import { ControllerCode, HopBlocked, ModeCode } from '../wasm/nl_wasm.js';

const KM_PER_H_PER_M_PER_S = 3.6;

/** What the alert says when a Hop is unavailable, by wasm `HopBlocked` value. */
const HOP_BLOCKED_TEXT: Record<number, string> = {
  [HopBlocked.No]: '',
  [HopBlocked.NotOnGround]: 'Hop needs the wheels down',
  [HopBlocked.Cooldown]: 'Hop recharging',
  [HopBlocked.NoAir]: 'Hop needs air',
};

/** Mode names, by wasm `ModeCode` value. */
const MODE_TEXT: Record<number, string> = {
  [ModeCode.Realistic]: 'Realistic',
  [ModeCode.Arcade]: 'Arcade',
  [ModeCode.Custom]: 'Custom',
};

/** Who fires the thrusters, by wasm `ControllerCode` value; "PD + AI" is the Hybrid. */
const CONTROLLER_TEXT: Record<number, string> = {
  [ControllerCode.Off]: 'Thrusters off',
  [ControllerCode.Pd]: 'PD',
  [ControllerCode.Ai]: 'AI',
  [ControllerCode.Manual]: 'Manual (keys 1–0)',
  [ControllerCode.HybridAi]: 'PD + AI · AI flying',
  [ControllerCode.HybridPd]: 'PD + AI · PD driving',
};

/** The numbers the HUD shows, already read from the game. */
export interface HudValues {
  speedMS: number;
  /** A wasm `ModeCode` value. */
  mode: number;
  /** A wasm `ControllerCode` value. */
  controller: number;
  /** Air left, 0..1. Arcade has unlimited air. */
  airFraction: number;
  unlimitedAir: boolean;
  /** A HopBlocked value: 0 means a Hop is allowed. */
  hopBlocked: number;
  /** True from a crash until the respawn. */
  crashed: boolean;
  /** Saves so far. */
  saves: number;
  /** Crashes so far, shown as "Lives used". */
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
  /** The wind now, [x, y, z] in m/s in the world frame: where the air moves to. */
  windWorldM_S: [number, number, number];
}

/** The HUD's elements, looked up once at start-up. */
export interface Hud {
  /** The speed number (its "km/h" unit is fixed in the page). */
  speed: HTMLElement;
  status: HTMLElement;
  alert: HTMLElement;
  /** The stats line on the menu. */
  stats: HTMLElement;
}

/** Finds the HUD elements in the page. Throws if the page is missing one. */
export function findHud(): Hud {
  return {
    speed: requireElement('hud-speed-value'),
    status: requireElement('hud-status'),
    alert: requireElement('hud-alert'),
    stats: requireElement('hud-stats'),
  };
}

/** Updates the HUD from the game's latest values. */
export function updateHud(hud: Hud, values: HudValues): void {
  const speedKmH = Math.abs(values.speedMS) * KM_PER_H_PER_M_PER_S;
  setText(hud.speed, speedKmH.toFixed(0));
  setText(hud.status, statusText(values));
  const alert = alertText(values);
  setText(hud.alert, alert);
  hud.alert.hidden = alert === '';
  setText(hud.stats, statsText(values));
}

/** The status line: who fires the thrusters, the mode and the course section. */
export function statusText(values: HudValues): string {
  const controller = CONTROLLER_TEXT[values.controller] ?? '';
  const mode = MODE_TEXT[values.mode] ?? '';
  return [controller, mode, courseText(values)].filter(Boolean).join(' · ');
}

/** A short note while something is unavailable: respawning, or why Hop won't fire. */
export function alertText(values: HudValues): string {
  if (values.crashed) {
    return 'Respawning…';
  }
  if (values.hopBlocked === HopBlocked.Cooldown && values.hopCooldownS > 0) {
    return `Hop in ${values.hopCooldownS.toFixed(1)} s`;
  }
  return HOP_BLOCKED_TEXT[values.hopBlocked] ?? '';
}

/** The menu's stats line: saves, lives used and time. */
export function statsText(values: HudValues): string {
  const time = formatTime(values.timeS);
  return `Saves ${values.saves} · Lives used ${values.crashes} · Time ${time}`;
}

/** The course section (SPEC §11.5), e.g. "Checkpoint 2/6"; empty on flat ground. */
export function courseText(values: HudValues): string {
  if (values.finished) {
    return 'Finished';
  }
  if (values.checkpointCount === 0) {
    return '';
  }
  return `Checkpoint ${values.checkpointsReached}/${values.checkpointCount}`;
}

/** Minutes and seconds with tenths, e.g. 1:05.3. */
export function formatTime(timeS: number): string {
  const minutes = Math.floor(timeS / 60);
  const seconds = (timeS - minutes * 60).toFixed(1).padStart(4, '0');
  return `${minutes}:${seconds}`;
}
