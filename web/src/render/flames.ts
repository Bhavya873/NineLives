// Thruster ports and their cold-gas flames (SPEC §11.4): a small flush port on the body
// for each nozzle (nozzle-ports.ts) and an additive blue-white cone from it whose length
// and opacity follow the nozzle's effective throttle. Cold gas, so never fire colours.
// They hang off the car body group, so they move and hide with the car. Rendering only:
// the sim decides thrust.
import {
  AdditiveBlending,
  ConeGeometry,
  type Group,
  Mesh,
  MeshBasicMaterial,
  Vector3,
} from 'three';

import type { NozzlePlace } from '../wasm-bridge/nozzle-layout.ts';
import { SIDE_PORT_RADIUS_M, buildNozzlePort, portPositionM, portSurface } from './nozzle-ports.ts';

/** Flame length at full throttle. */
const FLAME_MAX_LENGTH_M = 1.4;
/** Flame radius at a lift or tail port; just wider than the 0.06 m port, so it reads as a jet. */
const FLAME_RADIUS_M = 0.08;
/** Flame radius at a side port, in the same proportion to the smaller port. */
const SIDE_FLAME_RADIUS_M = SIDE_PORT_RADIUS_M * 1.3;
/** Below this throttle the flame is hidden rather than drawn as a sliver. */
const FLAME_MIN_THROTTLE = 0.02;
const FLAME_MAX_OPACITY = 0.85;
const FLAME_COLOUR = 0xd6ecff;

/** One nozzle's flame. */
type Flame = Mesh<ConeGeometry, MeshBasicMaterial>;

/** The way three.js cones point before they are turned toward the exhaust. */
const CONE_AXIS = new Vector3(0, 1, 0);

/**
 * Adds a port with a (hidden) flame to `carBody` for each place in `nozzles`, and
 * returns the flames in nozzle order.
 */
export function createFlames(carBody: Group, nozzles: NozzlePlace[]): Flame[] {
  // A unit-length cone with its wide base at the nozzle and its tip along +Y, so
  // scaling Y by the flame length stretches it away from the car.
  const flameGeometry = unitCone(FLAME_RADIUS_M);
  const sideFlameGeometry = unitCone(SIDE_FLAME_RADIUS_M);

  const flames: Flame[] = [];
  const exhaust = new Vector3();
  for (const place of nozzles) {
    // The flame starts at the port's outer face and points along the exhaust.
    const geometry = portSurface(place) === 'side' ? sideFlameGeometry : flameGeometry;
    const flame = new Mesh(geometry, buildFlameMaterial());
    flame.position.fromArray(portPositionM(place));
    flame.quaternion.setFromUnitVectors(CONE_AXIS, exhaust.fromArray(place.exhaust));
    flame.visible = false;
    carBody.add(buildNozzlePort(place), flame);
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

/** An open cone 1 m long with a `radiusM` base at the origin and its tip along +Y. */
function unitCone(radiusM: number): ConeGeometry {
  return new ConeGeometry(radiusM, 1, 12, 1, true).translate(0, 0.5, 0);
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
