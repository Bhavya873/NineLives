// The drawn world: the 3D scene, the car's physics frames, the explosion, the screen effects
// and the course. It also holds the actions that reset the view when a run starts over.
import { prefersReducedMotion, requireElement } from '../shared/dom.ts';
import { snapCamera } from '../render/chase-camera.ts';
import { createExplosion, stopExplosion } from '../render/explosion.ts';
import { type BlendedPose, FLOATS_PER_POSE, newBlendedPose } from '../render/pose-blend.ts';
import { createGameScene, type PhysicsFrames, placeCar } from '../render/scene.ts';
import { createScreenEffects, resetScreenEffects } from '../render/screen-effects.ts';
import type { Game } from '../wasm/nl_wasm.js';
import { createCourseSession, showCourse } from './course-session.ts';

/** Everything the game draws, plus the sim it follows. */
export interface World {
  game: Game;
  /** The last two physics states, blended each frame for smooth motion. */
  frames: PhysicsFrames;
  view: ReturnType<typeof createGameScene>;
  carPose: BlendedPose;
  explosion: ReturnType<typeof createExplosion>;
  effects: ReturnType<typeof createScreenEffects>;
  course: ReturnType<typeof createCourseSession>;
}

/** Builds the scene, the physics frames and the effects around the car. */
export function createWorld(game: Game): World {
  const frames: PhysicsFrames = {
    previousChassis: game.chassis_pose(),
    latestChassis: game.chassis_pose(),
    previousWheels: game.wheel_poses(),
    latestWheels: game.wheel_poses(),
  };
  const wheelCount = frames.latestWheels.length / FLOATS_PER_POSE;
  const view = createGameScene(requireElement('game'), wheelCount);
  const explosion = createExplosion(view.scene, prefersReducedMotion());
  const carPose = newBlendedPose();
  const effects = createScreenEffects(view.scene);
  const course = createCourseSession(view.scene, view.ground);
  return { game, frames, view, carPose, explosion, effects, course };
}

/** Shows the car exactly where the sim has it, with no blending or camera easing. */
export function snapViewToGame(world: World): void {
  const { game, frames, view, carPose } = world;
  frames.previousChassis = frames.latestChassis = game.chassis_pose();
  frames.previousWheels = frames.latestWheels = game.wheel_poses();
  placeCar(view, frames, 1, carPose);
  snapCamera(view.camera, carPose);
}

/** Ends any explosion, snaps the view to the sim and clears the screen effects. */
export function resetView(world: World): void {
  stopExplosion(world.explosion);
  snapViewToGame(world);
  resetScreenEffects(world.effects);
}

/** The actions that start a run on the drawn course. */
export interface CourseControls {
  /** Puts the drawn course in step with a fresh run (null: the open arena). */
  startCourse: (shapesJson: string | null, courseJson?: string) => void;
  /** A fresh run in the open-world playground (or the flat arena with ?world=flat). */
  startFreeDrive: () => void;
}

/** Builds the course actions. The playground JSON is the open-world course for free driving. */
export function createCourseControls(
  world: World,
  params: URLSearchParams,
  playgroundJson: string,
): CourseControls {
  const startCourse = (shapesJson: string | null, courseJson?: string) => {
    showCourse(world.course, shapesJson, courseJson);
    resetView(world);
  };
  const startFreeDrive = () => {
    if (params.get('world') === 'flat') {
      world.game.load_flat_ground();
      startCourse(null);
    } else {
      startCourse(world.game.load_course(playgroundJson), playgroundJson);
    }
  };
  return { startCourse, startFreeDrive };
}

/**
 * X (or the pause card's Drop test): a drop test (SPEC §9.6 Stage A) from a fresh random
 * start, to watch a save.
 */
export function createDropTest(world: World): () => void {
  return () => {
    world.game.drop_test(BigInt(Math.floor(Math.random() * 2 ** 32)));
    // A drop during the respawn wait clears the crash without a Respawn event.
    resetView(world);
  };
}
