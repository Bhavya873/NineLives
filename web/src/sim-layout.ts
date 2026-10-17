// Shapes of the flat arrays the wasm Game hands to the page that are not poses or events.
// main.ts checks each against the wasm build at start-up.

/** Numbers per nozzle in `Game.nozzle_layout`: position x, y, z, then exhaust x, y, z. */
export const FLOATS_PER_NOZZLE = 6;
