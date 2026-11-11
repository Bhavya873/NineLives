// The floating joystick: a touch in the left zone puts the ring under the finger and the knob
// follows. It fills an AnalogInput for input-merge.ts. Maths are in stick-math.ts.
import type { AnalogInput } from './input-merge.ts';
import { clampToRing, stickAxes } from './stick-math.ts';

/** How far the knob may travel, as a fraction of the ring's radius. */
const KNOB_TRAVEL = 0.6;
/** The zone's top edge, in px, so the top row of the HUD stays tappable. */
const TOP_ROW_PX = 64;

/** The stick's live state. */
interface TouchStick {
  zone: HTMLElement;
  ring: HTMLElement;
  /** The pointer driving the stick, or null while it rests. */
  pointerId: number | null;
  centreX: number;
  centreY: number;
  travelPx: number;
  throttle: number;
  steer: number;
}

/** Wires the zone and ring. The caller adds `readStick` as an input source. */
export function createTouchStick(zone: HTMLElement, ring: HTMLElement): TouchStick {
  const stick: TouchStick = {
    zone,
    ring,
    pointerId: null,
    centreX: 0,
    centreY: 0,
    travelPx: 1,
    throttle: 0,
    steer: 0,
  };
  zone.addEventListener('pointerdown', (event) => grabStick(stick, event));
  zone.addEventListener('pointermove', (event) => {
    if (event.pointerId === stick.pointerId) {
      moveKnob(stick, event);
    }
  });
  const letGo = (event: PointerEvent) => {
    if (event.pointerId === stick.pointerId) {
      releaseStick(stick);
    }
  };
  zone.addEventListener('pointerup', letGo);
  zone.addEventListener('pointercancel', letGo);
  zone.addEventListener('lostpointercapture', letGo);
  return stick;
}

/** Puts the ring under the finger, inside the screen, and starts following it. */
function grabStick(stick: TouchStick, event: PointerEvent): void {
  if (stick.pointerId !== null) {
    return;
  }
  const radiusPx = stick.ring.offsetWidth / 2;
  // Keep the whole ring on screen so the knob can reach every direction.
  stick.centreX = Math.max(radiusPx, Math.min(window.innerWidth - radiusPx, event.clientX));
  stick.centreY = Math.max(
    TOP_ROW_PX + radiusPx,
    Math.min(window.innerHeight - radiusPx, event.clientY),
  );
  stick.travelPx = radiusPx * KNOB_TRAVEL;
  stick.pointerId = event.pointerId;
  stick.zone.setPointerCapture(event.pointerId);
  stick.ring.classList.add('drag');
  stick.ring.style.left = `${stick.centreX - radiusPx}px`;
  stick.ring.style.top = `${stick.centreY - radiusPx}px`;
  moveKnob(stick, event);
}

/** Moves the knob to the finger (clamped to the ring) and updates the axes. */
function moveKnob(stick: TouchStick, event: PointerEvent): void {
  const dxPx = event.clientX - stick.centreX;
  const dyPx = event.clientY - stick.centreY;
  const knob = clampToRing(dxPx, dyPx, stick.travelPx);
  stick.ring.style.setProperty('--kx', `${knob.dxPx}px`);
  stick.ring.style.setProperty('--ky', `${knob.dyPx}px`);
  const axes = stickAxes(dxPx, dyPx, stick.travelPx);
  stick.throttle = axes.throttle;
  stick.steer = axes.steer;
}

/** Lets go: the axes return to zero and the ring eases back to its resting place. */
export function releaseStick(stick: TouchStick): void {
  stick.pointerId = null;
  stick.throttle = 0;
  stick.steer = 0;
  stick.ring.classList.remove('drag');
  stick.ring.style.left = '';
  stick.ring.style.top = '';
  stick.ring.style.setProperty('--kx', '0px');
  stick.ring.style.setProperty('--ky', '0px');
}

/** Adds the stick's axes to the tick's input. */
export function readStick(stick: TouchStick, out: AnalogInput): void {
  out.throttle = stick.throttle;
  out.steer = stick.steer;
}
