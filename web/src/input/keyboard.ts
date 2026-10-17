// Turns the keyboard into the 4-byte InputFrame the wasm Game expects (SPEC §11.3).
// It only remembers which keys are held; the game loop asks for a frame each tick.
// Layout of a frame: [throttle i8, steer i8, buttons low byte, buttons high byte].

/** Full scale of the throttle and steer bytes. */
const FULL_SCALE = 127;

const THROTTLE_KEYS = ['KeyW', 'ArrowUp'];
const BRAKE_KEYS = ['KeyS', 'ArrowDown'];
const STEER_LEFT_KEYS = ['KeyA', 'ArrowLeft'];
const STEER_RIGHT_KEYS = ['KeyD', 'ArrowRight'];
const HOP_KEYS = ['Space'];
const BOOST_KEYS = ['ShiftLeft', 'ShiftRight'];
const CHAOS_KEYS = ['KeyC'];
const RESET_KEYS = ['KeyR'];

/**
 * Button bits, matching the BUTTON_* constants in crates/nl-sim/src/input.rs.
 * main.ts checks them against the wasm build at start-up.
 */
export const BUTTON_HOP = 1 << 0;
export const BUTTON_BOOST = 1 << 1;
export const BUTTON_CHAOS = 1 << 2;
export const BUTTON_RESET = 1 << 3;

/** Every held key this module handles, so the page does not scroll on arrows or Space. */
const HANDLED_KEYS = [
  ...THROTTLE_KEYS,
  ...BRAKE_KEYS,
  ...STEER_LEFT_KEYS,
  ...STEER_RIGHT_KEYS,
  ...HOP_KEYS,
  ...BOOST_KEYS,
  ...CHAOS_KEYS,
  ...RESET_KEYS,
];

/** Keys that are currently held down, by `KeyboardEvent.code`. */
const heldKeys = new Set<string>();

/**
 * Bits of the press-to-trigger buttons (Hop, Chaos, Reset) pressed since the last frame. A quick
 * tap can go down and up between two ticks on a slow frame; remembering the press makes
 * sure the game still sees it.
 */
let tappedButtons = 0;

/** One-shot actions for single key presses (e.g. M toggles the mode), by key code. */
const pressActions = new Map<string, () => void>();

/** Runs `action` once each time `code` is pressed (holding the key does not repeat it). */
export function onKeyPress(code: string, action: () => void): void {
  pressActions.set(code, action);
}

/** Starts listening to the keyboard. Call once at start-up. */
export function startKeyboard(target: Window): void {
  target.addEventListener('keydown', (event) => {
    // Browsers often never send keyup for keys pressed during a Cmd/Ctrl/Alt shortcut,
    // so drop everything rather than risk a key that stays held forever.
    if (event.metaKey || event.ctrlKey || event.altKey) {
      heldKeys.clear();
      return;
    }
    if (HANDLED_KEYS.includes(event.code)) {
      event.preventDefault();
    }
    heldKeys.add(event.code);
    if (HOP_KEYS.includes(event.code)) {
      tappedButtons |= BUTTON_HOP;
    }
    if (CHAOS_KEYS.includes(event.code)) {
      tappedButtons |= BUTTON_CHAOS;
    }
    if (RESET_KEYS.includes(event.code)) {
      tappedButtons |= BUTTON_RESET;
    }
    if (!event.repeat) {
      pressActions.get(event.code)?.();
    }
  });
  target.addEventListener('keyup', (event) => {
    heldKeys.delete(event.code);
  });
  // Letting go of every key when the tab loses focus stops the car "sticking" on.
  target.addEventListener('blur', () => {
    heldKeys.clear();
  });
}

/** The frame handed to the game each tick; reused so no memory is allocated per tick. */
const frame = new Uint8Array(4);
/** The same bytes seen as signed numbers, for throttle and steer. */
const frameSigned = new Int8Array(frame.buffer);

/** Builds the input frame for the next tick from the keys held right now. */
export function readInputFrame(): Uint8Array {
  frameSigned[0] = axis(THROTTLE_KEYS, BRAKE_KEYS);
  // Positive steer is left, matching the sim's yaw sign (SPEC §4).
  frameSigned[1] = axis(STEER_LEFT_KEYS, STEER_RIGHT_KEYS);
  let buttons = tappedButtons;
  tappedButtons = 0;
  if (isAnyHeld(HOP_KEYS)) {
    buttons |= BUTTON_HOP;
  }
  if (isAnyHeld(BOOST_KEYS)) {
    buttons |= BUTTON_BOOST;
  }
  if (isAnyHeld(CHAOS_KEYS)) {
    buttons |= BUTTON_CHAOS;
  }
  if (isAnyHeld(RESET_KEYS)) {
    buttons |= BUTTON_RESET;
  }
  // Bytes 2 and 3 are the buttons, little-endian.
  frame[2] = buttons & 0xff;
  frame[3] = buttons >> 8;
  return frame;
}

/** +127 if a "positive" key is held, -127 if a "negative" one is, 0 for both or neither. */
function axis(positiveKeys: string[], negativeKeys: string[]): number {
  const positive = isAnyHeld(positiveKeys);
  const negative = isAnyHeld(negativeKeys);
  if (positive === negative) {
    return 0;
  }
  return positive ? FULL_SCALE : -FULL_SCALE;
}

function isAnyHeld(keys: string[]): boolean {
  return keys.some((key) => heldKeys.has(key));
}
