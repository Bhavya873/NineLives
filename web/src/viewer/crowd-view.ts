// Draws the training viewer's crowd (SPEC §11.6): one translucent car per drop. On the
// flat arena the cars are laid out on a grid so they do not overlap: the sim runs every
// car at the arena centre, and the grid offset is for drawing only. On the obstacle arena
// they share one course, so they are drawn where they really are. Cars that crash fade
// out; cars that land stay solid.
// Every car shares the same geometry and one of two shared translucent material sets.
import { Group, type Material, Mesh, type Scene, Vector3 } from 'three';

import { buildCarBody, buildWheel } from '../render/car-mesh.ts';
import { newBlendedPose, readPose } from '../render/pose-blend.ts';

/** Metres between neighbouring cars on the grid. */
const GRID_SPACING_M = 14;
/** How see-through a car is while it flies or after it lands. */
const FLYING_OPACITY = 0.7;
/** How faint a crashed car becomes. */
const CRASHED_OPACITY = 0.12;
/** The `CrowdOutcome` value for a crash (crates/nl-wasm/src/crowd.rs). */
const OUTCOME_CRASHED = 2;

/** One mesh of a car, with the flying and crashed versions of its material. */
interface GhostPart {
  mesh: Mesh;
  flying: Material;
  crashed: Material;
}

/** One drawn car. */
interface GhostCar {
  body: Group;
  wheels: Group[];
  parts: GhostPart[];
  gridOffset: Vector3;
  crashed: boolean;
}

/** The whole drawn crowd. */
export interface CrowdView {
  cars: GhostCar[];
  /** True to spread the cars on the grid; false to draw them at their true positions. */
  spreadOnGrid: boolean;
}

/** No offset, for cars drawn at their true positions. */
const NO_OFFSET = new Vector3();

/** Reused each frame so placing cars allocates nothing. */
const pose = newBlendedPose();

/** Builds `count` translucent cars with `wheelCount` wheels each and adds them to `scene`. */
export function createCrowdView(scene: Scene, count: number, wheelCount: number): CrowdView {
  const columns = Math.ceil(Math.sqrt(count));
  const middle = (columns - 1) / 2;
  const looks = new Map<Material, { flying: Material; crashed: Material }>();
  const cars: GhostCar[] = [];
  for (let index = 0; index < count; index += 1) {
    const body = buildCarBody();
    const wheels = Array.from({ length: wheelCount }, () => buildWheel());
    const parts = ghostParts([body, ...wheels], looks);
    const column = index % columns;
    const row = Math.floor(index / columns);
    const gridOffset = new Vector3(
      (column - middle) * GRID_SPACING_M,
      0,
      (row - middle) * GRID_SPACING_M,
    );
    scene.add(body, ...wheels);
    cars.push({ body, wheels, parts, gridOffset, crashed: false });
  }
  return { cars, spreadOnGrid: true };
}

/** Moves every car to its latest sim pose (plus its grid offset when spread on the grid)
 * and fades the ones that crashed. `chassis` has one pose per car, `wheels` one per
 * wheel, `outcomes` one code per car. */
export function updateCrowdView(
  view: CrowdView,
  chassis: Float32Array,
  wheels: Float32Array,
  outcomes: Uint8Array,
): void {
  view.cars.forEach((car, index) => {
    const offset = view.spreadOnGrid ? car.gridOffset : NO_OFFSET;
    placeGroup(car.body, chassis, index, offset);
    car.wheels.forEach((wheel, wheelIndex) => {
      placeGroup(wheel, wheels, index * car.wheels.length + wheelIndex, offset);
    });
    const crashed = outcomes[index] === OUTCOME_CRASHED;
    if (crashed !== car.crashed) {
      car.crashed = crashed;
      for (const part of car.parts) {
        part.mesh.material = crashed ? part.crashed : part.flying;
      }
    }
  });
}

/** Puts `group` at pose number `index` of `poses`, shifted by `offset`. */
function placeGroup(group: Group, poses: Float32Array, index: number, offset: Vector3): void {
  readPose(poses, index, pose);
  group.position.copy(pose.position).add(offset);
  group.quaternion.copy(pose.rotation);
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
