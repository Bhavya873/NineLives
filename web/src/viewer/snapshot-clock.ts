// Smooth motion from snapshots (plan: all-stages viewer). A stage worker posts the cars' poses
// every few milliseconds of real time; each snapshot carries the simulated time it belongs to.
// The page draws the picture at a "display time" that runs at real speed between the previous
// and the latest snapshot, so motion stays even however the worker's posts line up with the
// page's frames. If the worker falls behind the display time waits at the latest snapshot (the
// picture slows down honestly instead of jumping). Pure.

/** The times, in simulated seconds, the picture is drawn between. */
export interface SnapshotClock {
  previousS: number;
  latestS: number;
  displayS: number;
}

/** A clock that has seen no snapshot. */
export function newSnapshotClock(): SnapshotClock {
  return { previousS: 0, latestS: 0, displayS: 0 };
}

/** A snapshot at `simTimeS` has arrived. `restart` starts a clock over (a new round). */
export function onSnapshot(clock: SnapshotClock, simTimeS: number, restart: boolean): void {
  if (restart) {
    clock.previousS = simTimeS;
    clock.latestS = simTimeS;
    clock.displayS = simTimeS;
    return;
  }
  clock.previousS = clock.latestS;
  clock.latestS = simTimeS;
  clock.displayS = Math.max(clock.displayS, clock.previousS);
}

/** A frame of `realDtS` seconds has passed: moves the display time on and returns how far
 * (0..1) the picture is from the previous snapshot to the latest. */
export function advanceClock(clock: SnapshotClock, realDtS: number): number {
  clock.displayS = Math.min(clock.displayS + realDtS, clock.latestS);
  const span = clock.latestS - clock.previousS;
  if (span <= 0) {
    return 1;
  }
  return Math.min(1, Math.max(0, (clock.displayS - clock.previousS) / span));
}
