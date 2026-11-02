// Draws the training viewer's arena (SPEC §11.4, §11.6): the endless checkered floor for
// the flat arena, or the obstacle arena's course meshes (built by the game's course
// renderer from the Crowd's shape list) with the endless floor hidden, as the game does
// on a course. Also aims the camera so the whole crowd is in view.
import type { Group, Mesh, PerspectiveCamera, Scene } from 'three';

import { swapCourseView } from '../render/course-view.ts';
import type { ArenaKind } from './arena-choice.ts';

/** Camera spots, in m: high and far back for the flat arena's 64-car grid (about 100 m
 * across), closer for the obstacle arena, where every car drops near the centre. The
 * courses start at the origin and run along +X; the camera follows the cars from there. */
const CAMERA_SPOT_M: Record<ArenaKind, [number, number, number]> = {
  flat: [0, 70, 105],
  side: [0, 70, 105],
  obstacles: [0, 38, 54],
  driving: [-18, 9, 22],
  flight: [-18, 9, 22],
  gauntlet: [-18, 9, 22],
};
/** The camera trails the cars' centre by this much, in m (behind, above, to the side). */
const FOLLOW_OFFSET_M: [number, number, number] = [-18, 9, 22];
/** It looks this far ahead of the cars' centre, in m. */
const FOLLOW_LOOK_AHEAD_M = 8;
/** Both cameras look at this point just above the arena centre, in m. */
const CAMERA_TARGET_M: [number, number, number] = [0, 2, 0];

/** What is drawn now: which arena and, for a course, its meshes. */
export interface ArenaView {
  scene: Scene;
  camera: PerspectiveCamera;
  /** The arena's endless floor, hidden while a course (with its own floor) is up. */
  ground: Mesh;
  /** The arena drawn now, or null before the first crowd. */
  kind: ArenaKind | null;
  /** The course's meshes, or null on the flat arena. */
  course: Group | null;
  /** How many course shapes are drawn (0 on the flat arena), for tests. */
  shapeCount: number;
}

/** Nothing drawn yet; `showArena` draws the first arena. */
export function createArenaView(scene: Scene, camera: PerspectiveCamera, ground: Mesh): ArenaView {
  return { scene, camera, ground, kind: null, course: null, shapeCount: 0 };
}

/** Draws arena `kind` from `shapesJson` (a Crowd's `course_shapes()`, undefined on the
 * flat arena) unless it is already up. */
export function showArena(view: ArenaView, kind: ArenaKind, shapesJson: string | undefined): void {
  if (view.kind === kind) {
    return;
  }
  view.course = swapCourseView(view.scene, view.ground, view.course, shapesJson ?? null);
  view.shapeCount = view.course?.children.length ?? 0;
  view.camera.position.set(...CAMERA_SPOT_M[kind]);
  view.camera.lookAt(...CAMERA_TARGET_M);
  view.kind = kind;
}

/** Aims the camera at the cars' centre `(x, z)`, in m, for the arenas with a long course. */
export function followCars(view: ArenaView, x: number, z: number): void {
  const [dx, dy, dz] = FOLLOW_OFFSET_M;
  view.camera.position.set(x + dx, dy, z + dz);
  view.camera.lookAt(x + FOLLOW_LOOK_AHEAD_M, 1, z);
}
