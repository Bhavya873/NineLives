// Start-up check that this page and the wasm build agree on shared layouts and button bits.
// A mismatch would draw garbage or press the wrong button, so it stops the game with a clear error.
import {
  BUTTON_BOOST,
  BUTTON_CHAOS,
  BUTTON_HOP,
  BUTTON_RESET,
  FIRST_MANUAL_NOZZLE_BIT,
  MANUAL_NOZZLE_COUNT,
} from '../ui/keyboard.ts';
import { FLOATS_PER_POSE } from '../render/pose-blend.ts';
import { FLOATS_PER_NOZZLE } from './nozzle-layout.ts';
import { FLOATS_PER_EVENT } from './sim-events.ts';
import { Game } from '../wasm/nl_wasm.js';

/** Throws if this page and the wasm build disagree on a shared layout or bit. */
export function checkWasmLayout(): void {
  if (Game.floats_per_pose() !== FLOATS_PER_POSE) {
    throw new Error('The wasm build and the renderer disagree on the pose layout');
  }
  if (Game.floats_per_nozzle() !== FLOATS_PER_NOZZLE) {
    throw new Error('The wasm build and the renderer disagree on the nozzle layout');
  }
  if (Game.floats_per_event() !== FLOATS_PER_EVENT) {
    throw new Error('The wasm build and the game disagree on the event layout');
  }
  const buttonsMatch =
    Game.button_hop() === BUTTON_HOP &&
    Game.button_boost() === BUTTON_BOOST &&
    Game.button_chaos() === BUTTON_CHAOS &&
    Game.button_reset() === BUTTON_RESET &&
    Game.first_manual_nozzle_bit() === FIRST_MANUAL_NOZZLE_BIT &&
    Game.nozzle_count() === MANUAL_NOZZLE_COUNT;
  if (!buttonsMatch) {
    throw new Error('The wasm build and the keyboard disagree on the button bits');
  }
}
