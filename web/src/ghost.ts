// The ghost run: the best run's chassis path, kept in localStorage and replayed as a
// translucent car in the next run. Pure logic: no three.js, no DOM, so it runs in plain
// Node. Every storage access is in try/catch and an unknown stored shape is dropped.

export const GHOST_KEY = 'ninelives.ghost.v1';
export const GHOST_VERSION = 1;
/** Samples per second of run time. */
export const GHOST_HZ = 10;
/** A run longer than this stops recording (keeps the stored run small), in s. */
export const GHOST_MAX_S = 300;
/** Floats per sample: x, y, z, qx, qy, qz, qw. */
export const GHOST_FLOATS = 7;

/** The slice of `Storage` the ghost needs. */
export interface GhostStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

/** One stored run. */
export interface GhostRun {
  version: number;
  hz: number;
  /** Saves in the run; the better run has more, then the longer one. */
  saves: number;
  durationS: number;
  /** `GHOST_FLOATS` numbers per sample, rounded to a millimetre / 1e-3. */
  poses: number[];
}

/** A run being recorded. */
export interface GhostRecorder {
  poses: number[];
  /** Run time of the next sample, in s. */
  nextS: number;
  lastS: number;
}

export function createGhostRecorder(): GhostRecorder {
  return { poses: [], nextS: 0, lastS: 0 };
}

/** Starts the recording over. */
export function resetGhostRecorder(recorder: GhostRecorder): void {
  recorder.poses = [];
  recorder.nextS = 0;
  recorder.lastS = 0;
}

/**
 * Records `pose` (7 floats) for every 1/10 s slot that has come due by `runTimeS`. Slots are
 * on a fixed grid, so the replay's time base matches the run's; a late frame fills the
 * slots it missed with the same pose (at most a second's worth).
 */
export function recordGhostSample(
  recorder: GhostRecorder,
  runTimeS: number,
  pose: ArrayLike<number>,
): void {
  if (!(runTimeS >= 0) || pose.length < GHOST_FLOATS) {
    return;
  }
  const period = 1 / GHOST_HZ;
  if (runTimeS - recorder.nextS > 1) {
    // The clock jumped (a restart without a reset): do not fill the gap.
    recorder.nextS = runTimeS;
  }
  while (recorder.nextS <= runTimeS && recorder.nextS <= GHOST_MAX_S) {
    for (let index = 0; index < GHOST_FLOATS; index += 1) {
      recorder.poses.push(Math.round((pose[index] ?? 0) * 1000) / 1000);
    }
    recorder.lastS = recorder.nextS;
    recorder.nextS += period;
  }
}

/** The recording as a run, or null when it is too short to replay. */
export function finishGhostRun(recorder: GhostRecorder, saves: number): GhostRun | null {
  if (recorder.poses.length < 2 * GHOST_FLOATS) {
    return null;
  }
  return {
    version: GHOST_VERSION,
    hz: GHOST_HZ,
    saves,
    durationS: recorder.lastS,
    poses: recorder.poses.slice(),
  };
}

/** True if `candidate` should replace `best`: more saves, or as many and longer. */
export function isBetterRun(candidate: GhostRun, best: GhostRun | null): boolean {
  if (!best) {
    return true;
  }
  if (candidate.saves !== best.saves) {
    return candidate.saves > best.saves;
  }
  return candidate.durationS > best.durationS;
}

function isGhostRun(value: unknown): value is GhostRun {
  if (typeof value !== 'object' || value === null) {
    return false;
  }
  const run = value as Partial<GhostRun>;
  return (
    run.version === GHOST_VERSION &&
    typeof run.hz === 'number' &&
    run.hz > 0 &&
    typeof run.saves === 'number' &&
    typeof run.durationS === 'number' &&
    Array.isArray(run.poses) &&
    run.poses.length >= 2 * GHOST_FLOATS &&
    run.poses.length % GHOST_FLOATS === 0 &&
    run.poses.every((n) => typeof n === 'number' && Number.isFinite(n))
  );
}

/** The stored best run, or null (nothing stored, blocked storage or an unknown shape). */
export function loadGhost(storage: GhostStorage | null): GhostRun | null {
  try {
    const raw = storage?.getItem(GHOST_KEY);
    if (!raw) {
      return null;
    }
    const parsed: unknown = JSON.parse(raw);
    return isGhostRun(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

/** Stores `run` if it beats the stored best. Returns true if it was stored. */
export function saveGhostIfBetter(storage: GhostStorage | null, run: GhostRun): boolean {
  if (!isBetterRun(run, loadGhost(storage))) {
    return false;
  }
  try {
    storage?.setItem(GHOST_KEY, JSON.stringify(run));
    return storage !== null;
  } catch {
    // Storage may be blocked or full; the ghost then lasts for this visit only.
    return false;
  }
}

/**
 * Writes the ghost's pose at `runTimeS` into `out` (7 floats): positions blend linearly,
 * rotations by normalised lerp. Returns false once the run has ended or before it starts.
 */
export function sampleGhost(run: GhostRun, runTimeS: number, out: number[]): boolean {
  const count = run.poses.length / GHOST_FLOATS;
  const position = runTimeS * run.hz;
  if (!(position >= 0) || position > count - 1) {
    return false;
  }
  const first = Math.floor(position);
  const next = Math.min(first + 1, count - 1);
  const blend = position - first;
  const a = first * GHOST_FLOATS;
  const b = next * GHOST_FLOATS;
  for (let index = 0; index < 3; index += 1) {
    const from = run.poses[a + index] ?? 0;
    const to = run.poses[b + index] ?? 0;
    out[index] = from + (to - from) * blend;
  }
  // Take the short way round: flip the second quaternion if the two point apart.
  let dot = 0;
  for (let index = 3; index < GHOST_FLOATS; index += 1) {
    dot += (run.poses[a + index] ?? 0) * (run.poses[b + index] ?? 0);
  }
  const sign = dot < 0 ? -1 : 1;
  let length = 0;
  for (let index = 3; index < GHOST_FLOATS; index += 1) {
    const from = run.poses[a + index] ?? 0;
    const to = (run.poses[b + index] ?? 0) * sign;
    const value = from + (to - from) * blend;
    out[index] = value;
    length += value * value;
  }
  const norm = Math.sqrt(length) || 1;
  for (let index = 3; index < GHOST_FLOATS; index += 1) {
    out[index] = (out[index] ?? 0) / norm;
  }
  return true;
}
