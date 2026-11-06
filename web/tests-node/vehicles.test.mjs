// Unit tests for the parked-vehicle models (SPEC §11.4): each hatchback, pickup, SUV and box truck fits
// inside its 4.5 x 1.9 m collider box, its flat roof spans the box top so a landing never
// looks like floating, only car_row pieces become vehicles, and a car park costs four
// meshes however many cars it has. Needs `npm run wasm` first. Plain Node.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

import { Box3, Matrix4, Mesh, Quaternion, Vector3 } from 'three';

import { buildCourseView } from '../src/render/course-view.ts';
import {
  VEHICLE_LENGTH_M,
  VEHICLE_WIDTH_M,
  buildVehicleGeometry,
  PALETTE,
  VEHICLE_COLOURS,
  paintFor,
  vehicleTypeFor,
} from '../src/render/vehicle-model.ts';
import { Game, controllersToml, modesToml, vehicleToml } from './load-wasm.mjs';

const playgroundJson = readFileSync(
  new URL('../../courses/playground.json', import.meta.url),
  'utf8',
);
const HEIGHTS_M = [1.0, 1.2, 1.4, 1.6, 1.7, 1.8, 2.0, 2.2];
const EPSILON_M = 1e-4;

/** The playground's shapes without the floor (its checker texture needs a browser canvas). */
function playgroundShapes() {
  const game = new Game(vehicleToml, modesToml, controllersToml, 0n);
  return JSON.parse(game.load_course(playgroundJson)).filter((shape) => shape.surface !== 'floor');
}

test('heights pick the vehicle: hatchback, pickup, SUV, then box truck from 1.95 m', () => {
  assert.equal(vehicleTypeFor(1.0), 'hatchback');
  assert.equal(vehicleTypeFor(1.2), 'hatchback');
  assert.equal(vehicleTypeFor(1.4), 'pickup');
  assert.equal(vehicleTypeFor(1.6), 'pickup');
  assert.equal(vehicleTypeFor(1.7), 'suv');
  assert.equal(vehicleTypeFor(1.8), 'suv');
  assert.equal(vehicleTypeFor(2.0), 'truck');
  assert.equal(vehicleTypeFor(2.2), 'truck');
});

test('the palette is five muted colours and every vertex colour comes from the vehicle colours', () => {
  assert.equal(PALETTE.length, 5);
  const seen = new Set();
  for (let index = 0; index < 10; index += 1) {
    seen.add(paintFor(index));
  }
  assert.deepEqual([...seen].sort(), [...PALETTE].sort());

  const allowed = new Set(
    VEHICLE_COLOURS.map((hex) =>
      [1, 3, 5].map((at) => Math.round((parseInt(hex.slice(at, at + 2), 16) / 255) * 255)).join(),
    ),
  );
  for (const heightM of HEIGHTS_M) {
    for (let index = 0; index < 5; index += 1) {
      const colours = buildVehicleGeometry(heightM, index).getAttribute('color');
      for (let vertex = 0; vertex < colours.count; vertex += 1) {
        const rgb = [0, 1, 2].map((c) => Math.round(colours.array[vertex * 3 + c] * 255));
        assert.ok(allowed.has(rgb.join()), `${heightM} m #${index}: colour ${rgb} is off palette`);
      }
    }
  }
});

test('every vehicle fits inside its collider and its roof spans the whole box top', () => {
  for (const heightM of HEIGHTS_M) {
    for (let index = 0; index < 4; index += 1) {
      const geometry = buildVehicleGeometry(heightM, index);
      geometry.computeBoundingBox();
      const box = geometry.boundingBox;
      const label = `${vehicleTypeFor(heightM)} ${heightM} m #${index}`;

      assert.ok(box.min.x >= -VEHICLE_LENGTH_M / 2 - EPSILON_M, `${label}: tail pokes out`);
      assert.ok(box.max.x <= VEHICLE_LENGTH_M / 2 + EPSILON_M, `${label}: nose pokes out`);
      assert.ok(box.min.z >= -VEHICLE_WIDTH_M / 2 - EPSILON_M, `${label}: left pokes out`);
      assert.ok(box.max.z <= VEHICLE_WIDTH_M / 2 + EPSILON_M, `${label}: right pokes out`);
      assert.ok(
        Math.abs(box.max.y - heightM / 2) < EPSILON_M,
        `${label}: roof is not at ${heightM}`,
      );
      assert.ok(Math.abs(box.min.y + heightM / 2) < EPSILON_M, `${label}: tyres miss the ground`);

      // The roof slab covers the box top from end to end and side to side.
      const position = geometry.getAttribute('position');
      const roof = new Box3();
      for (let vertex = 0; vertex < position.count; vertex += 1) {
        if (position.getY(vertex) > heightM / 2 - EPSILON_M) {
          roof.expandByPoint(new Vector3().fromBufferAttribute(position, vertex));
        }
      }
      assert.ok(roof.min.x <= -2.2 && roof.max.x >= 2.2, `${label}: roof is short`);
      assert.ok(roof.min.z <= -0.9 && roof.max.z >= 0.9, `${label}: roof is narrow`);
    }
  }
});

