// The swipe fallback for stunts: a swipe of 40 CSS px or more on the right half of the screen
// asks for a stunt, like the A S D F keys (left/right roll, up front flip, down back flip).
import { StuntCode, type StuntCodeValue } from '../game/stunt-requests.ts';

/** A swipe shorter than this, in CSS px, is a tap and asks for nothing. */
export const MIN_SWIPE_PX = 40;

/**
 * The stunt a swipe asks for, or null for a short one. The longer direction wins; screen y
 * grows downward, so up (negative dy) is the front flip.
 */
export function classifySwipe(dxPx: number, dyPx: number): StuntCodeValue | null {
  if (Math.max(Math.abs(dxPx), Math.abs(dyPx)) < MIN_SWIPE_PX) {
    return null;
  }
  if (Math.abs(dxPx) > Math.abs(dyPx)) {
    return dxPx < 0 ? StuntCode.BarrelLeft : StuntCode.BarrelRight;
  }
  return dyPx < 0 ? StuntCode.FrontFlip : StuntCode.BackFlip;
}

/** Watches the swipe zone and calls `onStunt` for each swipe long enough. */
export function createSwipeStunts(
  zone: HTMLElement,
  onStunt: (code: StuntCodeValue) => void,
): void {
  const starts = new Map<number, { x: number; y: number }>();
  zone.addEventListener('pointerdown', (event) => {
    starts.set(event.pointerId, { x: event.clientX, y: event.clientY });
    zone.setPointerCapture(event.pointerId);
  });
  zone.addEventListener('pointerup', (event) => {
    const start = starts.get(event.pointerId);
    starts.delete(event.pointerId);
    if (start) {
      const code = classifySwipe(event.clientX - start.x, event.clientY - start.y);
      if (code !== null) {
        onStunt(code);
      }
    }
  });
  zone.addEventListener('pointercancel', (event) => starts.delete(event.pointerId));
}
