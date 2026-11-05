// Wind streaks (rendering only): faint white lines drifting with the wind around the
// car, so a breeze or storm is visible in the scene. They live in a box that follows
// the car and wrap around its edges, and hide entirely while the wind is off.
// The wind comes from the sim (HudField WindXMS/WindZMS); this file only draws it.
import {
  AdditiveBlending,
  BufferAttribute,
  BufferGeometry,
  LineBasicMaterial,
  LineSegments,
  type Scene,
  Vector3,
} from 'three';

const STREAK_COUNT = 160;
/** Half the size of the box the streaks fill around the car, in m. */
const BOX_HALF_M = 18;
/** Streaks sit between these heights above the car's centre, in m. */
const BOX_LOW_M = -0.5;
const BOX_HIGH_M = 6;
/** A streak is as long as the air travels in this time, so faster wind draws longer. */
const STREAK_TIME_S = 0.12;
/** Below this wind speed (m/s) the streaks hide. */
const HIDE_BELOW_M_S = 0.5;
const STREAK_OPACITY = 0.2;

/** The streaks and where each one is, relative to the box's centre. */
export interface WindStreaks {
  lines: LineSegments<BufferGeometry, LineBasicMaterial>;
  /** Each streak's tail, in m, relative to the car; three numbers per streak. */
  offsetsM: Float32Array;
}

/** Builds the (hidden) streaks and adds them to `scene`. */
export function createWindStreaks(scene: Scene): WindStreaks {
  const offsetsM = new Float32Array(STREAK_COUNT * 3);
  // A fixed spread from a tiny hash, so the pattern is the same on every visit.
  for (let index = 0; index < STREAK_COUNT; index += 1) {
    offsetsM[3 * index] = (spread(index, 1) * 2 - 1) * BOX_HALF_M;
    offsetsM[3 * index + 1] = BOX_LOW_M + spread(index, 2) * (BOX_HIGH_M - BOX_LOW_M);
    offsetsM[3 * index + 2] = (spread(index, 3) * 2 - 1) * BOX_HALF_M;
  }
  const geometry = new BufferGeometry();
  geometry.setAttribute('position', new BufferAttribute(new Float32Array(STREAK_COUNT * 6), 3));
  const material = new LineBasicMaterial({
    color: 0xffffff,
    transparent: true,
    opacity: STREAK_OPACITY,
    blending: AdditiveBlending,
    depthWrite: false,
  });
  const lines = new LineSegments(geometry, material);
  lines.frustumCulled = false;
  lines.visible = false;
  scene.add(lines);
  return { lines, offsetsM };
}

/**
 * Moves the streaks with the wind `windWorld` ([x, y, z] m/s) for `frameS` seconds and
 * centres their box on `centre` (the car). Hidden while the wind is off.
 */
export function updateWindStreaks(
  streaks: WindStreaks,
  windWorld: [number, number, number],
  centre: Vector3,
  frameS: number,
): void {
  const [windX, windY, windZ] = windWorld;
  const speedM_S = Math.hypot(windX, windY, windZ);
  streaks.lines.visible = speedM_S >= HIDE_BELOW_M_S;
  if (!streaks.lines.visible) {
    return;
  }
  const offsets = streaks.offsetsM;
  const positions = streaks.lines.geometry.getAttribute('position') as BufferAttribute;
  for (let index = 0; index < STREAK_COUNT; index += 1) {
    const x = wrap((offsets[3 * index] ?? 0) + windX * frameS, -BOX_HALF_M, BOX_HALF_M);
    const y = wrap((offsets[3 * index + 1] ?? 0) + windY * frameS, BOX_LOW_M, BOX_HIGH_M);
    const z = wrap((offsets[3 * index + 2] ?? 0) + windZ * frameS, -BOX_HALF_M, BOX_HALF_M);
    offsets[3 * index] = x;
    offsets[3 * index + 1] = y;
    offsets[3 * index + 2] = z;
    // Tail at the offset, head one streak-length downwind.
    positions.setXYZ(2 * index, centre.x + x, centre.y + y, centre.z + z);
    positions.setXYZ(
      2 * index + 1,
      centre.x + x + windX * STREAK_TIME_S,
      centre.y + y + windY * STREAK_TIME_S,
      centre.z + z + windZ * STREAK_TIME_S,
    );
  }
  positions.needsUpdate = true;
}

/** Wraps `value` into [low, high), so a streak leaving one side comes back on the other. */
export function wrap(value: number, low: number, high: number): number {
  const span = high - low;
  return low + ((((value - low) % span) + span) % span);
}

/** A repeatable number in [0, 1) for streak `index` and axis `salt` (no Math.random). */
function spread(index: number, salt: number): number {
  const hashed = Math.sin(index * 12.9898 + salt * 78.233) * 43758.5453;
  return hashed - Math.floor(hashed);
}
