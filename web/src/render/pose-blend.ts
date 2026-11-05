// Blends two physics poses so the car moves smoothly between 120 Hz ticks (SPEC §11.2).
// A pose is 7 floats: position x, y, z, then quaternion x, y, z, w.
import { Quaternion, Vector3 } from 'three';

/** Floats in one pose. layout-check.ts checks this against Game.floats_per_pose() at start-up. */
export const FLOATS_PER_POSE = 7;

/** One pose as three.js objects. */
export interface BlendedPose {
  position: Vector3;
  rotation: Quaternion;
}

/** Reusable objects so blending allocates nothing per frame. */
const fromRotation = new Quaternion();
const toRotation = new Quaternion();

/**
 * Writes the pose `blend` of the way from `previous` to `latest` into `out`.
 * `index` picks which pose in a packed array (0 for the chassis, 0..3 for wheels).
 */
export function blendPose(
  previous: Float32Array,
  latest: Float32Array,
  index: number,
  blend: number,
  out: BlendedPose,
): void {
  const start = index * FLOATS_PER_POSE;
  out.position.set(
    lerp(previous[start] ?? 0, latest[start] ?? 0, blend),
    lerp(previous[start + 1] ?? 0, latest[start + 1] ?? 0, blend),
    lerp(previous[start + 2] ?? 0, latest[start + 2] ?? 0, blend),
  );
  readQuaternion(previous, start + 3, fromRotation);
  readQuaternion(latest, start + 3, toRotation);
  out.rotation.slerpQuaternions(fromRotation, toRotation, blend);
}

/** Makes an empty pose to blend into. */
export function newBlendedPose(): BlendedPose {
  return { position: new Vector3(), rotation: new Quaternion() };
}

function lerp(from: number, to: number, blend: number): number {
  return from + (to - from) * blend;
}

function readQuaternion(values: Float32Array, start: number, out: Quaternion): void {
  out.set(
    values[start] ?? 0,
    values[start + 1] ?? 0,
    values[start + 2] ?? 0,
    values[start + 3] ?? 1,
  );
}
