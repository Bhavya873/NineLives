// The 3D scene: a lab-arena look (SPEC §11.4, the owner's 2026-10-17 choice): a dark
// gradient backdrop, a blue-grey checkered floor and neutral light, like a physics
// simulator's test arena, plus the car. Three.js objects only; no input or physics.
import {
  CanvasTexture,
  Color,
  DirectionalLight,
  Fog,
  Group,
  HemisphereLight,
  Mesh,
  MeshStandardMaterial,
  NearestFilter,
  PCFSoftShadowMap,
  PerspectiveCamera,
  PlaneGeometry,
  RepeatWrapping,
  Scene,
  SRGBColorSpace,
  WebGLRenderer,
} from 'three';

import { BACKDROP_HORIZON, BACKDROP_RADIUS_M, buildBackdropDome } from './backdrop.ts';
import { buildCarBody, buildWheel } from './car-mesh.ts';
import { blendPose, type BlendedPose, newBlendedPose } from './pose-blend.ts';

const GROUND_SIZE_M = 4000;
/** The floor starts fading into the backdrop here and is pure backdrop colour by the end. */
const FOG_START_M = 120;
const FOG_END_M = 700;
/** Side of one floor square; big enough to read speed from at a glance. */
const CHECKER_SIZE_M = 2;
const CHECKER_LIGHT = '#3b5470';
const CHECKER_DARK = '#22344a';

/** The arena every view draws into (see `createArena`). */
export interface Arena {
  renderer: WebGLRenderer;
  scene: Scene;
  camera: PerspectiveCamera;
  sun: DirectionalLight;
  /** The sky dome; the game moves it with the car. */
  backdrop: Mesh;
}

/** The game's view: the arena plus the one car. */
export interface GameScene extends Arena {
  carBody: Group;
  wheels: Group[];
}

/** The last two physics states, so each frame can blend between them (SPEC §11.2). */
export interface PhysicsFrames {
  previousChassis: Float32Array;
  latestChassis: Float32Array;
  previousWheels: Float32Array;
  latestWheels: Float32Array;
}

/** Where the sun sits relative to the car, so its shadow box always covers the car. */
const SUN_OFFSET_M = { x: 30, y: 60, z: 20 };

/** Reused for each wheel so placing the car allocates nothing per frame. */
const wheelPose = newBlendedPose();

/**
 * Moves the car, wheels and sun to the pose `blend` of the way between the previous
 * and latest physics tick. Writes the car's blended pose into `carPose` for the camera.
 */
export function placeCar(
  view: GameScene,
  frames: PhysicsFrames,
  blend: number,
  carPose: BlendedPose,
): void {
  blendPose(frames.previousChassis, frames.latestChassis, 0, blend, carPose);
  view.carBody.position.copy(carPose.position);
  view.carBody.quaternion.copy(carPose.rotation);

  view.wheels.forEach((wheel, index) => {
    blendPose(frames.previousWheels, frames.latestWheels, index, blend, wheelPose);
    wheel.position.copy(wheelPose.position);
    wheel.quaternion.copy(wheelPose.rotation);
  });

  view.sun.position.copy(carPose.position).add(SUN_OFFSET_M);
  view.sun.target.position.copy(carPose.position);
  view.backdrop.position.copy(carPose.position);
}

/** Shows or hides the car and its wheels, e.g. hidden while the wreck explodes. */
export function setCarVisible(view: GameScene, visible: boolean): void {
  view.carBody.visible = visible;
  for (const wheel of view.wheels) {
    wheel.visible = visible;
  }
}

/** Builds the scene and attaches its canvas to `container`. */
export function createGameScene(container: HTMLElement, wheelCount: number): GameScene {
  const arena = createArena(container);
  const carBody = buildCarBody();
  const wheels = Array.from({ length: wheelCount }, () => buildWheel());
  arena.scene.add(carBody, ...wheels);
  return { ...arena, carBody, wheels };
}

/** The lab arena without any car (SPEC §11.4): renderer, checkered floor fading into the
 * sky dome, light with soft shadows, and a camera kept matched to the window. The game
 * and the training viewer both build on it. */
export function createArena(container: HTMLElement): Arena {
  const renderer = new WebGLRenderer({ antialias: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = PCFSoftShadowMap;
  container.appendChild(renderer.domElement);

  const scene = new Scene();
  scene.background = new Color(BACKDROP_HORIZON);
  // The floor fades into the dome's horizon colour, so the arena has no visible edge.
  scene.fog = new Fog(new Color(BACKDROP_HORIZON), FOG_START_M, FOG_END_M);
  const backdrop = buildBackdropDome();
  scene.add(backdrop);

  const camera = new PerspectiveCamera(60, 1, 0.1, BACKDROP_RADIUS_M + 100);
  // Soft fill light, kept low so the sun's shadow still reads on the dark floor.
  scene.add(new HemisphereLight(0xffffff, 0x2a3440, 0.7));
  const sun = buildSun();
  scene.add(sun, sun.target);
  scene.add(buildGround());

  const arena = { renderer, scene, camera, sun, backdrop };
  resizeToWindow(arena);
  window.addEventListener('resize', () => resizeToWindow(arena));
  return arena;
}

/** Keeps the canvas and camera matched to the window size. */
function resizeToWindow({ renderer, camera }: Arena): void {
  renderer.setSize(window.innerWidth, window.innerHeight);
  camera.aspect = window.innerWidth / window.innerHeight;
  camera.updateProjectionMatrix();
}

/** A texture drawn once by `draw` into a small canvas of `width` × `height` pixels. */
function buildCanvasTexture(
  width: number,
  height: number,
  draw: (context: CanvasRenderingContext2D) => void,
): CanvasTexture {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d');
  if (context) {
    draw(context);
  }
  const texture = new CanvasTexture(canvas);
  texture.colorSpace = SRGBColorSpace;
  return texture;
}

/** One directional light that casts soft shadows near the car. */
function buildSun(): DirectionalLight {
  const sun = new DirectionalLight(0xffffff, 2.8);
  sun.castShadow = true;
  sun.shadow.mapSize.set(2048, 2048);
  const reach = 30;
  sun.shadow.camera.left = -reach;
  sun.shadow.camera.right = reach;
  sun.shadow.camera.top = reach;
  sun.shadow.camera.bottom = -reach;
  sun.shadow.camera.far = 200;
  return sun;
}

/** Flat ground at y = 0, checkered so speed and position are easy to see. */
function buildGround(): Mesh {
  const ground = new Mesh(
    new PlaneGeometry(GROUND_SIZE_M, GROUND_SIZE_M),
    new MeshStandardMaterial({ map: buildCheckerTexture(), roughness: 0.9 }),
  );
  ground.rotation.x = -Math.PI / 2;
  ground.receiveShadow = true;
  return ground;
}

/** A 2×2 checker drawn once into a tiny canvas and tiled across the whole floor. */
function buildCheckerTexture(): CanvasTexture {
  const texture = buildCanvasTexture(2, 2, (context) => {
    context.fillStyle = CHECKER_LIGHT;
    context.fillRect(0, 0, 2, 2);
    context.fillStyle = CHECKER_DARK;
    context.fillRect(0, 0, 1, 1);
    context.fillRect(1, 1, 1, 1);
  });
  texture.wrapS = RepeatWrapping;
  texture.wrapT = RepeatWrapping;
  // Sharp square edges up close; one texture tile is two squares.
  texture.magFilter = NearestFilter;
  const tiles = GROUND_SIZE_M / (2 * CHECKER_SIZE_M);
  texture.repeat.set(tiles, tiles);
  texture.anisotropy = 8;
  return texture;
}
