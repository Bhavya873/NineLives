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
const BODY_WIDTH_M = 1.72;
/** The glass cabin sits on top of the body and is narrower, like a real car's. */
const CABIN_WIDTH_M = 1.36;

// The shapes stay close to the physics boxes in configs/vehicle.toml (body from
// y = -0.45 to 0.45, roof up to 0.55), so landings and roof hits look right. The body
// is drawn narrower than the 2.0 m physics box so the wheels show at the sides.

/**
 * Lower body outline seen from the left, as [x forward, y up] points in metres around
 * the centre of mass: low nose, sloping bonnet, flat deck up to the rear spoiler lip.
 */
const BODY_PROFILE_M: [number, number][] = [
  [2.1, -0.4],
  [2.1, 0.02],
  [0.9, 0.22],
  [-1.95, 0.26],
  [-2.1, 0.32],
  [-2.1, -0.4],
];

/** Cabin outline: raked windscreen, short roof, long fastback rear window. */
const CABIN_PROFILE_M: [number, number][] = [
  [0.9, 0.22],
  [0.35, 0.55],
  [-0.8, 0.55],
  [-1.75, 0.25],
];

const paint = new MeshStandardMaterial({ color: 0xe0542f, flatShading: true, roughness: 0.45 });
const glass = new MeshStandardMaterial({ color: 0x1d2a3a, flatShading: true, roughness: 0.15 });
const tyre = new MeshStandardMaterial({ color: 0x1b1b1b, flatShading: true, roughness: 0.9 });
const hub = new MeshStandardMaterial({ color: 0xc8c8c8, flatShading: true, metalness: 0.6 });

/** The car body: a painted lower body with a dark glass cabin on top. */
export function buildCarBody(): Group {
  const body = new Group();
  body.add(new Mesh(extrudeProfile(BODY_PROFILE_M, BODY_WIDTH_M), paint));
  body.add(new Mesh(extrudeProfile(CABIN_PROFILE_M, CABIN_WIDTH_M), glass));
  castShadows(body);
  return body;
}

/** One wheel: a tyre with its axle along the body's Z axis, plus a hub bar to show spin. */
export function buildWheel(): Group {
  const wheel = new Group();

  const tyreMesh = new Mesh(
    new CylinderGeometry(WHEEL_RADIUS_M, WHEEL_RADIUS_M, WHEEL_WIDTH_M, 14),
    tyre,
  );
  // Cylinders stand along Y in three.js; lay this one along Z, the sim's axle axis.
  tyreMesh.rotation.x = Math.PI / 2;
  wheel.add(tyreMesh);

  const hubBar = new Mesh(new BoxGeometry(WHEEL_RADIUS_M * 1.4, 0.08, WHEEL_WIDTH_M + 0.02), hub);
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
