// Which arena the training viewer's drops land in (SPEC §11.6): the open flat arena (M6)
// or the obstacle arena of ramps and blocks (M7, courses/arena.json). This module picks
// the arena from the URL and builds the matching wasm Crowd; it draws nothing.
import arenaJson from '../../../courses/arena.json?raw';
import { Crowd } from '../wasm/nl_wasm.js';

/** The two arenas: open flat ground, or the obstacle arena course. */
export type ArenaKind = 'flat' | 'obstacles';

/** The URL parameter that picks the arena: `?arena=obstacles`. */
const ARENA_PARAM = 'arena';

/** The arena the URL asks for (`?arena=obstacles`), else the flat arena. */
export function arenaFromUrl(search: string): ArenaKind {
  return new URLSearchParams(search).get(ARENA_PARAM) === 'obstacles' ? 'obstacles' : 'flat';
}

/** The page's query string with `kind` as its arena, for `history.replaceState`. */
export function searchWithArena(search: string, kind: ArenaKind): string {
  const params = new URLSearchParams(search);
  if (kind === 'obstacles') {
    params.set(ARENA_PARAM, kind);
  } else {
    params.delete(ARENA_PARAM);
  }
  const text = params.toString();
  return text ? `?${text}` : '';
}

/** The arena that is not `kind`. */
export function otherArena(kind: ArenaKind): ArenaKind {
  return kind === 'flat' ? 'obstacles' : 'flat';
}

/** The arena's name as the page shows it. */
export function arenaLabel(kind: ArenaKind): string {
  return kind === 'flat' ? 'Flat arena' : 'Obstacle arena';
}

/** `count` Realistic Stage A drops from seed `firstSeed` on, flown by the policy in
 * `policyJson`, in the arena `kind`. */
export function newCrowd(
  kind: ArenaKind,
  policyJson: string,
  firstSeed: bigint,
  count: number,
): Crowd {
  if (kind === 'obstacles') {
    return Crowd.on_course(policyJson, 'realistic', firstSeed, count, arenaJson);
  }
  return new Crowd(policyJson, 'realistic', firstSeed, count);
}
