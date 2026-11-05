// Draws the training viewer's crowd (SPEC §11.6): one translucent car per drop, one row of
// cars per checkpoint, each car with a floating label naming its checkpoint. On the flat
// arena the rows are laid out on a grid so they do not overlap (the sim runs every car at
// the arena centre; the offset is for drawing only). On the obstacle arena the cars share
// one course, so they are drawn where they really are. Crashed cars fade out.
// Every mesh of the car (and of its wheels) is one InstancedMesh shared by the whole crowd, in a
// flying and a crashed version, so the 64 cars cost a few dozen draw calls instead of over a
// thousand. Poses are blended between the last two physics ticks so motion is smooth.
import {
  CanvasTexture,
  type Group,
  type InstancedBufferAttribute,
  InstancedMesh,
  type Material,
  Matrix4,
  Mesh,
  type Quaternion,
  type Scene,
  Sprite,
  SpriteMaterial,
  Vector3,
} from 'three';

import { buildCarBody, buildWheel } from '../render/car-mesh.ts';
import { blendPose, newBlendedPose } from '../render/pose-blend.ts';

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

/** One mesh of a car or wheel, drawn for the whole crowd by two instanced meshes. */
interface Slot {
  /** The mesh's own transform inside its car body or wheel. */
  local: Matrix4;
  flying: InstancedMesh;
  crashed: InstancedMesh;
  /** Instances written so far this frame. */
  flyingCount: number;
  crashedCount: number;
}

/** One checkpoint label and where its car sits on the grid. */
/** A row's two label looks, shared by its cars: a crashed car's label swaps to the faded one. */
interface LabelLooks {
  full: SpriteMaterial;
  faded: SpriteMaterial;
}

interface GhostCar {
  label: Sprite;
  looks: LabelLooks;
  gridOffset: Vector3;
  crashed: boolean;
}

/** The whole drawn crowd: one row of `carsPerRow` cars per checkpoint. */
export interface CrowdView {
  cars: GhostCar[];
  carsPerRow: number;
  wheelCount: number;
  bodySlots: Slot[];
  wheelSlots: Slot[];
  /** Every slot, wheels first: the order they are drawn in. */
  allSlots: Slot[];
  /** True to spread the cars on the grid; false to draw them at their true positions. */
  spreadOnGrid: boolean;
  /** Which labels show: one over every car, one over the first car of each row, or none. */
  labels: LabelMode;
}

/** Which labels a crowd shows. A crowd that shares a screen with others shows few, so the
 * sprites do not cover the cars. */
export type LabelMode = 'cars' | 'row' | 'off';

/** No offset, for cars drawn at their true positions. */
const NO_OFFSET = new Vector3();
const UNIT_SCALE = new Vector3(1, 1, 1);

/** Reused each frame so placing cars allocates nothing. */
const pose = newBlendedPose();
const placed = new Matrix4();
const instance = new Matrix4();

/** Builds one row of `carsPerRow` cars per label in `rowLabels` and adds them to `scene`, on
 * three.js layer `layer` (a camera draws them only if it has that layer enabled). */
