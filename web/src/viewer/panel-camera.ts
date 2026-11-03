// The camera of one stage panel (SPEC §11.6): fixed and high for the drop arenas, trailing the
// cars along the long courses. Each panel has its own camera, because the panels are drawn into
// different rectangles of one canvas.
import type { PerspectiveCamera } from 'three';

import type { ArenaKind } from './arena-choice.ts';

/** Camera spots, in m: high and far back for the flat arenas' 64-car grid (about 100 m
 * across), closer for the obstacle arena, where every car drops near the centre. The
 * courses start at the origin and run along +X; their cameras follow the cars from there. */
const CAMERA_SPOT_M: Record<ArenaKind, [number, number, number]> = {
  flat: [0, 70, 105],
  side: [0, 70, 105],
  obstacles: [0, 38, 54],
  driving: [-18, 9, 22],
  flight: [-18, 9, 22],
};
/** The camera trails the cars' centre by this much, in m (behind, above, to the side). */
const FOLLOW_OFFSET_M: [number, number, number] = [-18, 9, 22];
/** It looks this far ahead of the cars' centre, in m. */
const FOLLOW_LOOK_AHEAD_M = 8;
/** The fixed cameras look at this point just above the arena centre, in m. */
const CAMERA_TARGET_M: [number, number, number] = [0, 2, 0];

/** Puts `camera` at the fixed spot of `arena` and aims it at the arena's centre. */
export function aimFixedCamera(camera: PerspectiveCamera, arena: ArenaKind): void {
  camera.position.set(...CAMERA_SPOT_M[arena]);
  camera.lookAt(...CAMERA_TARGET_M);
}

/** Aims `camera` at the cars' centre `(x, z)`, in m, for the arenas with a long course. */
export function followCars(camera: PerspectiveCamera, x: number, z: number): void {
  const [dx, dy, dz] = FOLLOW_OFFSET_M;
  camera.position.set(x + dx, dy, z + dz);
  camera.lookAt(x + FOLLOW_LOOK_AHEAD_M, 1, z);
}

/** The vertical field of view, in degrees, at the reference aspect. */
const BASE_FOV_DEG = 60;
/** The aspect (width over height) the base field of view is meant for. */
const REFERENCE_ASPECT = 1.6;
/** A tall, narrow panel would cut the arena off at the sides, so its field of view widens to
 * keep the same horizontal view, up to this much. */
const MAX_FOV_DEG = 100;

/** The vertical field of view that keeps the reference's horizontal view at `aspect`. */
export function fovForAspect(aspect: number): number {
  if (aspect >= REFERENCE_ASPECT) {
    return BASE_FOV_DEG;
  }
  const half = Math.tan((BASE_FOV_DEG * Math.PI) / 360) * (REFERENCE_ASPECT / aspect);
  return Math.min(MAX_FOV_DEG, (2 * Math.atan(half) * 180) / Math.PI);
}
