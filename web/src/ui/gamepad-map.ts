// Maps a standard-layout gamepad to the game's inputs, with no page code so it runs in plain
// Node. Triggers drive, the left stick steers, buttons hop, boost, do stunts and pause.
import { StuntCode, type StuntCodeValue } from '../game/stunt-requests.ts';

/** The left stick reads as centred inside this fraction of its travel. */
export const PAD_STICK_DEAD_ZONE = 0.15;
/** A trigger reads as released below this pull (0 to 1). */
export const PAD_TRIGGER_DEAD_ZONE = 0.05;

/** Button numbers of the browser's "standard" gamepad mapping. */
export const PadButton = {
  A: 0,
  B: 1,
  X: 2,
  Y: 3,
  LeftBumper: 4,
  RightBumper: 5,
  LeftTrigger: 6,
  RightTrigger: 7,
  Start: 9,
} as const;

/** The stunt each button asks for (edge-detected, so holding it asks once). */
export const PAD_STUNT_BUTTONS: Record<number, StuntCodeValue> = {
  [PadButton.LeftBumper]: StuntCode.BarrelLeft,
  [PadButton.RightBumper]: StuntCode.BarrelRight,
  [PadButton.Y]: StuntCode.FrontFlip,
  [PadButton.X]: StuntCode.BackFlip,
};

/** The part of a browser Gamepad this module reads. */
export interface PadSnapshot {
  axes: readonly number[];
  buttons: readonly { pressed: boolean; value: number }[];
}

/** A stick or trigger value with its dead zone cut out and the rest stretched to full range. */
export function applyDeadZone(value: number, deadZone: number): number {
  const size = Math.abs(value);
  if (size <= deadZone) {
    return 0;
  }
  return Math.sign(value) * Math.min(1, (size - deadZone) / (1 - deadZone));
}

/** Throttle and steer: right trigger gas, left trigger brake, left stick X steers. */
export function padAxes(pad: PadSnapshot): { throttle: number; steer: number } {
  const gas = applyDeadZone(pad.buttons[PadButton.RightTrigger]?.value ?? 0, PAD_TRIGGER_DEAD_ZONE);
  const brake = applyDeadZone(
    pad.buttons[PadButton.LeftTrigger]?.value ?? 0,
    PAD_TRIGGER_DEAD_ZONE,
  );
  // Stick right is +1, but the game's steer is positive to the LEFT. "+ 0" removes a -0.
  const steer = -applyDeadZone(pad.axes[0] ?? 0, PAD_STICK_DEAD_ZONE) + 0;
  return { throttle: gas - brake, steer };
}

/** Which buttons are down, by button number (`count` entries). */
export function heldButtons(pad: PadSnapshot, count: number): boolean[] {
  const held: boolean[] = [];
  for (let index = 0; index < count; index += 1) {
    held.push(pad.buttons[index]?.pressed ?? false);
  }
  return held;
}

/** The button numbers that went down since `previous` (a held button is not repeated). */
export function newlyPressed(previous: readonly boolean[], current: readonly boolean[]): number[] {
  const pressed: number[] = [];
  for (let index = 0; index < current.length; index += 1) {
    if (current[index] && !previous[index]) {
      pressed.push(index);
    }
  }
  return pressed;
}
