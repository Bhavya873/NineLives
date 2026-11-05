// The ONE call from the web into the sim for stunts: the wasm `Game.request_stunt`.
// An older wasm without it returns false, so the request just waits and expires.
import type { StuntCodeValue } from '../game/stunt-requests.ts';

/** The wasm `Game` method (crates/nl-wasm). */
interface StuntCapableGame {
  /** Queue a stunt request (the planner decides). Returns true if queued. */
  request_stunt(code: number): boolean;
}

/** Passes a stunt request to the sim. False means the sim cannot take it (yet). */
export function requestStuntInSim(game: object, code: StuntCodeValue): boolean {
  const capable = game as Partial<StuntCapableGame>;
  return typeof capable.request_stunt === 'function' ? capable.request_stunt(code) : false;
}

/** Which stunts the planner would take now (bit `code - 1`), or 0 if the build lacks it. */
export function stuntReadyMaskFromSim(game: object): number {
  const capable = game as { stunt_ready_mask?: () => number };
  return typeof capable.stunt_ready_mask === 'function' ? capable.stunt_ready_mask() : 0;
}
