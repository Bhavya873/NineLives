// The on-screen display (SPEC §11.5). So far: speed, mode, air and why Hop is
// unavailable. Later M3 PRs add saves, crashes, toasts and the nozzle map.
// It reads plain numbers and writes text; it never touches physics.

import { requireElement } from '../dom';
import { HopBlocked, ModeCode } from '../wasm/nl_wasm.js';

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

/** The numbers the HUD shows, already read from the game. */
export interface HudValues {
  speedMS: number;
  /** A wasm `ModeCode` value. */
  mode: number;
  /** Air left, 0..1. Arcade has unlimited air. */
  airFraction: number;
  unlimitedAir: boolean;
  /** A HopBlocked value: 0 means a Hop is allowed. */
  hopBlocked: number;
}

/** The HUD's elements, looked up once at start-up. */
export interface Hud {
  speed: HTMLElement;
  status: HTMLElement;
}

/** Finds the HUD elements in the page. Throws if the page is missing one. */
export function findHud(): Hud {
  return { speed: requireElement('hud-speed'), status: requireElement('hud-status') };
}

/** Updates the HUD from the game's latest values. */
export function updateHud(hud: Hud, values: HudValues): void {
  const speedKmH = Math.abs(values.speedMS) * KM_PER_H_PER_M_PER_S;
  hud.speed.textContent = `${speedKmH.toFixed(0)} km/h`;

  const air = values.unlimitedAir ? '∞' : `${Math.round(values.airFraction * 100)}%`;
  const hop = HOP_BLOCKED_TEXT[values.hopBlocked] ?? '';
  const mode = MODE_TEXT[values.mode] ?? '';
  hud.status.textContent = [mode, `Air ${air}`, hop].filter(Boolean).join(' · ');
}
