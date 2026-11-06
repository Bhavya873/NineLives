// Parked vehicles for the course's car rows (SPEC §11.4): a plain, generic hatchback,
// pickup, SUV or box truck drawn inside each 4.5 x 1.9 m collider box, so what you land on
// is what you see. The type follows the collider's height. Every roof is one flat slab
// spanning the whole box top and there are no sloped hoods, so a car that comes down
// anywhere on the box never looks like it floats. Paint is one of five muted colours,
// matte; wheels sit inside open arches. Parts are boxes and the car's own tyres and rims
// (car-mesh.ts) with vertex colours; all vehicles of one type in a course merge into one
// mesh, so a car park costs four draw calls however many cars it holds. The collider is an
// axis-aligned box, so vehicles are never turned at an angle of their own.
import {
  type BufferGeometry,
  BoxGeometry,
  BufferAttribute,
  Matrix4,
  Mesh,
  MeshStandardMaterial,
  type Object3D,
} from 'three';
import { mergeGeometries } from 'three/examples/jsm/utils/BufferGeometryUtils.js';

import { WHEEL_RADIUS_M, WHEEL_WIDTH_M, rimGeometry, tyreGeometry } from './car-mesh.ts';

/** The collider box of one parked car (`nl_sim::course_build`), in m. */
export const VEHICLE_LENGTH_M = 4.5;
export const VEHICLE_WIDTH_M = 1.9;

export type VehicleType = 'hatchback' | 'pickup' | 'suv' | 'truck';

/** Collider heights below each limit pick that type; from `TRUCK_MIN_HEIGHT_M` up, a truck. */
const HATCHBACK_MAX_HEIGHT_M = 1.35;
const PICKUP_MAX_HEIGHT_M = 1.65;
const TRUCK_MIN_HEIGHT_M = 1.95;

const HALF_LENGTH_M = VEHICLE_LENGTH_M / 2;
const HALF_WIDTH_M = VEHICLE_WIDTH_M / 2;
/** The roof slab's thickness. */
const ROOF_M = 0.08;
/** The body's side panels sit this far in from the box side; tyres sit just inside them. */
const BODY_HALF_WIDTH_M = 0.92;
const PANEL_M = 0.09;
/** The upper body is a little narrower than the lower, so no faces share a plane. */
const UPPER_HALF_WIDTH_M = 0.9;
/** The body's underside above the ground. */
const BODY_BOTTOM_M = 0.24;

/** The five body paints: white, silver, charcoal, dark red, navy. */
export const PALETTE = ['#dedcd6', '#a3a7ab', '#383b40', '#6b2b2c', '#222d44'];

/** Colours of the parts that do not change with the paint. */
const GLASS = '#161d26';
const TRIM = '#1c1e21';
const HEADLIGHT = '#bdb9a8';
const TAILLIGHT = '#6a2c28';
const TYRE = '#161616';
const RIM = '#6a6e74';
/** Every colour a vehicle's vertices can have. */
export const VEHICLE_COLOURS = [...PALETTE, GLASS, TRIM, HEADLIGHT, TAILLIGHT, TYRE, RIM];

/** The vehicle drawn in a collider `heightM` tall. */
export function vehicleTypeFor(heightM: number): VehicleType {
  if (heightM < HATCHBACK_MAX_HEIGHT_M) {
    return 'hatchback';
  }
  if (heightM < PICKUP_MAX_HEIGHT_M) {
    return 'pickup';
  }
  return heightM >= TRUCK_MIN_HEIGHT_M ? 'truck' : 'suv';
}

/** True for a box shaped like a parked car (a car row's 4.5 x 1.9 m footprint). */
export function isParkedCarBox(halfExtentsM: number[]): boolean {
  const [hx = 0, , hz = 0] = halfExtentsM;
  return Math.abs(hx - HALF_LENGTH_M) < 1e-3 && Math.abs(hz - HALF_WIDTH_M) < 1e-3;
}

/** The deterministic paint of the vehicle numbered `index`. */
export function paintFor(index: number): string {
  return PALETTE[(index * 3) % PALETTE.length] ?? '#dedcd6';
}

/**
 * One vehicle as a single vertex-coloured geometry in its collider's own frame: centred
 * on the box, front towards +X, `heightM` tall. `index` picks the paint.
 */
