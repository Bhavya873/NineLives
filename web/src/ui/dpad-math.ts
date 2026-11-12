// The arrow pad's maths, with no page code: turn the thumb's offset from the cross centre into
// throttle and steer. Kept apart from touch-arrows.ts so it runs in plain Node.

/** Inside this fraction of the cross's half-size the thumb counts as centred. */
export const DPAD_DEAD_ZONE = 0.2;

/** tan(22.5 degrees): the edge between a straight sector and a diagonal one. */
const TAN_22_5_DEGREES = 0.4142;

/** Throttle and steer for the game: each -1, 0 or 1, steer positive to the LEFT (SPEC §4). */
export interface DpadAxes {
  throttle: number;
  steer: number;
}

/**
 * Reads the thumb offset (px from the cross centre, screen y down) as one of eight directions
 * or none. Up is gas, down is brake or reverse, left is positive steer. A thumb dragged outside
 * the cross still counts by its direction, so sliding off an arrow never drops the input.
 */
export function dpadAxes(dxPx: number, dyPx: number, halfSizePx: number): DpadAxes {
  const distancePx = Math.hypot(dxPx, dyPx);
  if (distancePx <= DPAD_DEAD_ZONE * halfSizePx) {
    return { throttle: 0, steer: 0 };
  }
  const sideways = Math.abs(dxPx) > Math.abs(dyPx) * TAN_22_5_DEGREES;
  const upDown = Math.abs(dyPx) > Math.abs(dxPx) * TAN_22_5_DEGREES;
  const throttle = upDown ? (dyPx < 0 ? 1 : -1) : 0;
  const steer = sideways ? (dxPx < 0 ? 1 : -1) : 0;
  return { throttle, steer };
}
