// The course on screen (SPEC §7.11): which course meshes are drawn. The wasm Game owns
// the run itself; this only swaps meshes to match it.
import type { Group, Mesh, Scene } from 'three';

import { swapCourseView } from '../render/course-view.ts';

/** The drawn course. */
interface CourseSession {
  scene: Scene;
  /** The arena's endless floor, hidden while a course (with its own floor) is up. */
  ground: Mesh;
  /** The course's meshes, or null in the open arena. */
  view: Group | null;
}

/** Starts in the open arena. */
export function createCourseSession(scene: Scene, ground: Mesh): CourseSession {
  return { scene, ground, view: null };
}

/** Draws the course whose shapes `Game.load_course` returned, or the open arena for null. */
export function showCourse(session: CourseSession, shapesJson: string | null): void {
  session.view = swapCourseView(session.scene, session.ground, session.view, shapesJson);
}
