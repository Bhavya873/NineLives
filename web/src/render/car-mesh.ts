// A generic low-poly sports car (SPEC §11.4: no brand shapes).
// The painted body, glass cabin and dark trim are side outlines (car-shape.ts) pushed out
// across the car with bevelled edges, plus small boxes for lights, bumpers, skirts and a
// rear wing. Parts that share a material are merged into one mesh, so each car costs a
// handful of draw calls (the training viewer draws 64). The body group sits at the
// car's centre of mass. Wheels are separate groups: the sim gives each its own pose.
import {
  type BufferGeometry,
  BoxGeometry,
  CylinderGeometry,
  Group,
  Mesh,
  MeshStandardMaterial,
  type Object3D,
} from 'three';
import { mergeGeometries } from 'three/examples/jsm/utils/BufferGeometryUtils.js';

import {
  BODY_BEVEL_M,
  BODY_BOTTOM_M,
  BODY_WIDTH_M,
  CABIN_PROFILE_M,
  LINER_PROFILE_M,
  bodyProfileM,
  extrudeProfile,
  warpToBodyShape,
} from './car-shape.ts';

const WHEEL_RADIUS_M = 0.34;
const WHEEL_WIDTH_M = 0.26;
/** The glass cabin is narrower than the body, like a real car's. */
const CABIN_WIDTH_M = 1.34;
const CABIN_BEVEL_M = 0.09;
/** The dark liner between the arches stops short of the tyres' inner faces (z ±0.72). */
const LINER_WIDTH_M = 1.4;

/** The body paint, shared with the nozzle pylons (nozzle-mounts.ts). */
export const paint = new MeshStandardMaterial({
  color: 0xe0542f,
  flatShading: true,
  roughness: 0.45,
});
/** Tinted glass; light enough that the cabin still reads when the car is upside down. */
const glass = new MeshStandardMaterial({
  color: 0x3d5a72,
  flatShading: true,
  roughness: 0.12,
  metalness: 0.4,
});
/** Bumpers, skirts, wing and wheel wells. */
const trim = new MeshStandardMaterial({ color: 0x23262b, flatShading: true, roughness: 0.75 });
const headlight = new MeshStandardMaterial({
  color: 0xffffff,
  emissive: 0xfff2d0,
  emissiveIntensity: 1.6,
  flatShading: true,
});
const taillight = new MeshStandardMaterial({
  color: 0xff3020,
  emissive: 0xff1a10,
  emissiveIntensity: 1.4,
  flatShading: true,
});
const tyre = new MeshStandardMaterial({ color: 0x1b1b1b, flatShading: true, roughness: 0.9 });
const hub = new MeshStandardMaterial({ color: 0xc8c8c8, flatShading: true, metalness: 0.6 });

// One merged geometry per material, shared by every car built.
const paintGeometry = mergeParts([
  extrudeProfile(bodyProfileM(), { widthM: BODY_WIDTH_M, bevelM: BODY_BEVEL_M, centreZM: 0 }),
  ...roofAndPillars(),
]);
const glassGeometry = mergeParts([
  extrudeProfile(CABIN_PROFILE_M, { widthM: CABIN_WIDTH_M, bevelM: CABIN_BEVEL_M, centreZM: 0 }),
]);
const trimGeometry = mergeParts([
  extrudeProfile(LINER_PROFILE_M, { widthM: LINER_WIDTH_M, bevelM: 0, centreZM: 0 }),
  ...bumpers(),
  ...sideSkirts(),
  ...rearWing(),
]);
const headlightGeometry = mergeParts(headlights());
const taillightGeometry = mergeParts(taillights());
const tyreGeometry = new CylinderGeometry(WHEEL_RADIUS_M, WHEEL_RADIUS_M, WHEEL_WIDTH_M, 14);
const hubGeometry = new BoxGeometry(WHEEL_RADIUS_M * 1.4, 0.08, WHEEL_WIDTH_M + 0.02);

/**
 * The car body: painted body (its first child, which the nozzle mounts attach to),
 * glass cabin, dark trim, and the head and tail lights.
 */
export function buildCarBody(): Group {
  const body = new Group();
  body.add(new Mesh(paintGeometry, paint));
  body.add(new Mesh(glassGeometry, glass));
  body.add(new Mesh(trimGeometry, trim));
  body.add(new Mesh(headlightGeometry, headlight));
  body.add(new Mesh(taillightGeometry, taillight));
  castShadows(body);
  return body;
}

