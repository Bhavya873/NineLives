// Which stage the training viewer's crowd flies and where (SPEC §11.6): Stage A drops on
// the open flat arena (M6) or the obstacle arena of ramps and blocks (M7, courses/arena.json),
// Stage D on a generated course, Stage F on a generated flip-plate straight, or the Stage D
// bot on the Gauntlet. This module picks the stage from the URL and builds the matching wasm
// Crowd (every car flown by the Hybrid); it draws nothing.
import { Crowd } from '../wasm/nl_wasm.js';

/** The viewer's arenas: flat ground or the obstacle arena for Stage A drops, flat ground with
 * Stage B's cars lying on their side, a generated
 * course for Stage D, a flip-plate straight for Stage F, and the Gauntlet. */
export type ArenaKind = 'flat' | 'side' | 'obstacles' | 'driving' | 'flight' | 'gauntlet';

/** Every arena, in the order the picker lists them. */
export const ARENAS: ArenaKind[] = ['flat', 'side', 'obstacles', 'driving', 'flight', 'gauntlet'];

/** The URL parameter that picks the arena: `?arena=obstacles`. */
const ARENA_PARAM = 'arena';

/** The `Crowd.for_stage` name of each arena (crates/nl-wasm/src/crowd.rs). */
const STAGE_NAMES: Record<ArenaKind, string> = {
  flat: 'A',
  side: 'B',
  obstacles: 'arena',
  driving: 'D',
  flight: 'F',
  gauntlet: 'gauntlet',
};

/** The arena the URL asks for (`?arena=obstacles`, `driving`, `flight` or `gauntlet`),
 * else the flat arena. */
export function arenaFromUrl(search: string): ArenaKind {
  const wanted = new URLSearchParams(search).get(ARENA_PARAM);
  return ARENAS.find((kind) => kind === wanted) ?? 'flat';
}

/** The page's query string with `kind` as its arena, for `history.replaceState`. */
export function searchWithArena(search: string, kind: ArenaKind): string {
  const params = new URLSearchParams(search);
  if (kind === 'flat') {
    params.delete(ARENA_PARAM);
  } else {
    params.set(ARENA_PARAM, kind);
  }
  const text = params.toString();
  return text ? `?${text}` : '';
}

/** The arena the O key and the toggle button switch to: the obstacle arena from any
 * other, back to the flat arena from the obstacle arena. */
export function otherArena(kind: ArenaKind): ArenaKind {
  return kind === 'obstacles' ? 'flat' : 'obstacles';
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
    case 'gauntlet':
      return 'Gauntlet';
  }
}

/** True if every checkpoint row flies the same drops or the same generated course, so rows
 * compare fairly. On the obstacle arena and the Gauntlet each row gets its own seeds, so
 * the cars do not sit on top of each other. */
export function rowsShareSeeds(kind: ArenaKind): boolean {
  return kind === 'flat' || kind === 'side' || kind === 'driving' || kind === 'flight';
}

/** True if the cars drive along a long course, so the camera follows them. */
export function followsCars(kind: ArenaKind): boolean {
  return kind === 'driving' || kind === 'flight' || kind === 'gauntlet';
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
