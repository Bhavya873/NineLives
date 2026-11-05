// Reading `Game.nozzle_layout` (SPEC §7.4, §11.1): where each nozzle sits on the car and
// which way its gas leaves, in the car's body frame (+X forward, +Y up, +Z right).
// The flames and the HUD's nozzle map both draw from this one reader.

/** Numbers per nozzle in the layout; layout-check.ts checks it against the wasm build. */
export const FLOATS_PER_NOZZLE = 6;

/** One nozzle's place on the car, in nozzle order. */
export interface NozzlePlace {
  /** Position relative to the centre of mass, in metres: [forward, up, right]. */
  positionM: [number, number, number];
  /** Unit vector the exhaust leaves along (opposite to the push). */
  exhaust: [number, number, number];
}

/** Splits the flat layout array into one `NozzlePlace` per nozzle. */
export function readNozzleLayout(layout: Float32Array): NozzlePlace[] {
  const places: NozzlePlace[] = [];
  for (let start = 0; start + FLOATS_PER_NOZZLE <= layout.length; start += FLOATS_PER_NOZZLE) {
    places.push({
      positionM: [layout[start] ?? 0, layout[start + 1] ?? 0, layout[start + 2] ?? 0],
      exhaust: [layout[start + 3] ?? 0, layout[start + 4] ?? 0, layout[start + 5] ?? 0],
    });
  }
  return places;
}