/** One wheel: a tyre with its axle along the body's Z axis, plus a hub bar to show spin. */
export function buildWheel(): Group {
  const wheel = new Group();

  const tyreMesh = new Mesh(tyreGeometry, tyre);
  // Cylinders stand along Y in three.js; lay this one along Z, the sim's axle axis.
  tyreMesh.rotation.x = Math.PI / 2;
  wheel.add(tyreMesh);

  const hubBar = new Mesh(hubGeometry, hub);
  wheel.add(hubBar);

  castShadows(wheel);
  return wheel;
}

/** A painted roof panel over the glass and a pillar on each side between the windows. */
function roofAndPillars(): BufferGeometry[] {
  const roof = boxAt([-0.74, 0.505, -0.64], [0.34, 0.545, 0.64]);
  const parts = [roof];
  for (const side of [-1, 1]) {
    // The pillar behind the door window, splitting side glass from the rear quarter.
    parts.push(boxAt([-0.5, 0.27, side * 0.62], [-0.4, 0.52, side * 0.72]));
  }
  return parts;
}

/** Dark bumpers wrapping the lower nose and tail, following the cut-away underside. */
function bumpers(): BufferGeometry[] {
  const front: [number, number][] = [
    [2.11, -0.03],
    [2.11, -0.17],
    [1.86, -0.34],
    [1.84, -0.27],
    [1.99, -0.1],
  ];
  const rear = front.map(([x, y]): [number, number] => [-x, y]);
  const options = { widthM: BODY_WIDTH_M + 0.02, bevelM: 0.04, centreZM: 0 };
  return [extrudeProfile(front, options), extrudeProfile(rear.reverse(), options)];
}

/** A dark sill along each side between the wheel arches. */
function sideSkirts(): BufferGeometry[] {
  const outerZM = BODY_WIDTH_M / 2 + 0.02;
  return [-1, 1].map((side) =>
    boxAt([-0.93, BODY_BOTTOM_M - 0.03, side * (outerZM - 0.06)], [0.93, -0.28, side * outerZM]),
  );
}

/** A small rear wing on two posts, kept under the physics roof height (0.55 m). */
function rearWing(): BufferGeometry[] {
  const parts = [boxAt([-2.07, 0.44, -0.8], [-1.77, 0.48, 0.8])];
  for (const side of [-1, 1]) {
    parts.push(boxAt([-1.95, 0.28, side * 0.42], [-1.89, 0.45, side * 0.48]));
    // End plates at the wing tips.
    parts.push(boxAt([-2.07, 0.38, side * 0.8], [-1.8, 0.5, side * 0.82]));
  }
  return parts;
}

/** Two headlight strips laid on the sloping nose above the bumper. */
function headlights(): BufferGeometry[] {
  // The nose slopes up from (2.07, 0.07) to (1.95, 0.15); lay the lamps along it.
  const slopeRad = Math.atan2(0.08, 0.12);
  return [-1, 1].map((side) => {
    const lamp = new BoxGeometry(0.15, 0.03, 0.4);
    lamp.rotateZ(-slopeRad);
    lamp.translate(2.02, 0.115, side * 0.56);
    return lamp;
  });
}

/** Two taillight bars and a thin centre strip across the tail, between the nozzle plates. */
function taillights(): BufferGeometry[] {
  const parts: BufferGeometry[] = [];
  for (const side of [-1, 1]) {
    parts.push(boxAt([-2.115, 0.06, side * 0.58], [-2.08, 0.18, side * 0.84]));
  }
  parts.push(boxAt([-2.11, 0.13, -0.22], [-2.08, 0.16, 0.22]));
  return parts;
}

/** A box spanning from corner `min` to corner `max`, in metres in the body frame. */
function boxAt(min: [number, number, number], max: [number, number, number]): BoxGeometry {
  const [x0, y0, z0] = min;
  const [x1, y1, z1] = max;
  const box = new BoxGeometry(Math.abs(x1 - x0), Math.abs(y1 - y0), Math.abs(z1 - z0));
  box.translate((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2);
  return box;
}

/** Merges parts into one geometry and gives it the body's rounded plan. */
function mergeParts(parts: BufferGeometry[]): BufferGeometry {
  const unindexed = parts.map((part) => (part.index ? part.toNonIndexed() : part));
  const merged = mergeGeometries(unindexed);
  if (!merged) {
    throw new Error('car-mesh.ts: the car parts could not be merged (mismatched attributes)');
  }
  return warpToBodyShape(merged);
}

function castShadows(object: Object3D): void {
  object.traverse((child) => {
    child.castShadow = true;
  });
}
