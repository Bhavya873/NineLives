// The four on-screen arrows in a cross, bottom-left. One finger is captured on the cross and
// may slide between arrows or hold two at once (a diagonal). It fills an AnalogInput for
// input-merge.ts. The maths are in dpad-math.ts.
import { dpadAxes } from './dpad-math.ts';
import type { AnalogInput } from './input-merge.ts';

/** The arrows' live state. */
interface TouchArrows {
  cross: HTMLElement;
  up: HTMLElement;
  down: HTMLElement;
  left: HTMLElement;
  right: HTMLElement;
  /** The pointer driving the cross, or null while it rests. */
  pointerId: number | null;
  throttle: number;
  steer: number;
}

/** Finds the arrow element marked `data-arrow="<name>"` inside the cross. */
function findArrow(cross: HTMLElement, name: string): HTMLElement {
  const arrow = cross.querySelector<HTMLElement>(`[data-arrow="${name}"]`);
  if (!arrow) {
    throw new Error(
      `touch arrows: the cross is missing its '${name}' arrow (data-arrow="${name}")`,
    );
  }
  return arrow;
}

/** Wires the cross. The caller adds `readArrows` as an input source. */
export function createTouchArrows(cross: HTMLElement): TouchArrows {
  const arrows: TouchArrows = {
    cross,
    up: findArrow(cross, 'up'),
    down: findArrow(cross, 'down'),
    left: findArrow(cross, 'left'),
    right: findArrow(cross, 'right'),
    pointerId: null,
    throttle: 0,
    steer: 0,
  };
  cross.addEventListener('pointerdown', (event) => {
    if (arrows.pointerId !== null) {
      return;
    }
    event.preventDefault();
    arrows.pointerId = event.pointerId;
    cross.setPointerCapture(event.pointerId);
    readThumb(arrows, event);
  });
  cross.addEventListener('pointermove', (event) => {
    if (event.pointerId === arrows.pointerId) {
      readThumb(arrows, event);
    }
  });
  const letGo = (event: PointerEvent) => {
    if (event.pointerId === arrows.pointerId) {
      releaseArrows(arrows);
    }
  };
  cross.addEventListener('pointerup', letGo);
  cross.addEventListener('pointercancel', letGo);
  cross.addEventListener('lostpointercapture', letGo);
  // A long press on a phone opens a context menu; the game never wants it.
  cross.addEventListener('contextmenu', (event) => event.preventDefault());
  return arrows;
}

/** Turns the thumb position into axes and lights the arrows it is on. */
function readThumb(arrows: TouchArrows, event: PointerEvent): void {
  const box = arrows.cross.getBoundingClientRect();
  const dxPx = event.clientX - (box.left + box.width / 2);
  const dyPx = event.clientY - (box.top + box.height / 2);
  const axes = dpadAxes(dxPx, dyPx, box.width / 2);
  arrows.throttle = axes.throttle;
  arrows.steer = axes.steer;
  showPressed(arrows);
}

/** Gives the `pressed` class to the arrows the current axes ask for. */
function showPressed(arrows: TouchArrows): void {
  arrows.up.classList.toggle('pressed', arrows.throttle > 0);
  arrows.down.classList.toggle('pressed', arrows.throttle < 0);
  arrows.left.classList.toggle('pressed', arrows.steer > 0);
  arrows.right.classList.toggle('pressed', arrows.steer < 0);
}

/** Lets go: the axes return to zero and every arrow pops back up. */
export function releaseArrows(arrows: TouchArrows): void {
  arrows.pointerId = null;
  arrows.throttle = 0;
  arrows.steer = 0;
  showPressed(arrows);
}

/** Adds the arrows' axes to the tick's input. */
export function readArrows(arrows: TouchArrows, out: AnalogInput): void {
  out.throttle = arrows.throttle;
  out.steer = arrows.steer;
}
