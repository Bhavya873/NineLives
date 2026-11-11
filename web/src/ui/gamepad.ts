// Gamepad support: polled once per frame with navigator.getGamepads(). It adds throttle,
// steer, HOP and BOOST to the tick's input and fires stunts and pause on button presses.
// The mapping itself is in gamepad-map.ts. No vibration.
import type { StuntCodeValue } from '../game/stunt-requests.ts';
import { BUTTON_BOOST, BUTTON_HOP } from './keyboard.ts';
import type { AnalogInput } from './input-merge.ts';
import {
  heldButtons,
  newlyPressed,
  PAD_STUNT_BUTTONS,
  padAxes,
  PadButton,
  type PadSnapshot,
} from './gamepad-map.ts';

/** Buttons read per poll: everything up to and including Start. */
const BUTTONS_READ = PadButton.Start + 1;

/** What the gamepad does when a button goes down. */
interface PadActions {
  onStunt: (code: StuntCodeValue) => void;
  onPause: () => void;
}

/** The pad's last poll. */
interface PadState {
  throttle: number;
  steer: number;
  /** HOP and BOOST held at the last poll. */
  buttons: number;
  /** HOP or BOOST pressed since the last tick read them. */
  tapped: number;
  previousHeld: boolean[];
  actions: PadActions;
}

/** Starts the gamepad. Marks `<html data-gamepad="on">` while one is plugged in. */
export function createGamepad(actions: PadActions): PadState {
  window.addEventListener('gamepadconnected', () => {
    document.documentElement.dataset['gamepad'] = 'on';
  });
  window.addEventListener('gamepaddisconnected', () => {
    delete document.documentElement.dataset['gamepad'];
  });
  return { throttle: 0, steer: 0, buttons: 0, tapped: 0, previousHeld: [], actions };
}

/** The first connected pad with the standard layout, or null. */
function findStandardPad(): Gamepad | null {
  const pads = navigator.getGamepads?.() ?? [];
  for (const pad of pads) {
    if (pad?.connected && pad.mapping === 'standard') {
      return pad;
    }
  }
  return null;
}

/** Reads the pad once. Call once per rendered frame. */
export function pollGamepad(state: PadState): void {
  const pad: PadSnapshot | null = findStandardPad();
  if (!pad) {
    state.throttle = 0;
    state.steer = 0;
    state.buttons = 0;
    state.previousHeld = [];
    return;
  }
  const axes = padAxes(pad);
  state.throttle = axes.throttle;
  state.steer = axes.steer;
  const held = heldButtons(pad, BUTTONS_READ);
  state.buttons = (held[PadButton.A] ? BUTTON_HOP : 0) | (held[PadButton.B] ? BUTTON_BOOST : 0);
  const pressed = newlyPressed(state.previousHeld, held);
  state.previousHeld = held;
  for (const button of pressed) {
    const stunt = PAD_STUNT_BUTTONS[button];
    if (stunt !== undefined) {
      state.actions.onStunt(stunt);
    }
    if (button === PadButton.A) {
      state.tapped |= BUTTON_HOP;
    } else if (button === PadButton.B) {
      state.tapped |= BUTTON_BOOST;
    } else if (button === PadButton.Start) {
      state.actions.onPause();
    }
  }
}

/** Adds the pad's last poll to the tick's input. */
export function readGamepad(state: PadState, out: AnalogInput): void {
  out.throttle = state.throttle;
  out.steer = state.steer;
  out.buttons |= state.buttons | state.tapped;
  state.tapped = 0;
}
