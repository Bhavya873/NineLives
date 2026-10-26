// Draws the training viewer's crowd (SPEC §11.6): one translucent car per drop, one row of
// cars per checkpoint, each car with a floating label naming its checkpoint. On the flat
// arena the rows are laid out on a grid so they do not overlap (the sim runs every car at
// the arena centre; the offset is for drawing only). On the obstacle arena the cars share
// one course, so they are drawn where they really are. Crashed cars fade out.
import {
  CanvasTexture,
  Group,
  type Material,
  Mesh,
  type Scene,
  Sprite,
  SpriteMaterial,
  Vector3,
} from 'three';

import { buildCarBody, buildWheel } from '../render/car-mesh.ts';
import { newBlendedPose, readPose } from '../render/pose-blend.ts';

/** Metres between neighbouring cars in a row. */
const COLUMN_SPACING_M = 12;
/** Metres between checkpoint rows. */
const ROW_SPACING_M = 12;
/** How see-through a car is while it flies or after it lands. */
const FLYING_OPACITY = 0.7;
/** How faint a crashed car becomes. */
const CRASHED_OPACITY = 0.12;
/** How faint a crashed car's label becomes. */
const CRASHED_LABEL_OPACITY = 0.35;
/** The `CrowdOutcome` value for a crash (crates/nl-wasm/src/crowd.rs). */
const OUTCOME_CRASHED = 2;
/** Label height above the car's centre, in m. */
const LABEL_HEIGHT_M = 3.5;
/** Label size in the world, in m (the canvas below has the same 4:1 shape). */
const LABEL_WIDTH_M = 8;
const LABEL_TALL_M = 2;
/** Label canvas size, in pixels. */
const LABEL_CANVAS_PX: [number, number] = [256, 64];

/** One mesh of a car, with the flying and crashed versions of its material. */
interface GhostPart {
  mesh: Mesh;
  flying: Material;
  crashed: Material;
}

/** One drawn car and its checkpoint label. */
interface GhostCar {
  body: Group;
  wheels: Group[];
  parts: GhostPart[];
  label: Sprite;
  gridOffset: Vector3;
  crashed: boolean;
}

/** The whole drawn crowd: one row of `carsPerRow` cars per checkpoint. */
export interface CrowdView {
  cars: GhostCar[];
  carsPerRow: number;
  /** True to spread the cars on the grid; false to draw them at their true positions. */
  spreadOnGrid: boolean;
}

/** No offset, for cars drawn at their true positions. */
const NO_OFFSET = new Vector3();

/** Reused each frame so placing cars allocates nothing. */
const pose = newBlendedPose();

/** Builds one row of `carsPerRow` cars per label in `rowLabels` and adds them to `scene`. */
export function createCrowdView(
  scene: Scene,
  rowLabels: string[],
  carsPerRow: number,
  wheelCount: number,
): CrowdView {
  const looks = new Map<Material, { flying: Material; crashed: Material }>();
  const cars: GhostCar[] = [];
  rowLabels.forEach((text, row) => {
    const labelCanvas = drawLabel(text);
    for (let column = 0; column < carsPerRow; column += 1) {
      const body = buildCarBody();
      const wheels = Array.from({ length: wheelCount }, () => buildWheel());
      // Each car gets its own material so a crashed car's label can fade alone.
      const label = new Sprite(
        new SpriteMaterial({ map: new CanvasTexture(labelCanvas), depthTest: false }),
      );
      label.scale.set(LABEL_WIDTH_M, LABEL_TALL_M, 1);
      const gridOffset = new Vector3(
        (column - (carsPerRow - 1) / 2) * COLUMN_SPACING_M,
        0,
        (row - (rowLabels.length - 1) / 2) * ROW_SPACING_M,
      );
      const parts = ghostParts([body, ...wheels], looks);
      scene.add(body, label, ...wheels);
      cars.push({ body, wheels, parts, label, gridOffset, crashed: false });
    }
  });
  return { cars, carsPerRow, spreadOnGrid: true };
}

/** Moves row `row`'s cars to their latest sim poses and fades the ones that crashed.
 * `chassis` has one pose per car, `wheels` one per wheel, `outcomes` one code per car. */
export function updateCrowdRow(
  view: CrowdView,
  row: number,
  chassis: Float32Array,
  wheels: Float32Array,
  outcomes: Uint8Array,
): void {
  for (let index = 0; index < view.carsPerRow; index += 1) {
    const car = view.cars[row * view.carsPerRow + index];
    if (!car) {
      return;
    }
    const offset = view.spreadOnGrid ? car.gridOffset : NO_OFFSET;
    placeGroup(car.body, chassis, index, offset);
    car.label.position.copy(car.body.position);
    car.label.position.y += LABEL_HEIGHT_M;
    car.wheels.forEach((wheel, wheelIndex) => {
      placeGroup(wheel, wheels, index * car.wheels.length + wheelIndex, offset);
    });
    setCrashed(car, outcomes[index] === OUTCOME_CRASHED);
  }
}

/** Switches a car between its flying and crashed looks. */
function setCrashed(car: GhostCar, crashed: boolean): void {
  if (crashed === car.crashed) {
    return;
  }
  car.crashed = crashed;
  for (const part of car.parts) {
    part.mesh.material = crashed ? part.crashed : part.flying;
  }
  car.label.material.opacity = crashed ? CRASHED_LABEL_OPACITY : 1;
}

/** Puts `group` at pose number `index` of `poses`, shifted by `offset`. */
function placeGroup(group: Group, poses: Float32Array, index: number, offset: Vector3): void {
  readPose(poses, index, pose);
  group.position.copy(pose.position).add(offset);
  group.quaternion.copy(pose.rotation);
}

/** A canvas with `text` on a dark tag, for a car's label. */
function drawLabel(text: string): HTMLCanvasElement {
  const canvas = document.createElement('canvas');
  [canvas.width, canvas.height] = LABEL_CANVAS_PX;
  const context = canvas.getContext('2d');
  if (context) {
    context.fillStyle = 'rgba(12, 17, 24, 0.75)';
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.fillStyle = '#f2f5f8';
    context.font = '600 34px system-ui, sans-serif';
    context.textAlign = 'center';
    context.textBaseline = 'middle';
    context.fillText(text, canvas.width / 2, canvas.height / 2);
  }
  return canvas;
}

/** The meshes under `groups`, each switched to a shared see-through version of its
 * material (one flying and one crashed version per original, kept in `looks`). Crowd
 * cars cast no shadows: the arena's shadow box covers only the middle of the grid. */
function ghostParts(
  groups: Group[],
  looks: Map<Material, { flying: Material; crashed: Material }>,
): GhostPart[] {
  const parts: GhostPart[] = [];
  for (const group of groups) {
    group.traverse((object) => {
      object.castShadow = false;
      if (!(object instanceof Mesh)) {
        return;
      }
      const original = object.material as Material;
      let look = looks.get(original);
      if (!look) {
        look = {
          flying: seeThrough(original, FLYING_OPACITY),
          crashed: seeThrough(original, CRASHED_OPACITY),
        };
        looks.set(original, look);
      }
      object.material = look.flying;
      parts.push({ mesh: object, flying: look.flying, crashed: look.crashed });
    });
  }
  return parts;
}

/** A see-through copy of `material`. */
function seeThrough(material: Material, opacity: number): Material {
  const copy = material.clone();
  copy.transparent = true;
  copy.opacity = opacity;
  copy.depthWrite = false;
  return copy;
}