export function createCrowdView(
  scene: Scene,
  rowLabels: string[],
  carsPerRow: number,
  wheelCount: number,
  layer = 0,
): CrowdView {
  const looks = new Map<Material, { flying: Material; crashed: Material }>();
  const carCount = rowLabels.length * carsPerRow;
  const bodySlots = slotsOf(buildCarBody(), carCount, looks, scene, layer);
  const wheelSlots = slotsOf(buildWheel(), carCount * wheelCount, looks, scene, layer);
  const cars: GhostCar[] = [];
  rowLabels.forEach((text, row) => {
    const labelTexture = new CanvasTexture(drawLabel(text));
    // One full and one faded material per row, not per car: fewer materials to bind.
    const labelLooks: LabelLooks = {
      full: new SpriteMaterial({ map: labelTexture, depthTest: false }),
      faded: new SpriteMaterial({
        map: labelTexture,
        depthTest: false,
        transparent: true,
        opacity: CRASHED_LABEL_OPACITY,
      }),
    };
    for (let column = 0; column < carsPerRow; column += 1) {
      const label = new Sprite(labelLooks.full);
      label.scale.set(LABEL_WIDTH_M, LABEL_TALL_M, 1);
      label.layers.set(layer);
      const gridOffset = new Vector3(
        (column - (carsPerRow - 1) / 2) * COLUMN_SPACING_M,
        0,
        (row - (rowLabels.length - 1) / 2) * ROW_SPACING_M,
      );
      scene.add(label);
      cars.push({ label, looks: labelLooks, gridOffset, crashed: false });
    }
  });
  const allSlots = [...wheelSlots, ...bodySlots];
  setDrawOrder(wheelSlots, bodySlots);
  return {
    cars,
    carsPerRow,
    wheelCount,
    bodySlots,
    wheelSlots,
    allSlots,
    spreadOnGrid: true,
    labels: 'cars',
  };
}

/** Starts a frame: every instanced mesh will be refilled by `updateCrowdRow`. */
export function beginCrowdFrame(view: CrowdView): void {
  for (const slot of view.allSlots) {
    slot.flyingCount = 0;
    slot.crashedCount = 0;
  }
}

/** Ends a frame: tells three.js how many instances each mesh draws and to upload them. */
export function endCrowdFrame(view: CrowdView): void {
  for (const slot of view.allSlots) {
    commit(slot.flying, slot.flyingCount);
    commit(slot.crashed, slot.crashedCount);
  }
}

function commit(mesh: InstancedMesh, count: number): void {
  mesh.count = count;
  mesh.visible = count > 0;
  if (count > 0) {
    (mesh.instanceMatrix as InstancedBufferAttribute).addUpdateRange(0, count * 16);
    mesh.instanceMatrix.needsUpdate = true;
  }
}

/** The poses of one row at the last two physics ticks. `previous` is where the cars were one
 * tick before `latest`; the picture is `blend` (0..1) of the way from one to the other.
 * `chassis` has one pose per car, `wheels` one per wheel, `outcomes` one code per car. */
export interface RowPoses {
  previousChassis: Float32Array;
  latestChassis: Float32Array;
  previousWheels: Float32Array;
  latestWheels: Float32Array;
  outcomes: Uint8Array;
}

/** Places row `row`'s cars between their last two sim poses and fades the ones that crashed.
 * Call between `beginCrowdFrame` and `endCrowdFrame`. */
export function updateCrowdRow(view: CrowdView, row: number, poses: RowPoses, blend: number): void {
  for (let index = 0; index < view.carsPerRow; index += 1) {
    const car = view.cars[row * view.carsPerRow + index];
    if (!car) {
      return;
    }
    const offset = view.spreadOnGrid ? car.gridOffset : NO_OFFSET;
    const crashed = poses.outcomes[index] === OUTCOME_CRASHED;
    blendPose(poses.previousChassis, poses.latestChassis, index, blend, pose);
    car.label.visible = view.labels === 'cars' || (view.labels === 'row' && index === 0);
    car.label.position.copy(pose.position).add(offset);
    car.label.position.y += LABEL_HEIGHT_M;
    placeSlots(view.bodySlots, pose, offset, crashed);
    for (let wheel = 0; wheel < view.wheelCount; wheel += 1) {
      const wheelIndex = index * view.wheelCount + wheel;
      blendPose(poses.previousWheels, poses.latestWheels, wheelIndex, blend, pose);
      placeSlots(view.wheelSlots, pose, offset, crashed);
    }
    if (crashed !== car.crashed) {
      car.crashed = crashed;
      car.label.material = crashed ? car.looks.faded : car.looks.full;
    }
  }
}

