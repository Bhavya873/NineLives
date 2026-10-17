// The on-screen display. M2 shows only speed; M3 adds air, saves, crashes and the
// nozzle map (SPEC §11.5). It reads numbers and writes text; it never touches physics.

import { requireElement } from '../dom';

const KM_PER_H_PER_M_PER_S = 3.6;

/** The HUD's elements, looked up once at start-up. */
export interface Hud {
  speed: HTMLElement;
}

/** Finds the HUD elements in the page. Throws if the page is missing one. */
export function findHud(): Hud {
  return { speed: requireElement('hud-speed') };
}

/** Updates the HUD from the game's latest values. */
export function updateHud(hud: Hud, speedMS: number): void {
  const speedKmH = Math.abs(speedMS) * KM_PER_H_PER_M_PER_S;
  hud.speed.textContent = `${speedKmH.toFixed(0)} km/h`;
}
