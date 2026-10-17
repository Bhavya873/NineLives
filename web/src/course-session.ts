// The course on screen (SPEC §7.11, §11.6): which course is drawn and whether its
// results screen is up. The wasm Game owns the run itself; this only swaps meshes and
// panels to match it.
import type { Group, Mesh, Scene } from 'three';

import type { HudValues } from './hud/hud.ts';
import { hideResults, type ResultsScreen, showResults } from './hud/results.ts';
import { buildCourseView, disposeCourseView } from './render/course-view.ts';

/** The drawn course and the results screen. */
export interface CourseSession {
  scene: Scene;
  /** The arena's endless floor, hidden while a course (with its own floor) is up. */
  ground: Mesh;
  results: ResultsScreen;
  /** The course's meshes, or null in the open arena. */
  view: Group | null;
  resultsShown: boolean;
}

/** Starts in the open arena with no results showing. */
export function createCourseSession(
  scene: Scene,
  ground: Mesh,
  results: ResultsScreen,
): CourseSession {
  return { scene, ground, results, view: null, resultsShown: false };
}

/**
 * Draws the course whose shapes `Game.load_course` returned, or the open arena for
 * null, and hides any results from the last run.
 */
export function showCourse(session: CourseSession, shapesJson: string | null): void {
  if (session.view) {
    session.scene.remove(session.view);
    disposeCourseView(session.view);
    session.view = null;
  }
  if (shapesJson !== null) {
    session.view = buildCourseView(shapesJson);
    session.scene.add(session.view);
  }
  // A course brings its own floor, with holes where the gaps are.
  session.ground.visible = shapesJson === null;
  hideResults(session.results);
  session.resultsShown = false;
}

/** Shows the results the first time the run is finished; `values` are frozen there. */
export function showResultsWhenFinished(session: CourseSession, values: HudValues): void {
  if (values.finished && !session.resultsShown) {
    showResults(session.results, values);
    session.resultsShown = true;
  }
}
