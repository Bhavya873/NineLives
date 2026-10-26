// A generic low-poly sports car (SPEC §11.4: no brand shapes, names or logos).
// The painted body, glass cabin and dark trim are side outlines (car-shape.ts) pushed out
// across the car with bevelled edges, plus small boxes for lights, a rear diffuser,
// skirts, a slim front intake and wing mirrors. Parts that share a material are merged
// into one mesh, so each car costs a handful of draw calls (the training viewer draws
// 64). The body group sits at the car's centre of mass. Wheels are separate groups: the
// sim gives each its own pose.
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

/** The body paint. */
const paint = new MeshStandardMaterial({
  color: 0xe0542f,
  flatShading: true,
  roughness: 0.4,
  metalness: 0.15,
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
const rim = new MeshStandardMaterial({ color: 0x8a9099, flatShading: true, metalness: 0.7 });

// One merged geometry per material, shared by every car built.
const paintGeometry = mergeParts([
  extrudeProfile(bodyProfileM(), { widthM: BODY_WIDTH_M, bevelM: BODY_BEVEL_M, centreZM: 0 }),
  roofPanel(),
  ...wingMirrors(),
]);
const glassGeometry = mergeParts([
  extrudeProfile(CABIN_PROFILE_M, { widthM: CABIN_WIDTH_M, bevelM: CABIN_BEVEL_M, centreZM: 0 }),
]);
const trimGeometry = mergeParts([
  extrudeProfile(LINER_PROFILE_M, { widthM: LINER_WIDTH_M, bevelM: 0, centreZM: 0 }),
  ...rearDiffuser(),
  ...sideSkirts(),
  ...frontIntake(),
]);
const headlightGeometry = mergeParts(headlights());
const taillightGeometry = mergeParts(taillights());
const tyreGeometry = new CylinderGeometry(WHEEL_RADIUS_M, WHEEL_RADIUS_M, WHEEL_WIDTH_M, 14);
/** Rim radius: the tyre's sidewall shows as a dark band around it. */
const RIM_RADIUS_M = 0.23;
const rimGeometry = new CylinderGeometry(RIM_RADIUS_M, RIM_RADIUS_M, WHEEL_WIDTH_M + 0.01, 14);
/** Five spokes across the rim's outer faces, so a turning wheel shows its spin. */
const spokeGeometry = mergeOrThrow(
  [0, 1, 2, 3, 4].map((index) => {
    const spoke = new BoxGeometry(RIM_RADIUS_M * 0.9, 0.045, WHEEL_WIDTH_M + 0.03);
    spoke.translate(RIM_RADIUS_M * 0.45, 0, 0);
    spoke.rotateZ((index * 2 * Math.PI) / 5);
    return spoke;
  }),
);
const spoke = new MeshStandardMaterial({ color: 0x5a5f66, flatShading: true, metalness: 0.5 });

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

/** One wheel: a tyre with its axle along the body's Z axis, a light rim and dark spokes. */
export function buildWheel(): Group {
  const wheel = new Group();

  for (const [geometry, material] of [
    [tyreGeometry, tyre],
    [rimGeometry, rim],
  ] as const) {
    const disc = new Mesh(geometry, material);
    // Cylinders stand along Y in three.js; lay this one along Z, the sim's axle axis.
    disc.rotation.x = Math.PI / 2;
    wheel.add(disc);
  }
  wheel.add(new Mesh(spokeGeometry, spoke));

  castShadows(wheel);
  return wheel;
}

/** A painted roof panel over the glass cabin's short roof. */
function roofPanel(): BufferGeometry {
  return boxAt([-0.57, 0.455, -0.6], [0.02, 0.49, 0.6]);
}

/** A wing mirror on each door, just ahead of the side window. */
function wingMirrors(): BufferGeometry[] {
  return [-1, 1].map((side) => boxAt([0.5, 0.27, side * 0.7], [0.6, 0.33, side * 0.85]));
}

/** A slim dark intake low across the smooth, grille-less nose. */
function frontIntake(): BufferGeometry[] {
  return [boxAt([2.07, -0.12, -0.42], [2.125, -0.09, 0.42])];
}

/** A dark diffuser under the tail, following the cut-away underside. */
function rearDiffuser(): BufferGeometry[] {
  const outline: [number, number][] = [
    [-1.99, -0.1],
    [-1.84, -0.27],
    [-1.86, -0.34],
    [-2.11, -0.17],
    [-2.11, -0.1],
  ];
  return [extrudeProfile(outline, { widthM: BODY_WIDTH_M + 0.02, bevelM: 0.04, centreZM: 0 })];
}

/** A dark sill along each side between the wheel arches. */
function sideSkirts(): BufferGeometry[] {
  const outerZM = BODY_WIDTH_M / 2 + 0.02;
  return [-1, 1].map((side) =>
    boxAt([-0.93, BODY_BOTTOM_M - 0.03, side * (outerZM - 0.06)], [0.93, -0.28, side * outerZM]),
  );
}

/** Two slim headlight strips laid on the sloping nose. */
function headlights(): BufferGeometry[] {
  // The nose slopes up from (2.08, 0.06) to (1.98, 0.15); lay the lamps along it.
  const slopeRad = Math.atan2(0.09, 0.1);
  return [-1, 1].map((side) => {
    const lamp = new BoxGeometry(0.13, 0.022, 0.34);
    lamp.rotateZ(-slopeRad);
    lamp.translate(2.03, 0.105, side * 0.58);
    return lamp;
  });
}

/** One thin light bar across the whole tail, under the ducktail and above the rear ports. */
function taillights(): BufferGeometry[] {
  return [boxAt([-2.115, 0.13, -0.8], [-2.085, 0.165, 0.8])];
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
  return warpToBodyShape(mergeOrThrow(parts));
}

/** Merges parts into one geometry; they may mix indexed and non-indexed geometry. */
function mergeOrThrow(parts: BufferGeometry[]): BufferGeometry {
  const unindexed = parts.map((part) => (part.index ? part.toNonIndexed() : part));
  const merged = mergeGeometries(unindexed);
  if (!merged) {
    throw new Error('car-mesh.ts: the car parts could not be merged (mismatched attributes)');
  }
  return merged;
}

function castShadows(object: Object3D): void {
  object.traverse((child) => {
    child.castShadow = true;
  });
}
