// The HUD's top-down nozzle map (SPEC §11.5): a car outline with one dot per nozzle that
// glows with its effective throttle, so the player can see which thrusters the AI fires.
// Plain DOM elements; it reads numbers from the game and never touches physics.

import { FLOATS_PER_NOZZLE } from '../sim-layout.ts';

/** Half the length and width of the area the map shows, in metres (the car plus a margin). */
const MAP_HALF_LENGTH_M = 2.5;
const MAP_HALF_WIDTH_M = 1.3;
/** A dot never fades out completely, so idle nozzles still show where they are. */
const IDLE_OPACITY = 0.25;

/** The map's dots and the opacity last written to each, to skip unchanged writes. */
export interface NozzleMap {
  dots: HTMLElement[];
  shownOpacity: number[];
}

/**
 * Builds one dot per nozzle inside `container`, placed from `layout` (from
 * `Game.nozzle_layout`): the car's nose points up the map and its right side to the right.
 */
export function createNozzleMap(container: HTMLElement, layout: Float32Array): NozzleMap {
  const dots: HTMLElement[] = [];
  for (let start = 0; start + FLOATS_PER_NOZZLE <= layout.length; start += FLOATS_PER_NOZZLE) {
    const forwardM = layout[start] ?? 0;
    const rightM = layout[start + 2] ?? 0;
    const dot = document.createElement('div');
    dot.className = 'nozzle-dot';
    dot.style.left = `${mapPercent(rightM, MAP_HALF_WIDTH_M)}%`;
    dot.style.top = `${100 - mapPercent(forwardM, MAP_HALF_LENGTH_M)}%`;
    dot.style.opacity = `${IDLE_OPACITY}`;
    container.appendChild(dot);
    dots.push(dot);
  }
  return { dots, shownOpacity: dots.map(() => IDLE_OPACITY) };
}

/** Brightens each dot with its nozzle's effective throttle (0..1). */
export function updateNozzleMap(map: NozzleMap, throttles: Float32Array): void {
  map.dots.forEach((dot, index) => {
    const throttle = throttles[index] ?? 0;
    // Two decimals is plenty for a glow, and it lets most frames skip the write.
    const opacity = Math.round((IDLE_OPACITY + (1 - IDLE_OPACITY) * throttle) * 100) / 100;
    if (opacity !== map.shownOpacity[index]) {
      dot.style.opacity = `${opacity}`;
      map.shownOpacity[index] = opacity;
    }
  });
}

/** Where `valueM` (from -halfM to +halfM) falls across the map, from 0 to 100. */
export function mapPercent(valueM: number, halfM: number): number {
  return ((valueM + halfM) / (2 * halfM)) * 100;
}
