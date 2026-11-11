// Which bottom-centre tiles show (SPEC §11.5): the nozzle map and the g-meter appear while any
// nozzle fires (the automatic saves), while a thruster key (1 to 0) is held, or right after a
// hard landing, then fade fast. Pure numbers and elements; no physics.

/** A landing counts as hard from this passenger load, in g (the meter's amber mark). */
const HARD_LANDING_G = 3;
/** A nozzle counts as firing above this effective throttle (0..1); matches the flames' cut-off. */
export const FIRING_THROTTLE = 0.02;
/** How long each tile stays after its reason ends, in ms. */
const G_HOLD_MS = 1200;
const NOZZLE_HOLD_MS = 800;
/** The last stretch of the hold in which the tile fades out, in ms. */
const FADE_MS = 300;

/** When each tile may hide again, in `performance.now()` milliseconds. */
export interface ZoneHolds {
  gUntilMs: number;
  nozzleUntilMs: number;
}

/** Why the tiles might show this frame. */
export interface ZoneReasons {
  /** A manual thruster key (1 to 0) is held. */
  thrusterKeyHeld: boolean;
  /** True when any nozzle's throttle is above `FIRING_THROTTLE`. */
  anyNozzleFiring: boolean;
  /** Passenger load in g. */
  gLoad: number;
  nowMs: number;
}

/** Nothing held yet. */
export function newZoneHolds(): ZoneHolds {
  return { gUntilMs: 0, nozzleUntilMs: 0 };
}

/** True when any of the per-nozzle throttles (0..1) is above `FIRING_THROTTLE`. */
export function isAnyNozzleFiring(throttles: ArrayLike<number>): boolean {
  for (let index = 0; index < throttles.length; index += 1) {
    if ((throttles[index] ?? 0) > FIRING_THROTTLE) {
      return true;
    }
  }
  return false;
}

/** True when a reason to show a tile is live this frame. */
function showReason(reasons: ZoneReasons): boolean {
  return reasons.thrusterKeyHeld || reasons.anyNozzleFiring || reasons.gLoad >= HARD_LANDING_G;
}

/** True when the g-meter should show: a nozzle fires, a key is held, or a hard landing is fresh. */
export function gMeterWanted(holds: ZoneHolds, reasons: ZoneReasons): boolean {
  if (showReason(reasons)) {
    holds.gUntilMs = reasons.nowMs + G_HOLD_MS;
  }
  return reasons.nowMs < holds.gUntilMs;
}

/** True when the nozzle map should show: the same reasons as the g-meter. */
export function nozzleMapWanted(holds: ZoneHolds, reasons: ZoneReasons): boolean {
  if (showReason(reasons)) {
    holds.nozzleUntilMs = reasons.nowMs + NOZZLE_HOLD_MS;
  }
  return reasons.nowMs < holds.nozzleUntilMs;
}

/** Opacity 0..1: full until the last `FADE_MS` of the hold, then down to 0. */
export function zoneOpacity(untilMs: number, nowMs: number): number {
  return Math.max(0, Math.min(1, (untilMs - nowMs) / FADE_MS));
}

/** Shows or hides the two tiles for this frame, fading them at the end. */
export function updateZones(
  tiles: { gTile: HTMLElement; nozzleTile: HTMLElement },
  holds: ZoneHolds,
  reasons: ZoneReasons,
): void {
  const nowMs = reasons.nowMs;
  tiles.gTile.hidden = !gMeterWanted(holds, reasons);
  tiles.nozzleTile.hidden = !nozzleMapWanted(holds, reasons);
  tiles.gTile.style.opacity = String(zoneOpacity(holds.gUntilMs, nowMs));
  tiles.nozzleTile.style.opacity = String(zoneOpacity(holds.nozzleUntilMs, nowMs));
}
