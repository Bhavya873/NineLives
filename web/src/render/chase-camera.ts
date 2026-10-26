// A smoothed chase camera that follows the car (SPEC §11.4).
// It stays level with the world (never rolls with the car), so the player can see the
// car flip. It sits behind the car's heading on the ground plane, eases into place,
// and pulls back a little while the car is in the air.
import { type PerspectiveCamera, Vector3 } from 'three';

import type { BlendedPose } from './pose-blend.ts';

const DISTANCE_BEHIND_M = 7.5;
/** Extra distance while airborne, so jumps and flips fit in the frame. */
const AIRBORNE_EXTRA_DISTANCE_M = 3;
/** How quickly the pull-back eases in and out, per second. */
const PULL_BACK_RATE_PER_S = 2;
/**
 * Only trust the car's nose as a heading while it points mostly sideways (pitched less
 * than about 60°). In the middle of a flip the old heading is kept, so the camera does
 * not swing over the top of the car.
 */
const MIN_FLAT_FORWARD_LENGTH_SQ = 0.25;
const HEIGHT_ABOVE_M = 2.8;
const LOOK_AT_HEIGHT_M = 0.8;
/** How quickly the camera catches up, per second. Higher is stiffer. */
const FOLLOW_RATE_PER_S = 4;
/** Field of view at rest, in degrees (the camera is built with it). */
const BASE_FOV_DEG = 60;
/** Extra field of view at `FULL_FOV_SPEED_M_S` and above, in degrees. */
const SPEED_FOV_EXTRA_DEG = 8;
const FULL_FOV_SPEED_M_S = 40;
/** How quickly the field of view follows the speed, per second. */
const FOV_RATE_PER_S = 2;
/** Smaller field-of-view changes than this are skipped, in degrees. */
const FOV_STEP_DEG = 0.01;

/** The direction the camera trails behind; kept when the car points straight up or down. */
const heading = new Vector3(1, 0, 0);
/** Current pull-back, eased between 0 and AIRBORNE_EXTRA_DISTANCE_M. */
let pullBackM = 0;
const carForward = new Vector3();
const wantedPosition = new Vector3();
const lookAtPoint = new Vector3();

/** Puts the camera straight behind the car without easing, e.g. at start-up. */
export function snapCamera(camera: PerspectiveCamera, car: BlendedPose): void {
  updateHeading(car);
  placeBehind(car, wantedPosition);
  camera.position.copy(wantedPosition);
  lookAtCar(camera, car);
}

/**
 * Eases the camera toward its place behind the car. `frameS` is the frame time;
 * `airborne` is true when no wheel touches the ground.
 */
export function followCar(
  camera: PerspectiveCamera,
  car: BlendedPose,
  frameS: number,
  airborne: boolean,
): void {
  const wantedPullBackM = airborne ? AIRBORNE_EXTRA_DISTANCE_M : 0;
  pullBackM += (wantedPullBackM - pullBackM) * (1 - Math.exp(-PULL_BACK_RATE_PER_S * frameS));
  updateHeading(car);
  placeBehind(car, wantedPosition);
  // Exponential easing: the same feel at any frame rate.
  const catchUp = 1 - Math.exp(-FOLLOW_RATE_PER_S * frameS);
  camera.position.lerp(wantedPosition, catchUp);
  lookAtCar(camera, car);
}

/** Widens the view a little with speed, eased, so going fast feels fast. */
export function widenWithSpeed(camera: PerspectiveCamera, speedMS: number, frameS: number): void {
  const share = Math.min(Math.abs(speedMS) / FULL_FOV_SPEED_M_S, 1);
  const wantedDeg = BASE_FOV_DEG + SPEED_FOV_EXTRA_DEG * share;
  const nextDeg = camera.fov + (wantedDeg - camera.fov) * (1 - Math.exp(-FOV_RATE_PER_S * frameS));
  if (Math.abs(nextDeg - camera.fov) > FOV_STEP_DEG) {
    camera.fov = nextDeg;
    camera.updateProjectionMatrix();
  }
}

/** Uses the car's nose direction flattened onto the ground as the heading. */
function updateHeading(car: BlendedPose): void {
  carForward.set(1, 0, 0).applyQuaternion(car.rotation);
  carForward.y = 0;
  if (carForward.lengthSq() > MIN_FLAT_FORWARD_LENGTH_SQ) {
    heading.copy(carForward.normalize());
  }
}

function placeBehind(car: BlendedPose, out: Vector3): void {
  out.copy(car.position).addScaledVector(heading, -(DISTANCE_BEHIND_M + pullBackM));
  out.y = car.position.y + HEIGHT_ABOVE_M;
}

function lookAtCar(camera: PerspectiveCamera, car: BlendedPose): void {
  lookAtPoint.copy(car.position);
  lookAtPoint.y += LOOK_AT_HEIGHT_M;
  camera.lookAt(lookAtPoint);
}
