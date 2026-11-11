// Merges the input sources (keyboard, touch, gamepad) into the one 4-byte InputFrame the wasm
// Game reads each tick, so the game loop still has a single place to ask (SPEC §11.3).
// Throttle and steer are signed bytes in the sim, so analog sources pass real fractions.

import { readInputFrame } from './keyboard.ts';

/** Full scale of the throttle and steer bytes (crates/nl-sim/src/input.rs). */
const FULL_SCALE = 127;

/** What one source wants this tick. Axes run -1 to 1; steer is positive to the LEFT (SPEC §4). */
export interface AnalogInput {
  /** +1 full gas, -1 full brake or reverse. */
  throttle: number;
  /** +1 full left, -1 full right. */
  steer: number;
  /** Button bits, as in keyboard.ts (BUTTON_HOP and so on). */
  buttons: number;
}

/** A source that fills `out` (already zeroed) with what it wants this tick. */
export type InputSource = (out: AnalogInput) => void;

/** An input that asks for nothing. */
export function newAnalogInput(): AnalogInput {
  return { throttle: 0, steer: 0, buttons: 0 };
}

/** The axis value with the larger size: two sources never cancel or add up beyond full scale. */
function strongerAxis(first: number, second: number): number {
  return Math.abs(second) > Math.abs(first) ? second : first;
}

/**
 * Merges the keyboard's frame with the other sources into `out`: each axis takes the source
 * pushing hardest, and the buttons of all sources are combined.
 */
export function mergeFrame(
  keyboardFrame: Uint8Array,
  others: readonly AnalogInput[],
  out: Uint8Array,
): Uint8Array {
  const keyboardSigned = new Int8Array(keyboardFrame.buffer, keyboardFrame.byteOffset, 2);
  let throttle = (keyboardSigned[0] ?? 0) / FULL_SCALE;
  let steer = (keyboardSigned[1] ?? 0) / FULL_SCALE;
  let buttons = (keyboardFrame[2] ?? 0) | ((keyboardFrame[3] ?? 0) << 8);
  for (const other of others) {
    throttle = strongerAxis(throttle, other.throttle);
    steer = strongerAxis(steer, other.steer);
    buttons |= other.buttons;
  }
  const outSigned = new Int8Array(out.buffer, out.byteOffset, 2);
  outSigned[0] = Math.round(Math.max(-1, Math.min(1, throttle)) * FULL_SCALE);
  outSigned[1] = Math.round(Math.max(-1, Math.min(1, steer)) * FULL_SCALE);
  out[2] = buttons & 0xff;
  out[3] = (buttons >> 8) & 0xff;
  return out;
}

const sources: InputSource[] = [];
/** Scratch inputs, one per source, reused so no memory is allocated per tick. */
const scratch: AnalogInput[] = [];
const mergedFrame = new Uint8Array(4);

/** Adds a touch or gamepad source. Call at start-up. */
export function addInputSource(source: InputSource): void {
  sources.push(source);
  scratch.push(newAnalogInput());
}

/** The frame for the next tick: the keyboard plus every added source. */
export function readMergedFrame(): Uint8Array {
  for (let index = 0; index < sources.length; index += 1) {
    const input = scratch[index];
    if (input) {
      input.throttle = 0;
      input.steer = 0;
      input.buttons = 0;
      sources[index]?.(input);
    }
  }
  return mergeFrame(readInputFrame(), scratch, mergedFrame);
}
