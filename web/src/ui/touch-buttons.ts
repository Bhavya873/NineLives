// The HOP and BOOST touch buttons. A press is remembered until a tick reads it, so a tap that
// ends between two physics ticks still counts (the same idea as tappedButtons in keyboard.ts).
import { BUTTON_BOOST, BUTTON_HOP } from './keyboard.ts';
import type { AnalogInput } from './input-merge.ts';

/** One on-screen button and the bit it presses. */
interface TouchButton {
  element: HTMLElement;
  bit: number;
  /** Pointer ids pressing it right now (two thumbs can share one button). */
  pointerIds: Set<number>;
  /** True from a press until the next tick reads it. */
  tapped: boolean;
}

/** Both buttons. */
interface TouchButtons {
  buttons: TouchButton[];
}

function wireButton(button: TouchButton): void {
  const { element } = button;
  element.addEventListener('pointerdown', (event) => {
    event.preventDefault();
    element.setPointerCapture(event.pointerId);
    button.pointerIds.add(event.pointerId);
    button.tapped = true;
    element.classList.add('pressed');
  });
  const letGo = (event: PointerEvent) => {
    button.pointerIds.delete(event.pointerId);
    if (button.pointerIds.size === 0) {
      element.classList.remove('pressed');
    }
  };
  element.addEventListener('pointerup', letGo);
  element.addEventListener('pointercancel', letGo);
  element.addEventListener('lostpointercapture', letGo);
  // Enter or Space on a focused button arrives as a click with no pointer (`detail` is 0); a
  // pointer tap also clicks (`detail` 1 or more) but was already counted on pointerdown.
  element.addEventListener('click', (event) => {
    if (event.detail === 0) {
      button.tapped = true;
    }
  });
  // A long press on a phone opens a context menu; the game never wants it.
  element.addEventListener('contextmenu', (event) => event.preventDefault());
}

/** Wires the two buttons. The caller adds `readButtons` as an input source. */
export function createTouchButtons(hop: HTMLElement, boost: HTMLElement): TouchButtons {
  const buttons = [
    { element: hop, bit: BUTTON_HOP },
    { element: boost, bit: BUTTON_BOOST },
  ].map((spec) => ({ ...spec, pointerIds: new Set<number>(), tapped: false }));
  buttons.forEach(wireButton);
  return { buttons };
}

/** Adds the held and just-tapped buttons to the tick's input. */
export function readButtons(touch: TouchButtons, out: AnalogInput): void {
  for (const button of touch.buttons) {
    if (button.pointerIds.size > 0 || button.tapped) {
      out.buttons |= button.bit;
    }
    button.tapped = false;
  }
}

/** Lets go of both buttons (the game paused under a finger). */
export function releaseButtons(touch: TouchButtons): void {
  for (const button of touch.buttons) {
    button.pointerIds.clear();
    button.element.classList.remove('pressed');
  }
}
