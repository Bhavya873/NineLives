// The on-screen display's text (SPEC §11.5): speed, mode, why Hop is unavailable, and
// the top-right stats (saves, lives used, time, Hop cooldown). The air gauge, nozzle
// map and toasts each have their own file in this folder.
// It reads plain numbers and writes text; it never touches physics.

import { requireElement, setText } from '../dom.ts';
import { ControllerCode, HopBlocked, ModeCode } from '../wasm/nl_wasm.js';

const KM_PER_H_PER_M_PER_S = 3.6;

/** What the HUD says when a Hop is unavailable, by wasm `HopBlocked` value. */
const HOP_BLOCKED_TEXT: Record<number, string> = {
  [HopBlocked.No]: '',
  [HopBlocked.NotOnGround]: 'Hop: wheels off the ground',
  [HopBlocked.Cooldown]: 'Hop: cooldown',
  [HopBlocked.NoAir]: 'Hop: no air',
};

/** Mode names, by wasm `ModeCode` value. */
const MODE_TEXT: Record<number, string> = {
  [ModeCode.Realistic]: 'Realistic',
  [ModeCode.Arcade]: 'Arcade',
  [ModeCode.Custom]: 'Custom',
};

/** Controller names, by wasm `ControllerCode` value. */
const CONTROLLER_TEXT: Record<number, string> = {
  [ControllerCode.Off]: 'Controller: Off',
  [ControllerCode.Manual]: 'Controller: Manual (keys 1–0)',
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
}

/** The HUD's elements, looked up once at start-up. */
export interface Hud {
  speed: HTMLElement;
  status: HTMLElement;
  stats: HTMLElement;
}

/** Finds the HUD elements in the page. Throws if the page is missing one. */
export function findHud(): Hud {
  return {
    speed: requireElement('hud-speed'),
    status: requireElement('hud-status'),
    stats: requireElement('hud-stats'),
  };
}

/** Updates the HUD from the game's latest values. */
export function updateHud(hud: Hud, values: HudValues): void {
  const speedKmH = Math.abs(values.speedMS) * KM_PER_H_PER_M_PER_S;
  setText(hud.speed, `${speedKmH.toFixed(0)} km/h`);

  const hop = HOP_BLOCKED_TEXT[values.hopBlocked] ?? '';
  const mode = MODE_TEXT[values.mode] ?? '';
  const controller = CONTROLLER_TEXT[values.controller] ?? '';
  const crash = values.crashed ? 'Respawning…' : '';
  setText(hud.status, [mode, controller, hop, crash].filter(Boolean).join(' · '));
  setText(hud.stats, statsText(values));
}

/** The top-right lines: saves, lives used, time, and the Hop cooldown while it runs. */
export function statsText(values: HudValues): string {
  const lines = [
    `Saves ${values.saves}`,
    `Lives used ${values.crashes}`,
    `Time ${formatTime(values.timeS)}`,
  ];
  if (values.hopCooldownS > 0) {
    lines.push(`Hop in ${values.hopCooldownS.toFixed(1)} s`);
  }
  return lines.join('\n');
}

/** Minutes and seconds with tenths, e.g. 1:05.3. */
function formatTime(timeS: number): string {
  const minutes = Math.floor(timeS / 60);
  const seconds = (timeS - minutes * 60).toFixed(1).padStart(4, '0');
  return `${minutes}:${seconds}`;
}