export function buildVehicleGeometry(heightM: number, index: number): BufferGeometry {
  const type = vehicleTypeFor(heightM);
  const paint = paintFor(index);
  const parts =
    type === 'hatchback'
      ? hatchbackParts(heightM, paint)
      : type === 'pickup'
        ? pickupParts(heightM, paint)
        : type === 'suv'
          ? suvParts(heightM, paint)
          : truckParts(heightM, paint);
  const geometry = mergeColoured(parts);
  // Parts are modelled from the ground up; the collider is centred on its middle.
  geometry.translate(0, -heightM / 2, 0);
  return geometry;
}

/** A vehicle to draw: its geometry's frame is the collider's pose. */
export interface VehiclePlacement {
  heightM: number;
  /** Collider pose: position and rotation as the wasm shape gives them. */
  matrix: Matrix4;
}

/** The material every vehicle shares; colours come from the vertices. */
export function buildVehicleMaterial(): MeshStandardMaterial {
  return new MeshStandardMaterial({
    vertexColors: true,
    flatShading: true,
    roughness: 0.95,
    metalness: 0,
  });
}

/** One mesh per vehicle type present, each merging all of that type's vehicles. */
export function buildVehicleMeshes(
  placements: VehiclePlacement[],
  material: MeshStandardMaterial,
): Object3D[] {
  const byType = new Map<VehicleType, BufferGeometry[]>();
  placements.forEach((placement, index) => {
    const geometry = buildVehicleGeometry(placement.heightM, index);
    geometry.applyMatrix4(placement.matrix);
    const type = vehicleTypeFor(placement.heightM);
    byType.set(type, [...(byType.get(type) ?? []), geometry]);
  });
  const meshes: Object3D[] = [];
  for (const [type, geometries] of byType) {
    const mesh = new Mesh(mergeColoured(geometries), material);
    mesh.name = `vehicles-${type}`;
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    meshes.push(mesh);
  }
  return meshes;
}

/** A hatchback: a low body, a glass band under one flat roof, small light clusters. */
function hatchbackParts(heightM: number, paint: string): BufferGeometry[] {
  const belt = 0.55 * heightM;
  const axles = [1.35, -1.35];
  return [
    ...bodyShell(belt, axles, 1, paint),
    ...cabin(
      belt,
      heightM,
      [
        [-1.75, -0.1],
        [0.15, 1.6],
      ],
      paint,
    ),
    ...lights(0.55, belt - 0.2, 0.1),
    ...wheelSet(axles, 1),
  ];
}

/** An SUV: taller, bigger wheels, roof rails and a window pillar. */
function suvParts(heightM: number, paint: string): BufferGeometry[] {
  const belt = 0.46 * heightM;
  const axles = [1.45, -1.45];
  const rails = [-1, 1].map((side) =>
    box([-1.9, heightM - 0.05, side * 0.68 - 0.04], [1.9, heightM, side * 0.68 + 0.04], TRIM),
  );
  return [
    ...bodyShell(belt, axles, 1.15, paint),
    ...cabin(
      belt,
      heightM,
      [
        [-1.85, -0.2],
        [-0.05, 0.75],
        [0.9, 1.7],
      ],
      paint,
    ),
    ...rails,
    ...lights(0.58, belt - 0.22, 0.14),
    ...wheelSet(axles, 1.15),
  ];
}

/** A pickup: a cab up front and a bed under a flat cover, level with the roof. */
function pickupParts(heightM: number, paint: string): BufferGeometry[] {
  const belt = 0.46 * heightM;
  const roofBottom = heightM - ROOF_M;
  const axles = [1.45, -1.4];
  const coverEnd = 0.2;
  return [
    ...bodyShell(belt, axles, 1.1, paint),
    // The cab has side windows; the cover behind it has one small pane each side.
    ...cabin(
      belt,
      heightM,
      [
        [0.5, 1.7],
        [-1.5, -0.5],
      ],
      paint,
    ),
    box([coverEnd - 0.03, belt, -0.91], [coverEnd + 0.03, roofBottom, 0.91], TRIM),
    box([-2.25, belt + 0.05, -0.04], [-2.2, roofBottom - 0.05, 0.04], TRIM),
    ...lights(0.58, belt - 0.22, 0.14),
    ...wheelSet(axles, 1.1),
  ];
}

