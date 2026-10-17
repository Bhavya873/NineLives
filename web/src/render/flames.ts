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

import type { NozzlePlace } from '../nozzle-layout.ts';

/** Flame length at full throttle. */
const FLAME_MAX_LENGTH_M = 1.4;
const FLAME_RADIUS_M = 0.12;
/** Below this throttle the flame is hidden rather than drawn as a sliver. */
const FLAME_MIN_THROTTLE = 0.02;
const FLAME_MAX_OPACITY = 0.85;
const FLAME_COLOUR = 0xd6ecff;
/** Light metal, so the nozzles show against the dark arena floor. */
const NOZZLE_COLOUR = 0xa9b1ba;
/**
 * Nozzle cones are drawn this far out along the exhaust from the physics position, so the
 * lift nozzles (just inside the body box) show below the car.
 */
const NOZZLE_DRAW_OFFSET_M = 0.12;

/** One nozzle's flame. */
export type Flame = Mesh<ConeGeometry, MeshBasicMaterial>;

/** The way three.js cones point before they are turned toward the exhaust. */
const CONE_AXIS = new Vector3(0, 1, 0);

/**
 * Adds a nozzle with a (hidden) flame to `carBody` for each place in `nozzles`, and
 * returns the flames in nozzle order.
 */
export function createFlames(carBody: Group, nozzles: NozzlePlace[]): Flame[] {
  // Narrow at the car, wide toward the gas, like a bell.
  const nozzleGeometry = new CylinderGeometry(0.09, 0.05, 0.12, 8);
  const nozzleMaterial = new MeshStandardMaterial({ color: NOZZLE_COLOUR, flatShading: true });
  // A unit-length cone with its wide base at the nozzle and its tip along +Y, so
  // scaling Y by the flame length stretches it away from the car.
  const flameGeometry = new ConeGeometry(FLAME_RADIUS_M, 1, 12, 1, true).translate(0, 0.5, 0);

  const flames: Flame[] = [];
  const exhaust = new Vector3();
  for (const place of nozzles) {
    // The nozzle cone points along the exhaust; its flame, a child, inherits that.
    const nozzle = new Mesh(nozzleGeometry, nozzleMaterial);
    exhaust.fromArray(place.exhaust);
    nozzle.position.fromArray(place.positionM).addScaledVector(exhaust, NOZZLE_DRAW_OFFSET_M);
    nozzle.quaternion.setFromUnitVectors(CONE_AXIS, exhaust);
    const flame = new Mesh(flameGeometry, buildFlameMaterial());
    flame.visible = false;
    nozzle.add(flame);
    carBody.add(nozzle);
    flames.push(flame);
  }
  return flames;
}

/** Sizes each flame from its effective throttle (0..1, from `Game.nozzle_throttles`). */
export function updateFlames(flames: Flame[], throttles: Float32Array): void {
  for (let index = 0; index < flames.length; index += 1) {
    const flame = flames[index];
    const throttle = throttles[index] ?? 0;
    if (!flame) {
      continue;
    }
    flame.visible = throttle >= FLAME_MIN_THROTTLE;
    if (flame.visible) {
      flame.scale.set(1, throttle * FLAME_MAX_LENGTH_M, 1);
      flame.material.opacity = throttle * FLAME_MAX_OPACITY;
    }
  }
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
