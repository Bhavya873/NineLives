// Which bottom-centre tiles show (SPEC §11.5): the g-meter while the car is in the air or
// pulling real g, the nozzle map while a nozzle fires. Each stays a moment after its
// reason ends, so it does not flicker. Pure numbers and elements; no physics.

/** The g-meter shows from this passenger load, in g (1 g is sitting still). */
const G_SHOW_FROM = 1.6;
/** A nozzle counts as firing from this throttle, 0..1. */
const NOZZLE_FIRING_FROM = 0.05;
/** How long each tile stays after its reason ends, in ms. */
const G_HOLD_MS = 2500;
const NOZZLE_HOLD_MS = 1500;

/** When each tile may hide again, in `performance.now()` milliseconds. */
export interface ZoneHolds {
  gUntilMs: number;
  nozzleUntilMs: number;
}

/** Nothing held yet. */
export function newZoneHolds(): ZoneHolds {
  return { gUntilMs: 0, nozzleUntilMs: 0 };
}

/** True when the g-meter should show: airborne or loaded now, or within its hold time. */
export function gMeterWanted(
  holds: ZoneHolds,
  airborne: boolean,
  gLoad: number,
  nowMs: number,
): boolean {
  if (airborne || gLoad >= G_SHOW_FROM) {
    holds.gUntilMs = nowMs + G_HOLD_MS;
  }
  return nowMs < holds.gUntilMs;
}

/** True when the nozzle map should show: a nozzle fires now or did a moment ago. */
export function nozzleMapWanted(holds: ZoneHolds, throttles: Float32Array, nowMs: number): boolean {
  if (throttles.some((throttle) => throttle >= NOZZLE_FIRING_FROM)) {
    holds.nozzleUntilMs = nowMs + NOZZLE_HOLD_MS;
  }
  return nowMs < holds.nozzleUntilMs;
}

/** Shows or hides the two tiles for this frame. */
export function updateZones(
  tiles: { gTile: HTMLElement; nozzleTile: HTMLElement },
  holds: ZoneHolds,
  frame: { airborne: boolean; gLoad: number; throttles: Float32Array; nowMs: number },
): void {
  tiles.gTile.hidden = !gMeterWanted(holds, frame.airborne, frame.gLoad, frame.nowMs);
  tiles.nozzleTile.hidden = !nozzleMapWanted(holds, frame.throttles, frame.nowMs);
}
