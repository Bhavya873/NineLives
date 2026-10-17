// The crash explosion (SPEC §7.9, §11.4): a flash, a shockwave ring and about 20 low-poly
// debris pieces thrown from where the car crashed, plus a short camera shake.
// Purely visual: the sim has already frozen the wreck; this only draws over it until the
// respawn, when `stopExplosion` ends it. Debris directions use Math.random because they
// never affect the physics.
import {
  DoubleSide,
  Group,
  MathUtils,
  Mesh,
  MeshBasicMaterial,
  MeshStandardMaterial,
  PointLight,
  RingGeometry,
  type Scene,
  TetrahedronGeometry,
  Vector3,
} from 'three';

const DEBRIS_COUNT = 20;
/** Fewer pieces when the player asks for reduced motion (SPEC §11.4). */
const DEBRIS_COUNT_REDUCED_MOTION = 8;
const DEBRIS_SIZE_M = 0.25;
/** Debris launch speed range, in m/s. */
const DEBRIS_MIN_SPEED_M_S = 4;
const DEBRIS_MAX_SPEED_M_S = 11;
/** Largest debris spin about each axis, in rad/s. */
const DEBRIS_MAX_SPIN_RAD_S = 8;
const DEBRIS_GRAVITY_M_S2 = 9.81;
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
  /** The ring and debris; visible exactly while the effect plays. */
  group: Group;
  /**
   * Kept in the scene at all times (dark when idle): adding or hiding a light changes
   * the scene's light count, which makes three.js recompile every material mid-crash.
   */
  flash: PointLight;
  ring: Mesh<RingGeometry, MeshBasicMaterial>;
  debris: Debris[];
  /** Seconds since `startExplosion`. */
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
  group.add(ring, ...debris.map((piece) => piece.mesh));
  scene.add(group, flash);
  return { group, flash, ring, debris, ageS: 0, reducedMotion };
}

/** Starts the effect at `position` (where the car crashed). */
export function startExplosion(explosion: Explosion, position: Vector3): void {
  explosion.group.position.copy(position);
  explosion.flash.position.copy(position);
  explosion.group.visible = true;
  explosion.ageS = 0;
  const spinSpread = 2 * DEBRIS_MAX_SPIN_RAD_S;
  for (const piece of explosion.debris) {
    piece.mesh.position.set(0, 0, 0);
    randomUpwardDirection(piece.velocityMS);
    const speedMS = MathUtils.randFloat(DEBRIS_MIN_SPEED_M_S, DEBRIS_MAX_SPEED_M_S);
    piece.velocityMS.multiplyScalar(speedMS);
    piece.spinRadS.set(
      MathUtils.randFloatSpread(spinSpread),
      MathUtils.randFloatSpread(spinSpread),
      MathUtils.randFloatSpread(spinSpread),
    );
  }
}

/** Ends the effect at once, e.g. when the car respawns: hides it and stops the shake. */
export function stopExplosion(explosion: Explosion): void {
  explosion.group.visible = false;
  explosion.flash.intensity = 0;
}

/** True from `startExplosion` until `stopExplosion`. */
export function isExplosionPlaying(explosion: Explosion): boolean {
  return explosion.group.visible;
}

/**
 * Advances the effect by `frameS` seconds. The flash, ring and shake fade within a
 * second; the debris keeps falling (the ground hides it) until `stopExplosion`.
 */
export function updateExplosion(explosion: Explosion, frameS: number): void {
  if (!isExplosionPlaying(explosion)) {
    return;
  }
  explosion.ageS += frameS;
  const age = explosion.ageS;
  explosion.flash.intensity = FLASH_INTENSITY * MathUtils.clamp(1 - age / FLASH_S, 0, 1);
  const ringProgress = MathUtils.clamp(age / RING_S, 0, 1);
  explosion.ring.scale.setScalar(MathUtils.lerp(0.1, RING_MAX_RADIUS_M, ringProgress));
  explosion.ring.material.opacity = 1 - ringProgress;
  for (const piece of explosion.debris) {
    moveDebris(piece, frameS);
  }
}

/**
 * Writes the camera shake for this frame into `out` (zero when the shake is over or the
 * player asked for reduced motion) and returns it.
 */
export function cameraShake(explosion: Explosion, out: Vector3): Vector3 {
  out.set(0, 0, 0);
  const shaking = isExplosionPlaying(explosion) && explosion.ageS < SHAKE_S;
  if (explosion.reducedMotion || !shaking) {
    return out;
  }
  const strength = SHAKE_AMPLITUDE_M * (1 - explosion.ageS / SHAKE_S);
  return out.randomDirection().multiplyScalar(strength);
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

/**
 * Simple ballistic flight. Pieces that fall below the ground are hidden by it, so no
 * ground test is needed (and it works the same on any course or off the world).
 */
function moveDebris(piece: Debris, frameS: number): void {
  piece.velocityMS.y -= DEBRIS_GRAVITY_M_S2 * frameS;
  piece.mesh.position.addScaledVector(piece.velocityMS, frameS);
  piece.mesh.rotation.x += piece.spinRadS.x * frameS;
  piece.mesh.rotation.y += piece.spinRadS.y * frameS;
  piece.mesh.rotation.z += piece.spinRadS.z * frameS;
}

/** A random unit vector in the upper half, so debris flies up and out. */
function randomUpwardDirection(out: Vector3): void {
  const angleRad = MathUtils.randFloat(0, 2 * Math.PI);
  const upward = MathUtils.randFloat(0.3, 1);
  const sideways = Math.sqrt(1 - upward * upward);
  out.set(Math.cos(angleRad) * sideways, upward, Math.sin(angleRad) * sideways);
}
