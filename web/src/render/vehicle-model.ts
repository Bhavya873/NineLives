// Parked vehicles for the course's car rows (SPEC §11.4): a plain, generic sedan, SUV or
// box truck drawn inside each 4.5 x 1.9 m collider box, so what you land on is what you
// see. Every roof is one flat slab spanning the whole box top, and there are no sloped
// hoods, so a car that comes down anywhere on the box never looks like it floats.
// Parts are boxes and the car's own tyres and rims (car-mesh.ts) with vertex colours;
// all vehicles of one type in a course merge into one mesh, so a car park costs three
// draw calls however many cars it holds.
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

export type VehicleType = 'sedan' | 'suv' | 'truck';

/** Heights up to this are sedans, from `TRUCK_MIN_HEIGHT_M` up box trucks, between SUVs. */
const SEDAN_MAX_HEIGHT_M = 1.5;
const TRUCK_MIN_HEIGHT_M = 1.9;

const HALF_LENGTH_M = VEHICLE_LENGTH_M / 2;
const HALF_WIDTH_M = VEHICLE_WIDTH_M / 2;
/** The roof slab's thickness. */
const ROOF_M = 0.1;

/** Colours of the parts that do not change with the paint. */
const GLASS = '#33485c';
const TRIM = '#202327';
const HEADLIGHT = '#fff3c4';
const TAILLIGHT = '#d02818';
const TYRE = '#1b1b1b';
const RIM = '#8a9099';
const STRIPE = '#c8553d';

/** Muted body paints; a vehicle takes one by its position in the course. */
const PAINTS: Record<VehicleType, string[]> = {
  sedan: ['#b8402f', '#2f6aa8', '#d9b240', '#7d8a8c', '#e9e6df', '#3b8a5b', '#7a4e9a', '#cf7b2e'],
  suv: ['#2d3a49', '#657a58', '#b3b3aa', '#80302e', '#3a4f66', '#c5bd9f'],
  truck: ['#f1eee6', '#d9d3c4', '#c5d3dc', '#e5d6a4'],
};
/** Cab colours of the box trucks. */
const CABS = ['#c8553d', '#2f6fa3', '#3d8a5a', '#d79a2b'];

/** The vehicle drawn in a collider `heightM` tall. */
export function vehicleTypeFor(heightM: number): VehicleType {
  if (heightM <= SEDAN_MAX_HEIGHT_M) {
    return 'sedan';
  }
  return heightM >= TRUCK_MIN_HEIGHT_M ? 'truck' : 'suv';
}

/** True for a box shaped like a parked car (a car row's 4.5 x 1.9 m footprint). */
export function isParkedCarBox(halfExtentsM: number[]): boolean {
  const [hx = 0, , hz = 0] = halfExtentsM;
  return Math.abs(hx - HALF_LENGTH_M) < 1e-3 && Math.abs(hz - HALF_WIDTH_M) < 1e-3;
}

/** The deterministic paint of the vehicle numbered `index`. */
export function paintFor(type: VehicleType, index: number): string {
  const paints = PAINTS[type];
  return paints[(index * 5) % paints.length] ?? '#cccccc';
}

/**
 * One vehicle as a single vertex-coloured geometry in its collider's own frame: centred
 * on the box, front towards +X, `heightM` tall. `index` picks the paint.
 */
