// A generic low-poly sports car (SPEC §11.4: no brand shapes).
// The body and cabin are side profiles (flat outlines seen from the left) pushed out
// across the car's width. The body group sits at the car's centre of mass. The wheels are separate
// groups because the sim gives each wheel its own pose (steering and rolling included).
import {
  BoxGeometry,
  CylinderGeometry,
  ExtrudeGeometry,
  Group,
  Mesh,
  MeshStandardMaterial,
  type Object3D,
  Shape,
} from 'three';

const WHEEL_RADIUS_M = 0.34;
const WHEEL_WIDTH_M = 0.26;
/** Body width; narrower than the 2.0 m physics box so the wheels show at the sides. */
export const BODY_WIDTH_M = 1.72;
/** Height of the body's flat underside, relative to the centre of mass. */
export const BODY_BOTTOM_M = -0.4;
/** The glass cabin sits on top of the body and is narrower, like a real car's. */
const CABIN_WIDTH_M = 1.36;

// The shapes stay close to the physics boxes in configs/vehicle.toml (body from
// y = -0.45 to 0.45, roof up to 0.55), so landings and roof hits look right.

/**
 * Lower body outline seen from the left, as [x forward, y up] points in metres around
 * the centre of mass: low nose, sloping bonnet, flat deck up to the rear spoiler lip.
 */
const BODY_PROFILE_M: [number, number][] = [
  [2.1, BODY_BOTTOM_M],
  [2.1, 0.02],
  [0.9, 0.22],
  [-1.95, 0.26],
  [-2.1, 0.32],
  [-2.1, BODY_BOTTOM_M],
];
/** The points of `BODY_PROFILE_M` that trace its top edge, from nose to tail. */
const BODY_TOP_EDGE_M = BODY_PROFILE_M.slice(1, 5);

/** Cabin outline: raked windscreen, short roof, long fastback rear window. */
const CABIN_PROFILE_M: [number, number][] = [
  [0.9, 0.22],
  [0.35, 0.55],
  [-0.8, 0.55],
  [-1.75, 0.25],
];

/** The body paint, shared with the nozzle pylons (nozzle-mounts.ts). */
export const paint = new MeshStandardMaterial({
  color: 0xe0542f,
  flatShading: true,
  roughness: 0.45,
});
// Light glass and hubs so the car still reads against the dark arena floor when it flips.
const glass = new MeshStandardMaterial({ color: 0x8fb0c8, flatShading: true, roughness: 0.15 });
const tyre = new MeshStandardMaterial({ color: 0x1b1b1b, flatShading: true, roughness: 0.9 });
const hub = new MeshStandardMaterial({ color: 0xc8c8c8, flatShading: true, metalness: 0.6 });

// One geometry of each part, shared by every car built (the training viewer draws 64).
const bodyGeometry = extrudeProfile(BODY_PROFILE_M, BODY_WIDTH_M);
const cabinGeometry = extrudeProfile(CABIN_PROFILE_M, CABIN_WIDTH_M);
const tyreGeometry = new CylinderGeometry(WHEEL_RADIUS_M, WHEEL_RADIUS_M, WHEEL_WIDTH_M, 14);
const hubGeometry = new BoxGeometry(WHEEL_RADIUS_M * 1.4, 0.08, WHEEL_WIDTH_M + 0.02);

/** The car body: a painted lower body with a dark glass cabin on top. */
export function buildCarBody(): Group {
  const body = new Group();
  body.add(new Mesh(bodyGeometry, paint));
  body.add(new Mesh(cabinGeometry, glass));
  castShadows(body);
  return body;
}

/**
 * Height of the lower body's top surface at `xM` metres forward of the centre of mass
 * (bonnet or deck), read off the side outline. Outside the body it gives the nearest end.
 */
export function bodyTopHeightM(xM: number): number {
  for (let index = 0; index + 1 < BODY_TOP_EDGE_M.length; index += 1) {
    const [frontX, frontY] = BODY_TOP_EDGE_M[index] ?? [0, 0];
    const [rearX, rearY] = BODY_TOP_EDGE_M[index + 1] ?? [0, 0];
    if (xM <= frontX && xM >= rearX) {
      const along = (frontX - xM) / (frontX - rearX);
      return frontY + along * (rearY - frontY);
    }
  }
  const nearestEnd = xM > 0 ? BODY_TOP_EDGE_M[0] : BODY_TOP_EDGE_M[BODY_TOP_EDGE_M.length - 1];
  return nearestEnd?.[1] ?? 0;
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

/** Turns a side outline into a solid `width_m` wide, centred on z = 0. */
function extrudeProfile(points: [number, number][], widthM: number): ExtrudeGeometry {
  const outline = new Shape();
  points.forEach(([x, y], index) => {
    if (index === 0) {
      outline.moveTo(x, y);
    } else {
      outline.lineTo(x, y);
    }
  });
  outline.closePath();

  const geometry = new ExtrudeGeometry(outline, { depth: widthM, bevelEnabled: false });
  // ExtrudeGeometry grows along +Z from 0; shift it so the car is centred.
  geometry.translate(0, 0, -widthM / 2);
  return geometry;
}

function castShadows(object: Object3D): void {
  object.traverse((child) => {
    child.castShadow = true;
  });
}
