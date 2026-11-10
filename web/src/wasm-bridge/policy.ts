// Loads the trained AI policy (models/policy.json, SPEC §9.12) into the game, for the
// default Hybrid page (?controller=pd skips it). The file is a separate asset fetched once at start; if it is
// missing or refused (wrong schema hash), the game runs without the AI.
import policyUrl from '../../../models/policy.json?url';

import type { Game } from '../wasm/nl_wasm.js';

/** Fetches the policy and hands it to `game`. Resolves to false (with a console warning)
 * when it cannot be loaded; the rest of the game works either way. */
export async function loadPolicy(game: Game): Promise<boolean> {
  try {
    const response = await fetch(policyUrl);
    if (!response.ok) {
      throw new Error(`HTTP ${response.status} for ${policyUrl}`);
    }
    game.load_policy(await response.text());
    return true;
  } catch (error) {
    console.warn('The AI policy could not be loaded; the AI controller is unavailable.', error);
    return false;
  }
}
