// The on-screen display (SPEC §11.5). So far: speed, mode, air and why Hop is
// unavailable. Later M3 PRs add saves, crashes, toasts and the nozzle map.
// It reads plain numbers and writes text; it never touches physics.

import { requireElement } from '../dom';

const KM_PER_H_PER_M_PER_S = 3.6;

/** Why a Hop is unavailable, matching the wasm HopBlocked values. */
const HOP_BLOCKED_TEXT = ['', 'Hop: wheels off the ground', 'Hop: cooldown', 'Hop: no air'];

/** The numbers the HUD shows, already read from the game. */
export interface HudValues {
  speedMS: number;
  /** "realistic", "arcade" or "custom". */
  modeName: string;
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
  const mode = values.modeName.charAt(0).toUpperCase() + values.modeName.slice(1);
  hud.status.textContent = [mode, `Air ${air}`, hop].filter(Boolean).join(' · ');
}
