// The car's outline in numbers (SPEC §11.4): side profiles of the body and cabin, the
// wheel arches cut into the body, and helpers that turn a side outline into a bevelled
// solid. car-mesh.ts assembles the parts; nozzle-ports.ts reads the body's sides and
// underside from here so every thruster port sits on the paint.
// All shapes stay inside the physics boxes in configs/vehicle.toml (body x ±2.1,
// y ±0.45, z ±1.0, roof up to y = 0.55), so landings and roof hits look right.
import { type BufferGeometry, ExtrudeGeometry, Shape } from 'three';
import { TessellateModifier } from 'three/examples/jsm/modifiers/TessellateModifier.js';

/** Body width; narrower than the 2.0 m physics box so the tyres show at the sides. */
export const BODY_WIDTH_M = 1.76;
/** Height of the body's flat floor between the wheels, relative to the centre of mass. */
export const BODY_BOTTOM_M = -0.4;
/** Where the wheel arches are centred along the car (the axles in configs/vehicle.toml). */
const ARCH_CENTRES_X_M = [1.3, -1.3];
/** Arch radius: the 0.34 m tyre plus room to steer and bounce. */
const ARCH_RADIUS_M = 0.39;
/** Arch centre height: where the wheel centre sits with the car at rest. */
const ARCH_CENTRE_Y_M = -0.25;
/** Straight pieces used to draw each arch. */
const ARCH_SEGMENTS = 7;

/** How far the bevel rounds off the body's edges, across and along the outline. */
export const BODY_BEVEL_M = 0.05;

/**
 * Top edge of the body seen from the left, as [x forward, y up] points from nose to
 * tail: a low, smooth nose, a long bonnet sloping up to a set-back windscreen, a flat
 * rear deck, and a ducktail lip in place of a wing. Over the front side ports (x 1.4 m)
 * the edge sits at about 0.28 m, so its 0.05 m rounding stays above the ports' tops
 * (0.23 m).
 */
const BODY_TOP_EDGE_M: [number, number][] = [
  [2.1, -0.04],
  [2.08, 0.06],
  [1.98, 0.16],
  [1.7, 0.27],
  [1.3, 0.29],
  [0.8, 0.295],
  [-1.4, 0.29],
  [-1.9, 0.32],
  [-2.06, 0.33],
  [-2.1, 0.22],
];

/**
 * Height of the body's top edge at `xM` metres forward, read off `BODY_TOP_EDGE_M` by
 * straight lines between its points. Lamps, mirrors and side ports are placed from it.
 */
export function bodyTopM(xM: number): number {
  let front = BODY_TOP_EDGE_M[0];
  for (const back of BODY_TOP_EDGE_M.slice(1)) {
    if (front && xM <= front[0] && xM >= back[0]) {
      const along = (front[0] - xM) / (front[0] - back[0]);
      return front[1] + (back[1] - front[1]) * along;
    }
    front = back;
  }
  throw new Error(`car-shape.ts: x = ${xM} m is off the car (the body runs from -2.1 to 2.1 m)`);
}

/**
 * Height of the flank's bottom edge at `xM` metres forward: the top of the wheel arch
 * where there is one, else the floor. Only for the middle of the car (|x| ≤ 1.8 m),
 * before the nose and tail undersides are cut away.
 */
export function flankBottomM(xM: number): number {
  if (Math.abs(xM) > 1.8) {
    throw new Error(`car-shape.ts: flankBottomM covers |x| ≤ 1.8 m, got ${xM} m`);
  }
  for (const centreXM of ARCH_CENTRES_X_M) {
    const offsetM = xM - centreXM;
    if (Math.abs(offsetM) < ARCH_RADIUS_M) {
      return ARCH_CENTRE_Y_M + Math.sqrt(ARCH_RADIUS_M ** 2 - offsetM ** 2);
    }
  }
  return BODY_BOTTOM_M;
}

/**
 * Lower outline from tail to nose: the tail and nose undersides are cut away like the
 * physics box's underside chamfer, with an arch over each axle in between.
 */
function bodyBottomEdgeM(): [number, number][] {
  const [frontArchX = 1.3, rearArchX = -1.3] = ARCH_CENTRES_X_M;
  return [
    [-2.1, -0.16],
    [-1.8, -0.36],
    ...archPointsM(rearArchX),
    ...archPointsM(frontArchX),
    [1.8, -0.36],
    [2.1, -0.16],
  ];
}

/** The body's full side outline: the top edge then the bottom edge, as one loop. */
export function bodyProfileM(): [number, number][] {
  return [...BODY_TOP_EDGE_M, ...bodyBottomEdgeM()];
}

/**
 * A plain side outline with no arches, for the dark wheel-well liner that fills the
 * space between the arches, so no daylight shows through the car above the tyres.
 */
export const LINER_PROFILE_M: [number, number][] = [
  [1.9, 0.05],
  [0.9, 0.22],
  [-1.9, 0.24],
  [-1.9, BODY_BOTTOM_M],
  [1.9, BODY_BOTTOM_M],
];

/** Cabin outline: a low cabin with a steeply raked windscreen, a short roof and a long
 * fastback down to the deck. */
