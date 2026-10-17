// Turns the keyboard into the 4-byte InputFrame the wasm Game expects (SPEC §11.3).
// It only remembers which keys are held; the game loop asks for a frame each tick.
// Layout of a frame: [throttle i8, steer i8, buttons low byte, buttons high byte].

/** Full scale of the throttle and steer bytes. */
const FULL_SCALE = 127;

const THROTTLE_KEYS = ['KeyW', 'ArrowUp'];
const BRAKE_KEYS = ['KeyS', 'ArrowDown'];
const STEER_LEFT_KEYS = ['KeyA', 'ArrowLeft'];
const STEER_RIGHT_KEYS = ['KeyD', 'ArrowRight'];

/** Every key this module handles, so the page does not scroll on arrow keys. */
const HANDLED_KEYS = [...THROTTLE_KEYS, ...BRAKE_KEYS, ...STEER_LEFT_KEYS, ...STEER_RIGHT_KEYS];

/** Keys that are currently held down, by `KeyboardEvent.code`. */
const heldKeys = new Set<string>();

/** Starts listening to the keyboard. Call once at start-up. */
export function startKeyboard(target: Window): void {
  target.addEventListener('keydown', (event) => {
    if (HANDLED_KEYS.includes(event.code)) {
      event.preventDefault();
    }
    heldKeys.add(event.code);
  });
  target.addEventListener('keyup', (event) => {
    heldKeys.delete(event.code);
  });
  // Letting go of every key when the tab loses focus stops the car "sticking" on.
  target.addEventListener('blur', () => {
    heldKeys.clear();
  });
}

/** Builds the input frame for the next tick from the keys held right now. */
export function readInputFrame(): Uint8Array {
  const throttle = axis(THROTTLE_KEYS, BRAKE_KEYS);
  // Positive steer is left, matching the sim's yaw sign (SPEC §4).
  const steer = axis(STEER_LEFT_KEYS, STEER_RIGHT_KEYS);
  const frame = new Uint8Array(4);
  const signed = new Int8Array(frame.buffer);
  signed[0] = throttle;
  signed[1] = steer;
  // Bytes 2 and 3 are the buttons (Hop, Boost, ...), added in M3.
  return frame;
}

/** +127 if a "positive" key is held, -127 if a "negative" one is, 0 for both or neither. */
function axis(positiveKeys: string[], negativeKeys: string[]): number {
  const positive = positiveKeys.some((key) => heldKeys.has(key));
  const negative = negativeKeys.some((key) => heldKeys.has(key));
  if (positive === negative) {
    return 0;
  }
  return positive ? FULL_SCALE : -FULL_SCALE;
}
