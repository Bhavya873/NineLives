// Which stages the training viewer's crowds fly and where (SPEC §11.6): Stage A drops on the
// open flat arena or the obstacle arena of ramps and blocks (courses/arena.json), Stage B
// drops with the cars on their side, Stage D on a generated course, Stage F on a generated
// flip-plate straight. The viewer flies all of them at once; this module names them, reads
// which one the URL asks to focus on, and builds the matching wasm Crowd (every car flown by
// the Hybrid). It draws nothing.
import { Crowd } from '../wasm/nl_wasm.js';

/** The viewer's arenas, one panel each. */
export type ArenaKind = 'flat' | 'side' | 'obstacles' | 'driving' | 'flight';

/** Every arena, in the order the panels are laid out. */
export const ARENAS: ArenaKind[] = ['flat', 'side', 'obstacles', 'driving', 'flight'];

/** The URL parameter that focuses one panel: `?arena=driving`. No parameter shows all. */
const ARENA_PARAM = 'arena';

/** The `Crowd.for_stage` name of each arena (crates/nl-wasm/src/crowd.rs). */
const STAGE_NAMES: Record<ArenaKind, string> = {
  flat: 'A',
  side: 'B',
  obstacles: 'arena',
  driving: 'D',
  flight: 'F',
};

/** The arena the URL asks to focus on (`?arena=obstacles`, `driving`, …), or null for all. */
export function arenaFromUrl(search: string): ArenaKind | null {
  const wanted = new URLSearchParams(search).get(ARENA_PARAM);
  return ARENAS.find((kind) => kind === wanted) ?? null;
}

/** The page's query string with `kind` focused (null for all), for `history.replaceState`. */
export function searchWithArena(search: string, kind: ArenaKind | null): string {
  const params = new URLSearchParams(search);
  if (kind === null) {
    params.delete(ARENA_PARAM);
  } else {
    params.set(ARENA_PARAM, kind);
  }
  const text = params.toString();
  return text ? `?${text}` : '';
}

/** The arena's name as the page shows it. */
export function arenaLabel(kind: ArenaKind): string {
  switch (kind) {
    case 'flat':
      return 'Flat arena';
    case 'side':
      return 'Stage B on its side';
    case 'obstacles':
      return 'Obstacle arena';
    case 'driving':
      return 'Stage D course';
    case 'flight':
      return 'Stage F flip plate';
  }
}

/** True if every checkpoint row flies the same drops or the same generated course, so rows
 * compare fairly. On the obstacle arena each row gets its own seeds, so the cars do not sit
 * on top of each other. */
export function rowsShareSeeds(kind: ArenaKind): boolean {
  return kind !== 'obstacles';
}

/** True if the cars drive along a long course, so the camera follows them. */
export function followsCars(kind: ArenaKind): boolean {
  return kind === 'driving' || kind === 'flight';
}

/** True if the rows are drawn spread out on a grid (the sim runs every car at the arena
 * centre; the offset is for drawing only). */
export function spreadsOnGrid(kind: ArenaKind): boolean {
  return kind === 'flat' || kind === 'side';
}

/** `count` Realistic cars of the arena `kind` from seed `firstSeed` on, flown by the Hybrid
 * with the policy in `policyJson`. On the generated courses (Stage D and F) the course is the
 * one of `firstSeed`. */
export function newCrowd(
  kind: ArenaKind,
  policyJson: string,
  firstSeed: bigint,
  count: number,
): Crowd {
  return Crowd.for_stage(policyJson, 'realistic', STAGE_NAMES[kind], firstSeed, count);
}
