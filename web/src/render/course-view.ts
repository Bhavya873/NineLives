// Draws a course (SPEC §7.11, §11.4) from the shape list `Game.load_course` returns:
// the same shapes the sim built its colliders from, so what you see is what you hit.
// Plain, flat-shaded colours like a physics test arena; three.js objects only.
import {
  BoxGeometry,
  BufferGeometry,
  DoubleSide,
  Group,
  Material,
  Mesh,
  MeshStandardMaterial,
  type Object3D,
  PlaneGeometry,
  type Scene,
  Vector3,
} from 'three';
import { ConvexGeometry } from 'three/examples/jsm/geometries/ConvexGeometry.js';

import { buildCheckerMaterial } from './scene.ts';

/** What a shape is for, as `nl_sim::course_build::Surface` names it. */
type Surface =
  'floor' | 'ramp' | 'obstacle' | 'landing_pad' | 'kick_plate' | 'checkpoint' | 'finish';

/** One shape as the wasm `Game.load_course` JSON gives it. */
interface CourseShape {
  surface: Surface;
  geometry: { box: { half_extents_m: number[] } } | { hull: { points_m: number[][] } };
  pose: { position_m: number[]; rotation_xyzw: number[] };
  solid: boolean;
}

/** Plain colours by surface (SPEC §11.4); markings are see-through. */
const SURFACE_LOOK: Record<Exclude<Surface, 'floor'>, { color: string; opacity: number }> = {
  ramp: { color: '#e8a33d', opacity: 1 },
  obstacle: { color: '#c9d3de', opacity: 1 },
  landing_pad: { color: '#4fbf7a', opacity: 0.6 },
  kick_plate: { color: '#d46ad8', opacity: 0.75 },
  checkpoint: { color: '#5ad1e8', opacity: 0.8 },
  finish: { color: '#ffffff', opacity: 0.9 },
};

/** A see-through banner above each checkpoint and finish line, so they show from afar. */
const GATE_HEIGHT_M = 4;
const GATE_OPACITY = 0.18;

/** Builds every shape of a course into one group, ready to add to the scene. */
export function buildCourseView(shapesJson: string): Group {
  const shapes = JSON.parse(shapesJson) as CourseShape[];
  // One material per look, shared by every mesh that has it (floors each get their
  // own, since their checker repeat depends on their size).
  const materials = new Map<string, Material>();
  const group = new Group();
  for (const shape of shapes) {
    const mesh = buildShapeMesh(shape, materials);
    group.add(mesh);
    if (shape.surface === 'checkpoint' || shape.surface === 'finish') {
      group.add(buildGate(shape, mesh, materials));
    }
  }
  return group;
}

/** The material stored under `key`, built by `build` the first time it is asked for. */
function sharedMaterial(
  materials: Map<string, Material>,
  key: string,
  build: () => Material,
): Material {
  let material = materials.get(key);
  if (!material) {
    material = build();
    materials.set(key, material);
  }
  return material;
}

/**
 * Replaces the course drawn in `scene`: removes and frees `current` (if any) and draws
 * the course in `shapesJson` (none for null). The arena's endless `ground` shows only
 * while no course is up, since a course brings its own floor with holes at its gaps.
 * Returns the new course group, or null.
 */
export function swapCourseView(
  scene: Scene,
  ground: Object3D,
  current: Group | null,
  shapesJson: string | null,
): Group | null {
  if (current) {
    scene.remove(current);
    disposeCourseView(current);
  }
  const next = shapesJson === null ? null : buildCourseView(shapesJson);
  if (next) {
    scene.add(next);
  }
  ground.visible = next === null;
  return next;
}

/** Frees the GPU memory of a course built by `buildCourseView` (disposing a shared
 * material more than once is harmless). */
export function disposeCourseView(group: Group): void {
  group.traverse((object) => {
    if (object instanceof Mesh) {
      (object.geometry as BufferGeometry).dispose();
      const material = object.material as Material & { map?: { dispose(): void } | null };
      material.map?.dispose();
      material.dispose();
    }
  });
}

/** One shape as a mesh at its pose. */
function buildShapeMesh(shape: CourseShape, materials: Map<string, Material>): Mesh {
  const geometry = buildGeometry(shape);
  const mesh = new Mesh(geometry, buildMaterial(shape, materials));
  const [x = 0, y = 0, z = 0] = shape.pose.position_m;
  const [qx = 0, qy = 0, qz = 0, qw = 1] = shape.pose.rotation_xyzw;
  mesh.position.set(x, y, z);
  mesh.quaternion.set(qx, qy, qz, qw);
  mesh.castShadow = shape.solid && shape.surface !== 'floor';
  mesh.receiveShadow = true;
  return mesh;
}

/** A box or a convex hull, in the shape's own frame. */
function buildGeometry(shape: CourseShape): BufferGeometry {
  if ('box' in shape.geometry) {
    const [hx = 0, hy = 0, hz = 0] = shape.geometry.box.half_extents_m;
    return new BoxGeometry(2 * hx, 2 * hy, 2 * hz);
  }
  const points = shape.geometry.hull.points_m.map(([x = 0, y = 0, z = 0]) => new Vector3(x, y, z));
  return new ConvexGeometry(points);
}

/** The checkered floor for floor boxes, a plain flat-shaded colour for the rest. */
function buildMaterial(shape: CourseShape, materials: Map<string, Material>): Material {
  if (shape.surface === 'floor' && 'box' in shape.geometry) {
    const [hx = 0, , hz = 0] = shape.geometry.box.half_extents_m;
    return buildCheckerMaterial(2 * hx, 2 * hz);
  }
  const surface = shape.surface === 'floor' ? 'obstacle' : shape.surface;
  const look = SURFACE_LOOK[surface];
  return sharedMaterial(
    materials,
    surface,
    () =>
      new MeshStandardMaterial({
        color: look.color,
        flatShading: true,
        roughness: 0.8,
        transparent: look.opacity < 1,
        opacity: look.opacity,
      }),
  );
}

/** A see-through banner standing on a line, as wide as the line and `GATE_HEIGHT_M` tall. */
function buildGate(shape: CourseShape, line: Mesh, materials: Map<string, Material>): Mesh {
  const halfWidthM = 'box' in shape.geometry ? (shape.geometry.box.half_extents_m[2] ?? 0) : 0;
  const kind = shape.surface === 'finish' ? 'finish' : 'checkpoint';
  const material = sharedMaterial(
    materials,
    `${kind}-gate`,
    () =>
      new MeshStandardMaterial({
        color: SURFACE_LOOK[kind].color,
        transparent: true,
        opacity: GATE_OPACITY,
        side: DoubleSide,
        depthWrite: false,
      }),
  );
  const gate = new Mesh(new PlaneGeometry(2 * halfWidthM, GATE_HEIGHT_M), material);
  // The plane faces local +Z; turn it to face along the course (local +X).
  gate.position.copy(line.position);
  gate.quaternion.copy(line.quaternion);
  gate.rotateY(Math.PI / 2);
  gate.translateY(GATE_HEIGHT_M / 2);
  return gate;
}
