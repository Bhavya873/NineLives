// The crash explosion (SPEC §7.9, §11.4): a flash, a shockwave ring and about 20 low-poly
// debris pieces thrown from where the car crashed, plus a short camera shake.
// Purely visual: the sim has already frozen the wreck; this only draws over it until the
// respawn. Debris directions use Math.random because they never affect the physics.
import {
  DoubleSide,
  Group,
  Mesh,
  MeshBasicMaterial,
  MeshStandardMaterial,
  PointLight,
  RingGeometry,
  type Scene,
  TetrahedronGeometry,
  Vector3,
} from 'three';

/** How long the effect plays, in seconds (about the sim's respawn delay). */
const DURATION_S = 1.5;
const DEBRIS_COUNT = 20;
/** Fewer pieces when the player asks for reduced motion (SPEC §11.4). */
const DEBRIS_COUNT_REDUCED_MOTION = 8;
const DEBRIS_SIZE_M = 0.25;
/** Debris launch speed range, in m/s. */
const DEBRIS_MIN_SPEED_M_S = 4;
const DEBRIS_MAX_SPEED_M_S = 11;
const DEBRIS_GRAVITY_M_S2 = 9.81;
/** Debris that hits the ground keeps this much of its speed. */
const DEBRIS_BOUNCE = 0.3;
const FLASH_S = 0.25;
const FLASH_INTENSITY = 400;
const RING_MAX_RADIUS_M = 9;
const RING_S = 0.6;
const SHAKE_S = 0.5;
const SHAKE_AMPLITUDE_M = 0.35;

/** One piece of debris and how it moves. */
interface Debris {
  mesh: Mesh;
  velocityMS: Vector3;
  spinRadS: Vector3;
}

/** The explosion's three.js objects and how far through the effect it is. */
export interface Explosion {
  group: Group;
  flash: PointLight;
  ring: Mesh<RingGeometry, MeshBasicMaterial>;
  debris: Debris[];
  /** Seconds since `startExplosion`; the effect is over at `DURATION_S`. */
  ageS: number;
  reducedMotion: boolean;
}

/** Builds the (hidden) explosion and adds it to `scene`. */
export function createExplosion(scene: Scene, reducedMotion: boolean): Explosion {
  const group = new Group();
  group.visible = false;
  const flash = new PointLight(0xffd29a, 0, 30);
  const ring = new Mesh(
    new RingGeometry(0.9, 1, 48),
    new MeshBasicMaterial({ color: 0xfff1d6, transparent: true, side: DoubleSide }),
  );
  ring.rotation.x = -Math.PI / 2;
  const debris = buildDebris(reducedMotion ? DEBRIS_COUNT_REDUCED_MOTION : DEBRIS_COUNT);
  group.add(flash, ring, ...debris.map((piece) => piece.mesh));
  scene.add(group);
  return { group, flash, ring, debris, ageS: DURATION_S, reducedMotion };
}

/** Starts the effect at `position` (where the car crashed). */
export function startExplosion(explosion: Explosion, position: Vector3): void {
  explosion.group.position.copy(position);
  explosion.group.visible = true;
  explosion.ageS = 0;
  for (const piece of explosion.debris) {
    piece.mesh.position.set(0, 0, 0);
    randomUpwardDirection(piece.velocityMS);
    piece.velocityMS.multiplyScalar(randomBetween(DEBRIS_MIN_SPEED_M_S, DEBRIS_MAX_SPEED_M_S));
    piece.spinRadS.set(randomBetween(-8, 8), randomBetween(-8, 8), randomBetween(-8, 8));
  }
}

/** True while the effect is playing. */
export function isExplosionPlaying(explosion: Explosion): boolean {
  return explosion.ageS < DURATION_S;
}

/** Advances the effect by `frameS` seconds; hides it once it is over. */
export function updateExplosion(explosion: Explosion, frameS: number): void {
  if (!isExplosionPlaying(explosion)) {
    explosion.group.visible = false;
    return;
  }
  explosion.ageS += frameS;
  const age = explosion.ageS;
  explosion.flash.intensity = FLASH_INTENSITY * Math.max(0, 1 - age / FLASH_S);
  const ringProgress = Math.min(1, age / RING_S);
  explosion.ring.scale.setScalar(0.1 + ringProgress * RING_MAX_RADIUS_M);
  explosion.ring.material.opacity = 1 - ringProgress;
  // The ground (world y = 0) in the group's frame; none if the car fell off the world.
  const groundY = explosion.group.position.y >= 0 ? -explosion.group.position.y : -Infinity;
  for (const piece of explosion.debris) {
    moveDebris(piece, frameS, groundY);
  }
}

/**
 * Writes the camera shake for this frame into `out` (zero when the shake is over or the
 * player asked for reduced motion) and returns it.
 */
export function cameraShake(explosion: Explosion, out: Vector3): Vector3 {
  out.set(0, 0, 0);
  if (explosion.reducedMotion || explosion.ageS >= SHAKE_S) {
    return out;
  }
  const strength = SHAKE_AMPLITUDE_M * (1 - explosion.ageS / SHAKE_S);
  return out
    .set(randomBetween(-1, 1), randomBetween(-1, 1), randomBetween(-1, 1))
    .multiplyScalar(strength);
}

/** Small dark tetrahedra: the pieces of the car. */
function buildDebris(count: number): Debris[] {
  const geometry = new TetrahedronGeometry(DEBRIS_SIZE_M);
  const material = new MeshStandardMaterial({ color: 0x2b2f36, flatShading: true });
  const debris: Debris[] = [];
  for (let index = 0; index < count; index += 1) {
    const mesh = new Mesh(geometry, material);
    mesh.castShadow = true;
    debris.push({ mesh, velocityMS: new Vector3(), spinRadS: new Vector3() });
  }
  return debris;
}

/** Simple ballistic flight with a dull bounce off the ground at `groundY` (group space). */
function moveDebris(piece: Debris, frameS: number, groundY: number): void {
  piece.velocityMS.y -= DEBRIS_GRAVITY_M_S2 * frameS;
  piece.mesh.position.addScaledVector(piece.velocityMS, frameS);
  piece.mesh.rotation.x += piece.spinRadS.x * frameS;
  piece.mesh.rotation.y += piece.spinRadS.y * frameS;
  piece.mesh.rotation.z += piece.spinRadS.z * frameS;
  const restingY = groundY + DEBRIS_SIZE_M / 2;
  if (piece.mesh.position.y < restingY) {
    piece.mesh.position.y = restingY;
    piece.velocityMS.multiplyScalar(DEBRIS_BOUNCE);
    piece.velocityMS.y = Math.abs(piece.velocityMS.y);
    piece.spinRadS.multiplyScalar(DEBRIS_BOUNCE);
  }
}

/** A random unit vector in the upper half, so debris flies up and out. */
function randomUpwardDirection(out: Vector3): void {
  const angleRad = randomBetween(0, 2 * Math.PI);
  const upward = randomBetween(0.3, 1);
  const sideways = Math.sqrt(1 - upward * upward);
  out.set(Math.cos(angleRad) * sideways, upward, Math.sin(angleRad) * sideways);
}

function randomBetween(low: number, high: number): number {
  return low + Math.random() * (high - low);
}
