// The ghost car: the car body drawn see-through where the best run's path says it was.
// The body only, no wheels or flames, so it reads as a marker rather than a second car.
import { type Group, type Material, Mesh, type Scene } from 'three';

import { buildCarBody } from './car-mesh.ts';

/** How see-through the ghost is. */
const GHOST_OPACITY = 0.35;

/** The ghost's mesh group. */
export interface GhostCar {
  body: Group;
}

/** Builds the ghost, hidden, and adds it to `scene`. */
export function createGhostCar(scene: Scene): GhostCar {
  const body = buildCarBody();
  body.traverse((object) => {
    if (object instanceof Mesh) {
      // Own copies: the real car shares these materials.
      const material = (object.material as Material).clone();
      material.transparent = true;
      material.opacity = GHOST_OPACITY;
      material.depthWrite = false;
      object.material = material;
      object.castShadow = false;
      object.receiveShadow = false;
    }
  });
  body.visible = false;
  scene.add(body);
  return { body };
}

/** Puts the ghost at `pose` (x, y, z, qx, qy, qz, qw) and shows it. */
export function placeGhost(ghost: GhostCar, pose: ArrayLike<number>): void {
  ghost.body.position.set(pose[0] ?? 0, pose[1] ?? 0, pose[2] ?? 0);
  ghost.body.quaternion.set(pose[3] ?? 0, pose[4] ?? 0, pose[5] ?? 0, pose[6] ?? 1);
  ghost.body.visible = true;
}

/** Hides the ghost. */
export function hideGhost(ghost: GhostCar): void {
  ghost.body.visible = false;
}
