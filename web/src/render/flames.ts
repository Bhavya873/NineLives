// Thruster nozzles and their cold-gas flames (SPEC §11.4): a small dark cone at each
// nozzle position, and an additive blue-white cone whose length and opacity follow the
// nozzle's effective throttle. Cold gas, so never fire colours. They hang off the car
// body group, so they move and hide with the car. Rendering only: the sim decides thrust.
import {
  AdditiveBlending,
  ConeGeometry,
  CylinderGeometry,
  type Group,
  Mesh,
  MeshBasicMaterial,
  MeshStandardMaterial,
  Vector3,
} from 'three';

import { FLOATS_PER_NOZZLE } from '../sim-layout.ts';

/** Flame length at full throttle. */
const FLAME_MAX_LENGTH_M = 1.4;
const FLAME_RADIUS_M = 0.12;
/** Below this throttle the flame is hidden rather than drawn as a sliver. */
const FLAME_MIN_THROTTLE = 0.02;
const FLAME_MAX_OPACITY = 0.85;
const FLAME_COLOUR = 0xd6ecff;
const NOZZLE_COLOUR = 0x30343b;

/** The flame meshes, one per nozzle in nozzle order. */
export interface Flames {
  flames: Mesh<ConeGeometry, MeshBasicMaterial>[];
}

/** The way three.js cones point before they are turned toward the exhaust. */
const CONE_AXIS = new Vector3(0, 1, 0);

/**
 * Adds a nozzle and a (hidden) flame to `carBody` for each nozzle in `layout`
 * (from `Game.nozzle_layout`, in the car's body frame).
 */
export function createFlames(carBody: Group, layout: Float32Array): Flames {
  const nozzleGeometry = new CylinderGeometry(0.05, 0.09, 0.12, 8);
  const nozzleMaterial = new MeshStandardMaterial({ color: NOZZLE_COLOUR, flatShading: true });
  // A unit-length cone whose wide base sits at the nozzle and whose tip points along
  // +Y, so scaling Y by the flame length stretches it away from the car.
  const flameGeometry = new ConeGeometry(FLAME_RADIUS_M, 1, 12, 1, true).translate(0, 0.5, 0);

  const flames: Mesh<ConeGeometry, MeshBasicMaterial>[] = [];
  const position = new Vector3();
  const exhaust = new Vector3();
  for (let start = 0; start + FLOATS_PER_NOZZLE <= layout.length; start += FLOATS_PER_NOZZLE) {
    position.fromArray(layout, start);
    exhaust.fromArray(layout, start + 3);

    const nozzle = new Mesh(nozzleGeometry, nozzleMaterial);
    nozzle.position.copy(position);
    nozzle.quaternion.setFromUnitVectors(CONE_AXIS, exhaust);

    const flame = new Mesh(flameGeometry, buildFlameMaterial());
    flame.position.copy(position);
    flame.quaternion.setFromUnitVectors(CONE_AXIS, exhaust);
    flame.visible = false;

    carBody.add(nozzle, flame);
    flames.push(flame);
  }
  return { flames };
}

/** Sizes each flame from its effective throttle (0..1, from `Game.nozzle_throttles`). */
export function updateFlames(flames: Flames, throttles: Float32Array): void {
  flames.flames.forEach((flame, index) => {
    const throttle = throttles[index] ?? 0;
    flame.visible = throttle >= FLAME_MIN_THROTTLE;
    if (flame.visible) {
      flame.scale.set(1, throttle * FLAME_MAX_LENGTH_M, 1);
      flame.material.opacity = throttle * FLAME_MAX_OPACITY;
    }
  });
}

/** Additive and see-through, so overlapping flames glow brighter like gas jets. */
function buildFlameMaterial(): MeshBasicMaterial {
  return new MeshBasicMaterial({
    color: FLAME_COLOUR,
    transparent: true,
    blending: AdditiveBlending,
    depthWrite: false,
  });
}
