// Where each thruster shows on the car (SPEC §11.4): a small flush port on the body
// surface nearest its physics position, so the thrusters are discreet instead of hanging
// off the body on pylons. Side nozzles show on the flanks under the beltline, lift
// nozzles under the body clear of the wheel arches, and rear nozzles in the tail like
// exhaust tips. Rendering only: the sim's nozzle positions are never moved, and a port
// is at most a few tenths of a metre from where its force acts.
import { CylinderGeometry, Mesh, MeshStandardMaterial, Vector3 } from 'three';

import type { NozzlePlace } from '../nozzle-layout.ts';
import { BODY_BOTTOM_M, bodyHalfWidthM, solidUndersideXM } from './car-shape.ts';

/**
 * Height of the side ports' centres: midway up the flat band of flank between the
 * bevelled wheel-arch lip (about 0.18 m at x ±1.4) and the bevelled top edge (about 0.235 m).
 */
const SIDE_PORT_HEIGHT_M = 0.206;
/** Lift ports sit this far beyond the end of a wheel arch, under solid body. */
const LIFT_PORT_ARCH_CLEARANCE_M = 0.08;
/** Lift ports stay this far in from the body's side, under the flat floor. */
const LIFT_PORT_SIDE_INSET_M = 0.16;
/** The rear face of the tail, where the rear ports sit. */
const TAIL_X_M = -2.11;
/** How far a port's rim stands proud of the paint, so it reads without sticking out. */
const PORT_PROUD_M = 0.01;

/** Rim radius of the lift and tail ports, in metres. */
const PORT_RADIUS_M = 0.06;
/** Rim radius of the side ports: smaller, to fit the narrow flank above the arches. */
export const SIDE_PORT_RADIUS_M = 0.026;
/** How deep every port sinks along its exhaust, in metres. */
const PORT_DEPTH_M = 0.04;

/** Dark metal, so the ports read as openings in the paint. */
const portMaterial = new MeshStandardMaterial({
  color: 0x30353c,
  flatShading: true,
  metalness: 0.5,
  roughness: 0.4,
});
const portGeometry = new CylinderGeometry(PORT_RADIUS_M, PORT_RADIUS_M, PORT_DEPTH_M, 10);
const sidePortGeometry = new CylinderGeometry(
  SIDE_PORT_RADIUS_M,
  SIDE_PORT_RADIUS_M,
  PORT_DEPTH_M,
  10,
);

/** The way three.js cylinders point before they are turned toward the exhaust. */
const CYLINDER_AXIS = new Vector3(0, 1, 0);

/** Which part of the body a port sits on: a flank, the underside, or the tail. */
export type PortSurface = 'side' | 'under' | 'tail';

/** Picks a port's surface by the main axis of its nozzle's exhaust. */
export function portSurface(place: NozzlePlace): PortSurface {
  const [exhaustX, exhaustY, exhaustZ] = place.exhaust.map(Math.abs) as [number, number, number];
  if (exhaustZ >= exhaustY && exhaustZ >= exhaustX) {
    return 'side';
  }
  return exhaustY >= exhaustX ? 'under' : 'tail';
}

/**
 * Where a nozzle's port shows: the centre of the port's outer face, in the body frame,
 * in metres. Flames start there.
 */
export function portPositionM(place: NozzlePlace): [number, number, number] {
  const [forwardM, upM, rightM] = place.positionM;
  const side = Math.sign(rightM) || 1;
  switch (portSurface(place)) {
    case 'side':
      return [forwardM, SIDE_PORT_HEIGHT_M, side * (bodyHalfWidthM(forwardM) + PORT_PROUD_M)];
    case 'under': {
      const clearXM = liftPortXM(forwardM);
      const insideZM = Math.min(Math.abs(rightM), bodyHalfWidthM(clearXM) - LIFT_PORT_SIDE_INSET_M);
      return [clearXM, BODY_BOTTOM_M - PORT_PROUD_M, side * insideZM];
    }
    case 'tail':
      return [TAIL_X_M - PORT_PROUD_M, upM, rightM];
  }
}

/** Along the car, a lift port sits at its nozzle, or just past the wheel arch above it. */
function liftPortXM(forwardM: number): number {
  const solidXM = solidUndersideXM(forwardM);
  if (solidXM === forwardM) {
    return forwardM;
  }
  return solidXM + Math.sign(solidXM) * LIFT_PORT_ARCH_CLEARANCE_M;
}

/**
 * The port for one nozzle: a short dark cylinder along the exhaust, its outer face at
 * `portPositionM` and the rest sunk into the body.
 */
export function buildNozzlePort(place: NozzlePlace): Mesh {
  const exhaust = new Vector3().fromArray(place.exhaust);
  const geometry = portSurface(place) === 'side' ? sidePortGeometry : portGeometry;
  const port = new Mesh(geometry, portMaterial);
  port.position.fromArray(portPositionM(place)).addScaledVector(exhaust, -PORT_DEPTH_M / 2);
  port.quaternion.setFromUnitVectors(CYLINDER_AXIS, exhaust);
  return port;
}
