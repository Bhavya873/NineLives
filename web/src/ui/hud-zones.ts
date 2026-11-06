// Which bottom-centre tiles show (SPEC §11.5): the g-meter and the nozzle map appear only
// while a thruster key (1 to 0) is held or right after a hard landing, then fade fast.
// Pure numbers and elements; no physics.

/** A landing counts as hard from this passenger load, in g (the meter's amber mark). */
const HARD_LANDING_G = 3;
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

/** Nothing held yet. */
export function newZoneHolds(): ZoneHolds {
  return { gUntilMs: 0, nozzleUntilMs: 0 };
}

/** True when the g-meter should show: a thruster key is held, or a hard landing is fresh. */
export function gMeterWanted(
  holds: ZoneHolds,
  thrusterKeyHeld: boolean,
  gLoad: number,
  nowMs: number,
): boolean {
  if (thrusterKeyHeld || gLoad >= HARD_LANDING_G) {
    holds.gUntilMs = nowMs + G_HOLD_MS;
  }
  return nowMs < holds.gUntilMs;
}

/** True when the nozzle map should show: a thruster key is held, or a hard landing is fresh. */
export function nozzleMapWanted(
  holds: ZoneHolds,
  thrusterKeyHeld: boolean,
  gLoad: number,
  nowMs: number,
): boolean {
  if (thrusterKeyHeld || gLoad >= HARD_LANDING_G) {
    holds.nozzleUntilMs = nowMs + NOZZLE_HOLD_MS;
  }
  return nowMs < holds.nozzleUntilMs;
}

/** Opacity 0..1: full until the last `FADE_MS` of the hold, then down to 0. */
export function zoneOpacity(untilMs: number, nowMs: number): number {
  return Math.max(0, Math.min(1, (untilMs - nowMs) / FADE_MS));
}

/** Shows or hides the two tiles for this frame, fading them at the end. */
export function updateZones(
  tiles: { gTile: HTMLElement; nozzleTile: HTMLElement },
  holds: ZoneHolds,
  frame: { thrusterKeyHeld: boolean; gLoad: number; nowMs: number },
): void {
  const { thrusterKeyHeld, gLoad, nowMs } = frame;
  tiles.gTile.hidden = !gMeterWanted(holds, thrusterKeyHeld, gLoad, nowMs);
  tiles.nozzleTile.hidden = !nozzleMapWanted(holds, thrusterKeyHeld, gLoad, nowMs);
  tiles.gTile.style.opacity = String(zoneOpacity(holds.gUntilMs, nowMs));
  tiles.nozzleTile.style.opacity = String(zoneOpacity(holds.nozzleUntilMs, nowMs));
}