test('vehicles are the same every time they are built', () => {
  const first = buildVehicleGeometry(1.7, 3);
  const second = buildVehicleGeometry(1.7, 3);

  assert.deepEqual(first.getAttribute('position').array, second.getAttribute('position').array);
  assert.deepEqual(first.getAttribute('color').array, second.getAttribute('color').array);
  assert.equal(paintFor(2), paintFor(2));
});

test('the playground draws its car rows as vehicles in at most four meshes, walls and bumps as boxes', () => {
  const shapes = playgroundShapes();
  const pieceTypes = JSON.parse(playgroundJson).pieces.map((piece) => piece.type);
  const carShapes = shapes.filter((shape) => pieceTypes[shape.piece] === 'car_row');
  assert.ok(carShapes.length >= 30, `only ${carShapes.length} cars`);

  const group = buildCourseView(JSON.stringify(shapes), playgroundJson);
  const vehicleMeshes = group.children.filter((child) => child.name.startsWith('vehicles-'));
  assert.ok(vehicleMeshes.length >= 2 && vehicleMeshes.length <= 4);

  // Every shape but the cars still has its own mesh (gates add a few more).
  const others = shapes.length - carShapes.length;
  const plainMeshes = group.children.filter((child) => !child.name.startsWith('vehicles-'));
  assert.ok(plainMeshes.length >= others);
  assert.equal(group.children.length - vehicleMeshes.length, plainMeshes.length);

  // Without the course file, the car-sized boxes are found by their footprint, same cars.
  const guessed = buildCourseView(JSON.stringify(shapes));
  const guessedVehicles = guessed.children.filter((child) => child.name.startsWith('vehicles-'));
  assert.equal(guessedVehicles.length, vehicleMeshes.length);
  const vertices = (meshes) =>
    meshes.reduce(
      (sum, mesh) =>
        sum + mesh instanceof Mesh ? sum + mesh.geometry.getAttribute('position').count : sum,
      0,
    );
  assert.equal(vertices(guessedVehicles), vertices(vehicleMeshes));
});

test('each placed vehicle sits inside its rotated collider box in the world', () => {
  const shapes = playgroundShapes();
  const pieceTypes = JSON.parse(playgroundJson).pieces.map((piece) => piece.type);
  for (const shape of shapes) {
    if (pieceTypes[shape.piece] !== 'car_row') {
      continue;
    }
    const [hx, hy, hz] = shape.geometry.box.half_extents_m;
    const [x, y, z] = shape.pose.position_m;
    const [qx, qy, qz, qw] = shape.pose.rotation_xyzw;
    const pose = new Matrix4().compose(
      new Vector3(x, y, z),
      new Quaternion(qx, qy, qz, qw),
      new Vector3(1, 1, 1),
    );
    const vehicle = buildVehicleGeometry(2 * hy, shape.piece);
    vehicle.applyMatrix4(pose);
    vehicle.computeBoundingBox();
    // The collider's own world bounds from its eight corners.
    const collider = new Box3();
    for (const corner of [-1, 1].flatMap((sx) =>
      [-1, 1].flatMap((sy) => [-1, 1].map((sz) => new Vector3(sx * hx, sy * hy, sz * hz))),
    )) {
      collider.expandByPoint(corner.applyMatrix4(pose));
    }
    assert.ok(collider.containsBox(vehicle.boundingBox.clone().expandByScalar(-EPSILON_M)));
  }
});
