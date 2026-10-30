// The ONE call from the web into the sim for stunts. Until the Rust planner lands
// (branch claude/stunt-forecast) the wasm Game has no stunt method, so this returns false
// and the request just waits and expires. When the planner lands, only this file changes.
import type { StuntCodeValue } from './stunt-requests.ts';

/** The method the planner must add to the wasm `Game`. */
interface StuntCapableGame {
  /** Queue a stunt request (never starts one directly). Returns true if accepted. */
  request_stunt(code: number): boolean;
}

/** Passes a stunt request to the sim. False means the sim cannot take it (yet). */
export function requestStuntInSim(game: object, code: StuntCodeValue): boolean {
  const capable = game as Partial<StuntCapableGame>;
  return typeof capable.request_stunt === 'function' ? capable.request_stunt(code) : false;
}
