// The 3D scene: sky, light, ground, and the car (SPEC §11.4).
// It owns three.js objects only; it never reads input or talks to the physics.
import {
  CanvasTexture,
  Color,
  DirectionalLight,
  Fog,
  GridHelper,
  Group,
  HemisphereLight,
  Mesh,
  MeshStandardMaterial,
  PCFSoftShadowMap,
  PerspectiveCamera,
  PlaneGeometry,
  Scene,
  SRGBColorSpace,
  WebGLRenderer,
} from 'three';

import { buildCarBody, buildWheel } from './car-mesh';

const GROUND_SIZE_M = 4000;
const SKY_TOP = '#5b8fd9';
const SKY_HORIZON = '#cfe3f5';

/** Everything the renderer draws, kept together so the game can move it each frame. */
export interface GameScene {
  renderer: WebGLRenderer;
  scene: Scene;
  camera: PerspectiveCamera;
  sun: DirectionalLight;
  carBody: Group;
  wheels: Group[];
}

/** Builds the scene and attaches its canvas to `container`. */
export function createGameScene(container: HTMLElement): GameScene {
  const renderer = new WebGLRenderer({ antialias: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = PCFSoftShadowMap;
  container.appendChild(renderer.domElement);

  const scene = new Scene();
  scene.background = buildSkyGradient();
  scene.fog = new Fog(new Color(SKY_HORIZON), 150, 900);

  const camera = new PerspectiveCamera(60, 1, 0.1, 2000);
  scene.add(new HemisphereLight(0xdfefff, 0x4a5a3a, 1.2));
  const sun = buildSun();
  scene.add(sun, sun.target);
  scene.add(buildGround());

  const carBody = buildCarBody();
  const wheels = [buildWheel(), buildWheel(), buildWheel(), buildWheel()];
  scene.add(carBody, ...wheels);

  const gameScene = { renderer, scene, camera, sun, carBody, wheels };
  resizeToWindow(gameScene);
  window.addEventListener('resize', () => resizeToWindow(gameScene));
  return gameScene;
}

/** Keeps the canvas and camera matched to the window size. */
function resizeToWindow({ renderer, camera }: GameScene): void {
  renderer.setSize(window.innerWidth, window.innerHeight);
  camera.aspect = window.innerWidth / window.innerHeight;
  camera.updateProjectionMatrix();
}

/** A vertical sky gradient drawn once into a tiny canvas. */
function buildSkyGradient(): CanvasTexture {
  const canvas = document.createElement('canvas');
  canvas.width = 1;
  canvas.height = 256;
  const context = canvas.getContext('2d');
  if (context) {
    const gradient = context.createLinearGradient(0, 0, 0, canvas.height);
    gradient.addColorStop(0, SKY_TOP);
    gradient.addColorStop(1, SKY_HORIZON);
    context.fillStyle = gradient;
    context.fillRect(0, 0, canvas.width, canvas.height);
  }
  const texture = new CanvasTexture(canvas);
  texture.colorSpace = SRGBColorSpace;
  return texture;
}

/** One directional light that casts soft shadows near the car. */
function buildSun(): DirectionalLight {
  const sun = new DirectionalLight(0xffffff, 2.2);
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

/** Flat ground at y = 0 with a grid, so speed is easy to see. */
function buildGround(): Group {
  const ground = new Group();
  const plane = new Mesh(
    new PlaneGeometry(GROUND_SIZE_M, GROUND_SIZE_M),
    new MeshStandardMaterial({ color: 0x7c9a5c, flatShading: true }),
  );
  plane.rotation.x = -Math.PI / 2;
  plane.receiveShadow = true;
  ground.add(plane);

  const grid = new GridHelper(GROUND_SIZE_M, GROUND_SIZE_M / 10, 0x4f6b38, 0x5a7841);
  grid.position.y = 0.01;
  ground.add(grid);
  return ground;
}