export const CABIN_PROFILE_M: [number, number][] = [
  [0.8, 0.28],
  [0.0, 0.47],
  [-0.55, 0.47],
  [-1.55, 0.29],
];

/**
 * Points along one wheel arch, from its rear foot over the top to its front foot,
 * where the circle meets the floor at `BODY_BOTTOM_M`.
 */
function archPointsM(centreXM: number): [number, number][] {
  // The feet sit below the arch centre, at angles π + footAngle (rear) and −footAngle
  // (front), measured from +x with y up.
  const footDropM = ARCH_CENTRE_Y_M - BODY_BOTTOM_M;
  const footAngleRad = Math.asin(footDropM / ARCH_RADIUS_M);
  const points: [number, number][] = [];
  // Sweep from the rear foot over the top of the arch to the front foot.
  const startRad = Math.PI + footAngleRad;
  const endRad = -footAngleRad;
  for (let step = 0; step <= ARCH_SEGMENTS; step += 1) {
    const angleRad = startRad + ((endRad - startRad) * step) / ARCH_SEGMENTS;
    points.push([
      centreXM + ARCH_RADIUS_M * Math.cos(angleRad),
      ARCH_CENTRE_Y_M + ARCH_RADIUS_M * Math.sin(angleRad),
    ]);
  }
  return points;
}

/**
 * If `xM` lies over a wheel arch, the x of that arch's end nearer the car's end (where
 * solid body starts again under the nose or tail); otherwise `xM` itself. Downward
 * lift ports use it to sit past the arch, under the body.
 */
export function solidUndersideXM(xM: number): number {
  for (const centreXM of ARCH_CENTRES_X_M) {
    if (Math.abs(xM - centreXM) < ARCH_RADIUS_M) {
      return centreXM + Math.sign(centreXM) * ARCH_RADIUS_M;
    }
  }
  return xM;
}

/** Past this distance from the centre (either way along the car), the body narrows. */
const NOSE_TAPER_START_M = 1.4;
/** How much narrower the very nose and tail are than the middle (0.16 = 16 %). */
const NOSE_TAPER = 0.16;
/**
 * How much of the full width the car keeps at `xM` metres forward, 0..1: full width
 * between the wheels, narrowing toward the nose and tail for a rounded plan view.
 * (The bevel on the body's edges already makes it narrower at the top.)
 */
function bodyWidthScale(xM: number): number {
  const taperLengthM = 2.1 - NOSE_TAPER_START_M;
  const noseAmount = Math.min(1, Math.max(0, (Math.abs(xM) - NOSE_TAPER_START_M) / taperLengthM));
  // Squared, so the nose curves in rather than cutting a straight wedge.
  return 1 - NOSE_TAPER * noseAmount * noseAmount;
}

/** Half the body's width at `xM` metres forward (see `bodyWidthScale`). */
export function bodyHalfWidthM(xM: number): number {
  return (BODY_WIDTH_M / 2) * bodyWidthScale(xM);
}

/** Longest triangle edge before warping, so the long flat sides bend in small facets. */
const WARP_EDGE_M = 0.3;

/** Squeezes a body part across the car by `bodyWidthScale`, so every part follows the
 * same rounded plan. Returns a new, finer-cut geometry. */
export function warpToBodyShape(original: BufferGeometry): BufferGeometry {
  // Cut long triangles first: a big flat triangle squeezed unevenly would show a crease.
  const geometry = new TessellateModifier(WARP_EDGE_M, 8).modify(original);
  const positions = geometry.getAttribute('position');
  for (let index = 0; index < positions.count; index += 1) {
    const scale = bodyWidthScale(positions.getX(index));
    positions.setZ(index, positions.getZ(index) * scale);
  }
  positions.needsUpdate = true;
  geometry.computeVertexNormals();
  return geometry;
}

/** How a side outline becomes a solid: its width and how much its edges are rounded. */
interface ExtrudeOptions {
  widthM: number;
  /** Edge rounding in metres; 0 for sharp edges. */
  bevelM: number;
  /** Centre of the solid across the car (z); 0 for the middle. */
  centreZM: number;
}

/**
 * Turns a side outline into a solid `widthM` wide centred on `centreZM`. The bevel
 * rounds every edge inward, so the outline and width stay the outer size.
 */
export function extrudeProfile(
  points: [number, number][],
  options: ExtrudeOptions,
): ExtrudeGeometry {
  const outline = new Shape();
  points.forEach(([x, y], index) => {
    if (index === 0) {
      outline.moveTo(x, y);
    } else {
      outline.lineTo(x, y);
    }
  });
  outline.closePath();

  const bevelM = options.bevelM;
  const geometry = new ExtrudeGeometry(outline, {
    depth: options.widthM - 2 * bevelM,
    bevelEnabled: bevelM > 0,
    bevelThickness: bevelM,
    bevelSize: bevelM,
    // Pull the bevel inside the outline, so the solid stays within the physics box.
    bevelOffset: -bevelM,
    bevelSegments: 2,
  });
  // ExtrudeGeometry grows along +Z from 0 (plus the bevel); centre it on `centreZM`.
  geometry.translate(0, 0, options.centreZM - options.widthM / 2 + bevelM);
  return geometry;
}