export function buildVehicleGeometry(heightM: number, index: number): BufferGeometry {
  const type = vehicleTypeFor(heightM);
  const paint = paintFor(type, index);
  const parts =
    type === 'sedan'
      ? sedanParts(heightM, paint)
      : type === 'suv'
        ? suvParts(heightM, paint)
        : truckParts(heightM, paint, CABS[index % CABS.length] ?? STRIPE);
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
  return new MeshStandardMaterial({ vertexColors: true, flatShading: true, roughness: 0.65 });
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

/** A sedan: a low body, a glass band under one flat roof, two light clusters. */
function sedanParts(heightM: number, paint: string): BufferGeometry[] {
  const belt = Math.min(Math.max(0.5 * heightM, 0.7), 0.8);
  return [
    ...passengerBody(heightM, belt, paint, 0.9),
    ...wheelSet([1.4, -1.4], 1),
    ...lights(0.5, belt - 0.14, 0.12),
  ];
}

/** An SUV or van: taller, with roof rails, a window pillar and a bigger wheel. */
function suvParts(heightM: number, paint: string): BufferGeometry[] {
  const belt = 0.46 * heightM;
  const rails = [-1, 1].map((side) =>
    box([-1.9, heightM - 0.05, side * 0.68 - 0.04], [1.9, heightM, side * 0.68 + 0.04], TRIM),
  );
  const pillar = box([-1.25, belt, -0.91], [-1.1, heightM - ROOF_M, 0.91], paint);
  return [
    ...passengerBody(heightM, belt, paint, 0.9),
    ...rails,
    pillar,
    ...wheelSet([1.45, -1.45], 1.15),
    ...lights(0.55, belt - 0.16, 0.16),
  ];
}

/** A cab-over box truck: cab and cargo box share one full-length roof. */
function truckParts(heightM: number, paint: string, cab: string): BufferGeometry[] {
  const half = 0.88;
  const floor = 0.5;
  const cabX = 0.6;
  const windowBottom = 0.55 * heightM;
  return [
    // Frame, cargo box with a stripe and rear door seam, and the cab.
    box([-2.2, 0.3, -0.7], [2.2, floor, 0.7], TRIM),
    box([-2.2, floor, -half], [cabX, heightM - ROOF_M, half], paint),
    box([-2.2, 0.5 * heightM, -half - 0.02], [cabX, 0.5 * heightM + 0.22, half + 0.02], STRIPE),
    box([-2.25, floor + 0.1, -0.02], [-2.19, heightM - ROOF_M - 0.1, 0.02], TRIM),
    box([cabX, floor, -half], [2.2, heightM - ROOF_M, half], cab),
    // Glass: windscreen and a side window on each side of the cab.
    box([2.2, windowBottom, -0.78], [2.25, heightM - ROOF_M - 0.08, 0.78], GLASS),
    ...[-1, 1].map((side) =>
      box(
        [0.95, windowBottom, side > 0 ? half : -half - 0.03],
        [2.0, heightM - ROOF_M - 0.08, side > 0 ? half + 0.03 : -half],
        GLASS,
      ),
    ),
    // One flat roof over everything.
    box([-HALF_LENGTH_M, heightM - ROOF_M, -0.93], [HALF_LENGTH_M, heightM, 0.93], paint),
    box([2.2, 0.38, -0.8], [HALF_LENGTH_M, 0.62, 0.8], TRIM),
    ...[-1, 1].map((side) =>
      box([2.2, 0.62, side * 0.62 - 0.14], [2.25, 0.84, side * 0.62 + 0.14], HEADLIGHT),
    ),
    ...wheelSet([1.5, -1.0, -1.85], 1.2),
  ];
}

/** Lower body, glass band, pillars and one full-length roof slab: the sedan and SUV shell. */
function passengerBody(
  heightM: number,
  belt: number,
  paint: string,
  half: number,
): BufferGeometry[] {
  const roofBottom = heightM - ROOF_M;
  const cabinX = 2.05;
  return [
    box([-2.15, 0.22, -half], [2.15, belt, half], paint),
    box([2.15, 0.22, -half + 0.05], [HALF_LENGTH_M, belt - 0.12, half - 0.05], TRIM),
    box([-HALF_LENGTH_M, 0.22, -half + 0.05], [-2.15, belt - 0.12, half - 0.05], TRIM),
    box([-cabinX, belt, -half + 0.03], [cabinX, roofBottom, half - 0.03], GLASS),
    // Corner pillars and a middle one cover the glass band's ends.
    ...[-1, 1].flatMap((end) =>
      [-1, 1].map((side) =>
        box(
          [end > 0 ? cabinX - 0.1 : -cabinX, belt, side > 0 ? half - 0.08 : -half],
          [end > 0 ? cabinX : -cabinX + 0.1, roofBottom, side > 0 ? half : -half + 0.08],
          paint,
        ),
      ),
    ),
    box([-0.1, belt, -half], [0.1, roofBottom, half], paint),
    box([-HALF_LENGTH_M, roofBottom, -0.93], [HALF_LENGTH_M, heightM, 0.93], paint),
  ];
}

/** Head and tail light boxes on the bumper ends, `reach` m either side of the middle. */
function lights(reach: number, bottom: number, size: number): BufferGeometry[] {
  return [-1, 1].flatMap((side) => [
    box(
      [2.12, bottom, side * reach - 0.2],
      [HALF_LENGTH_M, bottom + size, side * reach + 0.2],
      HEADLIGHT,
    ),
    box(
      [-HALF_LENGTH_M, bottom, side * reach - 0.2],
      [-2.12, bottom + size, side * reach + 0.2],
      TAILLIGHT,
    ),
  ]);
}

/** Wheels at each x, on both sides, scaled by `scale` and flush with the box sides. */
function wheelSet(axlesX: number[], scale: number): BufferGeometry[] {
  const radius = WHEEL_RADIUS_M * scale;
  // The rim is 0.01 m wider than the tyre; the whole wheel stays inside the box.
  const outerZ = HALF_WIDTH_M - ((WHEEL_WIDTH_M + 0.01) * scale) / 2;
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
