// A stand-in passenger g meter for the blackout HUD (g-blackout.ts), until the sim
// exposes its own seat g as the `Game` HUD field `g_load`. It differentiates the car's
// position twice, tick by tick, and smooths the result. 1 g sitting still, 0 g in free
// fall. Rendering only: it reads poses and never feeds anything back to the sim.

/** Standard gravity, m/s². */
const GRAVITY_M_S2 = 9.81;
/** Smoothing time, so single-tick contact spikes do not flash the screen. */
const SMOOTHING_S = 0.08;
/** A jump faster than this between ticks is a teleport (respawn, reset), not motion. */
const TELEPORT_SPEED_M_S = 150;

/** What the estimate carries between ticks. */
export interface GLoadEstimator {
  /** Last position, m; null until the first tick. */
  lastPositionM: [number, number, number] | null;
  /** Last velocity, m/s; null until two ticks have passed. */
  lastVelocityM_S: [number, number, number] | null;
  /** The smoothed load, in g. */
  gLoad: number;
}

/** An estimator that reads 1 g until it has seen enough ticks. */
export function newGLoadEstimator(): GLoadEstimator {
  return { lastPositionM: null, lastVelocityM_S: null, gLoad: 1 };
}

/** Forgets the motion history, e.g. after a respawn, so the jump is not read as g. */
export function resetGLoadEstimator(estimator: GLoadEstimator): void {
  estimator.lastPositionM = null;
  estimator.lastVelocityM_S = null;
  estimator.gLoad = 1;
}

/**
 * Feeds one physics tick: the car's world position (x, y, z in m, +Y up) after a step
 * of `dtS` seconds. Returns the smoothed passenger load in g: the size of
 * (acceleration − gravity) over g, which is what a seat pushes on a body with.
 */
export function updateGLoadEstimate(
  estimator: GLoadEstimator,
  positionM: [number, number, number],
  dtS: number,
): number {
  const last = estimator.lastPositionM;
  estimator.lastPositionM = positionM;
  if (!last) {
    return estimator.gLoad;
  }
  const velocity: [number, number, number] = [
    (positionM[0] - last[0]) / dtS,
    (positionM[1] - last[1]) / dtS,
    (positionM[2] - last[2]) / dtS,
  ];
  if (Math.hypot(...velocity) > TELEPORT_SPEED_M_S) {
    resetGLoadEstimator(estimator);
    return estimator.gLoad;
  }
  const lastVelocity = estimator.lastVelocityM_S;
  estimator.lastVelocityM_S = velocity;
  if (!lastVelocity) {
    return estimator.gLoad;
  }
  // The seat pushes with (a − g); gravity points down (−Y), so its Y part gains +g.
  const pushX = (velocity[0] - lastVelocity[0]) / dtS;
  const pushY = (velocity[1] - lastVelocity[1]) / dtS + GRAVITY_M_S2;
  const pushZ = (velocity[2] - lastVelocity[2]) / dtS;
  const rawG = Math.hypot(pushX, pushY, pushZ) / GRAVITY_M_S2;
  const blend = 1 - Math.exp(-dtS / SMOOTHING_S);
  estimator.gLoad += (rawG - estimator.gLoad) * blend;
  return estimator.gLoad;
}
