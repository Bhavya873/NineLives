// A smoothed chase camera that follows the car (SPEC §11.4).
// It stays level with the world (never rolls with the car), so the player can see the
// car flip. It sits behind the car's heading on the ground plane and eases into place.
import { type PerspectiveCamera, Vector3 } from 'three';

import type { BlendedPose } from './pose-blend';

const DISTANCE_BEHIND_M = 7.5;
const HEIGHT_ABOVE_M = 2.8;
const LOOK_AT_HEIGHT_M = 0.8;
/** How quickly the camera catches up, per second. Higher is stiffer. */
const FOLLOW_RATE_PER_S = 4;

/** The direction the camera trails behind; kept when the car points straight up or down. */
const heading = new Vector3(1, 0, 0);
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

/** Eases the camera toward its place behind the car. `frameS` is the frame time. */
export function followCar(camera: PerspectiveCamera, car: BlendedPose, frameS: number): void {
  updateHeading(car);
  placeBehind(car, wantedPosition);
  // Exponential easing: the same feel at any frame rate.
  const catchUp = 1 - Math.exp(-FOLLOW_RATE_PER_S * frameS);
  camera.position.lerp(wantedPosition, catchUp);
  lookAtCar(camera, car);
}

/** Uses the car's nose direction flattened onto the ground as the heading. */
function updateHeading(car: BlendedPose): void {
  carForward.set(1, 0, 0).applyQuaternion(car.rotation);
  carForward.y = 0;
  if (carForward.lengthSq() > 0.01) {
    heading.copy(carForward.normalize());
  }
}

function placeBehind(car: BlendedPose, out: Vector3): void {
  out.copy(car.position).addScaledVector(heading, -DISTANCE_BEHIND_M);
  out.y = car.position.y + HEIGHT_ABOVE_M;
}

function lookAtCar(camera: PerspectiveCamera, car: BlendedPose): void {
  lookAtPoint.copy(car.position);
  lookAtPoint.y += LOOK_AT_HEIGHT_M;
  camera.lookAt(lookAtPoint);
}
