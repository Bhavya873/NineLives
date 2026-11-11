// The joystick's maths, with no page code: clamp the knob to the ring and turn its offset
// into throttle and steer. Kept apart from touch-stick.ts so it runs in plain Node.

/** Inside this fraction of the ring's travel the stick reads as centred. */
export const STICK_DEAD_ZONE = 0.12;

/** Throttle and steer for the game: -1 to 1, steer positive to the LEFT (SPEC §4). */
export interface StickAxes {
  throttle: number;
  steer: number;
}

/** The knob offset (px from the ring's centre) kept inside a circle of radius `maxPx`. */
export function clampToRing(
  dxPx: number,
  dyPx: number,
  maxPx: number,
): { dxPx: number; dyPx: number } {
  const lengthPx = Math.hypot(dxPx, dyPx);
  if (lengthPx <= maxPx || lengthPx === 0) {
    return { dxPx, dyPx };
  }
  const scale = maxPx / lengthPx;
  return { dxPx: dxPx * scale, dyPx: dyPx * scale };
}

/**
 * Turns the knob offset into axes. Screen y grows downward, so pushing up is gas and pulling
 * down is brake or reverse; pushing right is a right turn, which is negative steer. The dead
 * zone is cut out and the rest stretched back to full range, so there is no jump at its edge.
 */
export function stickAxes(dxPx: number, dyPx: number, maxPx: number): StickAxes {
  const clamped = clampToRing(dxPx, dyPx, maxPx);
  const fraction = Math.hypot(clamped.dxPx, clamped.dyPx) / maxPx;
  if (fraction <= STICK_DEAD_ZONE) {
    return { throttle: 0, steer: 0 };
  }
  const strength = (fraction - STICK_DEAD_ZONE) / (1 - STICK_DEAD_ZONE);
  const unitX = clamped.dxPx / (fraction * maxPx);
  const unitY = clamped.dyPx / (fraction * maxPx);
  // "+ 0" turns -0 into 0 for a stick pushed straight along one axis.
  return { throttle: -unitY * strength + 0, steer: -unitX * strength + 0 };
}
