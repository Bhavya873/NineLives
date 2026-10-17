// The HUD's top-down nozzle map (SPEC §11.5): a car outline with one dot per nozzle that
// glows with its effective throttle, so the player can see which thrusters the AI fires.
// Plain DOM elements; it reads numbers from the game and never touches physics.
import type { NozzlePlace } from '../nozzle-layout.ts';

/** Space around the car on the map, in metres, so dots at the very edge still show. */
const MARGIN_M = 0.4;
/** A dot never fades out completely, so idle nozzles still show where they are. */
const IDLE_OPACITY = 0.25;
/** Map scale; the same on both axes so the car is not stretched. */
const PIXELS_PER_M = 26;
/** Glow steps; coarse enough that most frames write nothing. */
const OPACITY_STEPS = 20;

/**
 * Builds the car outline and one dot per nozzle inside `container`. The car's nose points
 * up the map and its right side to the right; the outline spans the nozzles' extent, which
 * is the car's body, so both use the same metres-to-percent scale. Returns the dots.
 */
export function createNozzleMap(container: HTMLElement, nozzles: NozzlePlace[]): HTMLElement[] {
  const halfLengthM = largestAbs(nozzles, 0) + MARGIN_M;
  const halfWidthM = largestAbs(nozzles, 2) + MARGIN_M;
  container.style.width = `${Math.round(2 * halfWidthM * PIXELS_PER_M)}px`;
  container.style.height = `${Math.round(2 * halfLengthM * PIXELS_PER_M)}px`;
  const outline = document.createElement('div');
  outline.className = 'car-outline';
  const insetLength = mapPercent(-(halfLengthM - MARGIN_M), halfLengthM);
  const insetWidth = mapPercent(-(halfWidthM - MARGIN_M), halfWidthM);
  outline.style.inset = `${insetLength}% ${insetWidth}%`;
  container.appendChild(outline);

  const dots: HTMLElement[] = [];
  for (const place of nozzles) {
    const [forwardM, , rightM] = place.positionM;
    const dot = document.createElement('div');
    dot.className = 'nozzle-dot';
    dot.style.left = `${mapPercent(rightM, halfWidthM)}%`;
    dot.style.top = `${100 - mapPercent(forwardM, halfLengthM)}%`;
    dot.style.opacity = `${IDLE_OPACITY}`;
    container.appendChild(dot);
    dots.push(dot);
  }
  return dots;
}

/** Brightens each dot with its nozzle's effective throttle (0..1). */
export function updateNozzleMap(dots: HTMLElement[], throttles: Float32Array): void {
  for (let index = 0; index < dots.length; index += 1) {
    const dot = dots[index];
    const throttle = throttles[index] ?? 0;
    const glow = IDLE_OPACITY + (1 - IDLE_OPACITY) * throttle;
    const opacity = `${Math.round(glow * OPACITY_STEPS) / OPACITY_STEPS}`;
    if (dot && dot.style.opacity !== opacity) {
      dot.style.opacity = opacity;
    }
  }
}

/** Where `valueM` (from -halfM to +halfM) falls across the map, from 0 to 100. */
export function mapPercent(valueM: number, halfM: number): number {
  return ((valueM + halfM) / (2 * halfM)) * 100;
}

/** The largest distance from the centre along one body axis (0 forward, 2 right). */
function largestAbs(nozzles: NozzlePlace[], axis: 0 | 2): number {
  let largest = 0;
  for (const place of nozzles) {
    largest = Math.max(largest, Math.abs(place.positionM[axis]));
  }
  return largest;
}