/** A cab-over box truck: cab and cargo box share one full-length roof. */
function truckParts(heightM: number, paint: string): BufferGeometry[] {
  const half = 0.88;
  const floor = 0.55;
  const cabX = 0.6;
  const roofBottom = heightM - ROOF_M;
  const windowBottom = 0.58 * heightM;
  const axles = [1.5, -1.0, -1.7];
  return [
    // Frame and wheel wells, the cargo box, the cab and a rear door seam.
    box([-2.2, 0.3, -0.7], [2.2, floor, 0.7], TRIM),
    ...wheelWells(axles, 1.2, floor),
    box([-2.2, floor, -half], [cabX, roofBottom, half], paint),
    box([cabX, floor, -half], [2.2, roofBottom, half], paint),
    box([-2.25, floor + 0.1, -0.02], [-2.19, roofBottom - 0.1, 0.02], TRIM),
    // Glass: windscreen and a side window on each side of the cab.
    box([2.2, windowBottom, -0.78], [2.25, roofBottom - 0.08, 0.78], GLASS),
    ...[-1, 1].map((side) =>
      box(
        [0.95, windowBottom, side > 0 ? half : -half - 0.03],
        [2.0, roofBottom - 0.08, side > 0 ? half + 0.03 : -half],
        GLASS,
      ),
    ),
    // One flat roof over everything.
    box([-HALF_LENGTH_M, roofBottom, -0.93], [HALF_LENGTH_M, heightM, 0.93], paint),
    box([2.2, 0.38, -0.8], [HALF_LENGTH_M, 0.62, 0.8], TRIM),
    ...[-1, 1].map((side) =>
      box([2.2, 0.62, side * 0.62 - 0.14], [2.25, 0.8, side * 0.62 + 0.14], HEADLIGHT),
    ),
    ...wheelSet(axles, 1.2),
  ];
}

/**
 * The lower body from the underside to the belt line: a dark core, painted side panels
 * with an open arch over each axle, bumpers and a painted deck.
 */
function bodyShell(
  belt: number,
  axlesX: number[],
  wheelScale: number,
  paint: string,
): BufferGeometry[] {
  const radius = WHEEL_RADIUS_M * wheelScale;
  const archHalf = radius + 0.12;
  const archTop = Math.min(radius + 0.14, belt - 0.12);
  const edges = [
    -2.2,
    ...[...axlesX].sort((a, b) => a - b).flatMap((x) => [x - archHalf, x + archHalf]),
    2.2,
  ];
  const inner = BODY_HALF_WIDTH_M - PANEL_M;
  const parts = [
    box([-2.1, BODY_BOTTOM_M, -inner], [2.1, belt - 0.04, inner], TRIM),
    box([2.1, BODY_BOTTOM_M, -UPPER_HALF_WIDTH_M], [2.25, belt - 0.1, UPPER_HALF_WIDTH_M], TRIM),
    box([-2.25, BODY_BOTTOM_M, -UPPER_HALF_WIDTH_M], [-2.1, belt - 0.1, UPPER_HALF_WIDTH_M], TRIM),
    box([-2.25, belt - 0.08, -BODY_HALF_WIDTH_M], [2.25, belt, BODY_HALF_WIDTH_M], paint),
  ];
  for (const side of [-1, 1]) {
    const [near, far] = [inner * side, BODY_HALF_WIDTH_M * side];
    // The strip over the arches, then the panels between them.
    parts.push(box([-2.2, archTop, near], [2.2, belt - 0.08, far], paint));
    for (let edge = 0; edge < edges.length; edge += 2) {
      parts.push(
        box([edges[edge] ?? 0, BODY_BOTTOM_M, near], [edges[edge + 1] ?? 0, archTop, far], paint),
      );
    }
  }
  return parts;
}

/** Dark wheel wells under a truck's floor, so each wheel sits in a recess. */
function wheelWells(axlesX: number[], wheelScale: number, floor: number): BufferGeometry[] {
  const reach = WHEEL_RADIUS_M * wheelScale + 0.03;
  return axlesX.flatMap((x) =>
    [-1, 1].map((side) =>
      box(
        [x - reach, floor - 0.12, side * 0.7],
        [x + reach, floor, side * (HALF_WIDTH_M - 0.04)],
        TRIM,
      ),
    ),
  );
}

/**
 * The upper body: a painted block from the belt line to the roof with dark window panes
 * set into it (side windows over `sideSpans`, a windscreen and a rear window), under one
 * full-box roof slab.
 */
