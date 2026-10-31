// Scrape sparks: a short burst of hot points at the car's underside when the sim raises
// a Scrape (event 15). The burst size follows the normal speed (0 to 4 m/s). The
// particle maths is pure (no three.js needed to test it); `createSparks` draws it.
import {
  AdditiveBlending,
  BufferAttribute,
  BufferGeometry,
  Points,
  PointsMaterial,
  type Scene,
} from 'three';

const MAX_SPARKS = 96;
const SPARK_LIFE_S = 0.45;
const GRAVITY_M_S2 = 9.81;
/** Scrape normal speed at which the burst is full size. */
const FULL_BURST_NORMAL_M_S = 4;

export interface SparkField {
  /** x, y, z per spark. */
  position: Float32Array;
  velocity: Float32Array;
  age: Float32Array;
  /** Next slot to reuse. */
  next: number;
  /** Deterministic counter for the spread, so tests are stable. */
  seed: number;
}

export function newSparkField(): SparkField {
  return {
    position: new Float32Array(MAX_SPARKS * 3),
    velocity: new Float32Array(MAX_SPARKS * 3),
    age: new Float32Array(MAX_SPARKS).fill(SPARK_LIFE_S),
    next: 0,
    seed: 1,
  };
}

/** How many sparks a scrape of `normalSpeedMS` throws: 6 to 24. */
export function burstCount(normalSpeedMS: number): number {
  const fraction = Math.min(1, Math.max(0, normalSpeedMS / FULL_BURST_NORMAL_M_S));
  return Math.round(6 + 18 * fraction);
}

function random(field: SparkField): number {
  field.seed = (field.seed * 1664525 + 1013904223) >>> 0;
  return field.seed / 4294967296;
}

/** Throws a burst from (x, y, z). */
export function emitBurst(
  field: SparkField,
  x: number,
  y: number,
  z: number,
  normalSpeedMS: number,
): void {
  const count = burstCount(normalSpeedMS);
  for (let n = 0; n < count; n += 1) {
    const slot = field.next;
    field.next = (field.next + 1) % MAX_SPARKS;
    const at = slot * 3;
    field.position[at] = x;
    field.position[at + 1] = y;
    field.position[at + 2] = z;
    field.velocity[at] = (random(field) - 0.5) * 6;
    field.velocity[at + 1] = 1 + random(field) * 3;
    field.velocity[at + 2] = (random(field) - 0.5) * 6;
    field.age[slot] = 0;
  }
}

/** Ages and moves the sparks; dead ones sink out of sight at y = -1000. */
export function stepSparks(field: SparkField, dtS: number): number {
  let alive = 0;
  for (let slot = 0; slot < MAX_SPARKS; slot += 1) {
    const at = slot * 3;
    const age = (field.age[slot] ?? SPARK_LIFE_S) + dtS;
    field.age[slot] = age;
    if (age >= SPARK_LIFE_S) {
      field.position[at + 1] = -1000;
      continue;
    }
    alive += 1;
    field.velocity[at + 1] = (field.velocity[at + 1] ?? 0) - GRAVITY_M_S2 * dtS;
    for (let axis = 0; axis < 3; axis += 1) {
      field.position[at + axis] =
        (field.position[at + axis] ?? 0) + (field.velocity[at + axis] ?? 0) * dtS;
    }
  }
  return alive;
}

export interface Sparks {
  field: SparkField;
  points: Points<BufferGeometry, PointsMaterial>;
}

/** Adds the (empty) spark points to `scene`. */
export function createSparks(scene: Scene): Sparks {
  const field = newSparkField();
  field.position.fill(-1000);
  const geometry = new BufferGeometry();
  geometry.setAttribute('position', new BufferAttribute(field.position, 3));
  const material = new PointsMaterial({
    color: 0xffb347,
    size: 0.12,
    blending: AdditiveBlending,
    transparent: true,
    depthWrite: false,
  });
  const points = new Points(geometry, material);
  points.frustumCulled = false;
  scene.add(points);
  return { field, points };
}

/** Emits bursts for this frame's scrapes at the car, then moves the sparks. */
export function updateSparks(
  sparks: Sparks,
  scrapeNormalSpeeds: number[],
  car: { x: number; y: number; z: number },
  frameS: number,
): void {
  for (const speed of scrapeNormalSpeeds) {
    emitBurst(sparks.field, car.x, car.y - 0.3, car.z, speed);
  }
  stepSparks(sparks.field, Math.min(frameS, 0.1));
  const attribute = sparks.points.geometry.getAttribute('position');
  attribute.needsUpdate = true;
}