/** Adds one instance of every slot at `at` (plus `offset`), to the crashed or flying mesh. */
function placeSlots(
  slots: Slot[],
  at: { position: Vector3; rotation: Quaternion },
  offset: Vector3,
  crashed: boolean,
): void {
  placed.compose(at.position.add(offset), at.rotation, UNIT_SCALE);
  for (const slot of slots) {
    instance.multiplyMatrices(placed, slot.local);
    if (crashed) {
      slot.crashed.setMatrixAt(slot.crashedCount, instance);
      slot.crashedCount += 1;
    } else {
      slot.flying.setMatrixAt(slot.flyingCount, instance);
      slot.flyingCount += 1;
    }
  }
}

/** Sets each slot's render order. three.js draws see-through meshes back to front by the
 * centre of each mesh's own geometry, so the old per-mesh cars drew low parts first (dark
 * trim, then paint, then glass) when seen from above. Instanced meshes share one object
 * position, so that order is fixed here: wheels first, then lowest geometry centre first
 * (equal heights keep their build order). Crashed copies go under the flying ones. */
function setDrawOrder(wheels: Slot[], body: Slot[]): void {
  const height = (slot: Slot): number => {
    const geometry = slot.flying.geometry;
    geometry.computeBoundingSphere();
    return geometry.boundingSphere?.center.clone().applyMatrix4(slot.local).y ?? 0;
  };
  const ordered = [...wheels, ...[...body].sort((a, b) => height(a) - height(b))];
  ordered.forEach((slot, rank) => {
    slot.crashed.renderOrder = 2 * rank;
    slot.flying.renderOrder = 2 * rank + 1;
  });
}

/** The tag colours: theme.css's --cream and --ink (a canvas cannot read CSS variables cheaply). */
const LABEL_CREAM = '#f1ead8';
const LABEL_INK = '#1b1a17';
const LABEL_FONT_PX = 40;

/** A canvas with `text` on a cream ticket, for a car's label. */
function drawLabel(text: string): HTMLCanvasElement {
  const canvas = document.createElement('canvas');
  [canvas.width, canvas.height] = LABEL_CANVAS_PX;
  const context = canvas.getContext('2d');
  if (context) {
    // a cream ticket with an ink border, like the page's tiles (theme.css)
    context.fillStyle = LABEL_CREAM;
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.strokeStyle = LABEL_INK;
    context.lineWidth = 6;
    context.strokeRect(3, 3, canvas.width - 6, canvas.height - 6);
    context.fillStyle = LABEL_INK;
    context.font = `700 ${LABEL_FONT_PX}px "Barlow Condensed", "Arial Narrow", sans-serif`;
    context.textAlign = 'center';
    context.textBaseline = 'middle';
    context.fillText(text, canvas.width / 2, canvas.height / 2);
  }
  return canvas;
}

/** Turns each mesh of `template` into a slot: a flying and a crashed instanced mesh with room
 * for `capacity` instances, both added to `scene`. Crowd cars cast no shadows: the arena's
 * shadow box covers only the middle of the grid. */
function slotsOf(
  template: Group,
  capacity: number,
  looks: Map<Material, { flying: Material; crashed: Material }>,
  scene: Scene,
  layer: number,
): Slot[] {
  const slots: Slot[] = [];
  template.updateMatrixWorld(true);
  template.traverse((object) => {
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
    const make = (material: Material) => {
      const mesh = new InstancedMesh(object.geometry, material, capacity);
      mesh.castShadow = false;
      mesh.frustumCulled = false;
      mesh.layers.set(layer);
      mesh.count = 0;
      mesh.visible = false;
      scene.add(mesh);
      return mesh;
    };
    slots.push({
      local: object.matrix.clone(),
      flying: make(look.flying),
      crashed: make(look.crashed),
      flyingCount: 0,
      crashedCount: 0,
    });
  });
  return slots;
}

/** A see-through copy of `material`. */
function seeThrough(material: Material, opacity: number): Material {
  const copy = material.clone();
  copy.transparent = true;
  copy.opacity = opacity;
  copy.depthWrite = false;
  return copy;
}