function cabin(
  belt: number,
  heightM: number,
  sideSpans: [number, number][],
  paint: string,
): BufferGeometry[] {
  const roofBottom = heightM - ROOF_M;
  const sill = belt + 0.07;
  const top = roofBottom - 0.07;
  const half = UPPER_HALF_WIDTH_M;
  return [
    box([-2.2, belt, -half], [2.2, roofBottom, half], paint),
    box([-HALF_LENGTH_M, roofBottom, -HALF_WIDTH_M], [HALF_LENGTH_M, heightM, HALF_WIDTH_M], paint),
    box([2.2, sill, -0.74], [2.23, top, 0.74], GLASS),
    box([-2.23, sill, -0.72], [-2.2, top, 0.72], GLASS),
    ...sideSpans.flatMap(([from, to]) =>
      [-1, 1].map((side) =>
        box(
          [from, sill, side > 0 ? half : -half - 0.015],
          [to, top, side > 0 ? half + 0.015 : -half],
          GLASS,
        ),
      ),
    ),
  ];
}

/** Head and tail light boxes on the bumper ends, `reach` m either side of the middle. */
function lights(reach: number, bottom: number, size: number): BufferGeometry[] {
  const grille = box([2.2, bottom, -reach + 0.25], [2.23, bottom + size, reach - 0.25], TRIM);
  const lamps = [-1, 1].flatMap((side) => [
    box(
      [2.2, bottom, side * reach - 0.2],
      [HALF_LENGTH_M, bottom + size, side * reach + 0.2],
      HEADLIGHT,
    ),
    box(
      [-HALF_LENGTH_M, bottom, side * reach - 0.2],
      [-2.2, bottom + size, side * reach + 0.2],
      TAILLIGHT,
    ),
  ]);
  return [grille, ...lamps];
}

/** Wheels at each x, on both sides, scaled by `scale`, tucked just inside the side panels. */
function wheelSet(axlesX: number[], scale: number): BufferGeometry[] {
  const radius = WHEEL_RADIUS_M * scale;
  // The rim is 0.01 m wider than the tyre; the outer face stops inside the panel plane.
  const outerZ = BODY_HALF_WIDTH_M - PANEL_M - ((WHEEL_WIDTH_M + 0.01) * scale) / 2 + 0.06;
  return axlesX.flatMap((x) =>
    [-1, 1].flatMap((side) => [
      wheelPart(tyreGeometry, TYRE, scale, [x, radius, side * outerZ]),
      wheelPart(rimGeometry, RIM, scale, [x, radius, side * outerZ]),
    ]),
  );
}

/** A copy of a car-mesh cylinder laid along Z, scaled, coloured and moved. */
function wheelPart(
  source: BufferGeometry,
  color: string,
  scale: number,
  at: [number, number, number],
): BufferGeometry {
  const part = source.clone();
  part.rotateX(Math.PI / 2);
  part.scale(scale, scale, scale);
  part.translate(...at);
  return colour(part, color);
}

/** A box from corner `min` to corner `max`, coloured. */
function box(
  min: [number, number, number],
  max: [number, number, number],
  color: string,
): BufferGeometry {
  const [x0, y0, z0] = min;
  const [x1, y1, z1] = max;
  const part = new BoxGeometry(Math.abs(x1 - x0), Math.abs(y1 - y0), Math.abs(z1 - z0));
  part.translate((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2);
  return colour(part, color);
}

/** Gives every vertex of `geometry` one colour (hex string). */
function colour(geometry: BufferGeometry, hex: string): BufferGeometry {
  const rgb = [
    parseInt(hex.slice(1, 3), 16) / 255,
    parseInt(hex.slice(3, 5), 16) / 255,
    parseInt(hex.slice(5, 7), 16) / 255,
  ];
  const count = geometry.getAttribute('position').count;
  const colors = new Float32Array(count * 3);
  for (let vertex = 0; vertex < count; vertex += 1) {
    colors.set(rgb, vertex * 3);
  }
  geometry.setAttribute('color', new BufferAttribute(colors, 3));
  return geometry;
}

/** Merges coloured parts (boxes and cylinders share position, normal, uv and colour). */
function mergeColoured(parts: BufferGeometry[]): BufferGeometry {
  const merged = mergeGeometries(parts);
  if (!merged) {
    throw new Error('vehicle-model.ts: the vehicle parts could not be merged');
  }
  for (const part of parts) {
    part.dispose();
  }
  return merged;
}
