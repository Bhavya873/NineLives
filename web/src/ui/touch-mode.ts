// Decides whether this is a touch device and tells the page with `data-touch="on"` on <html>.
// The touch controls, the phone layout and the rotate card all key off that one attribute.

/** True when `<html>` says touch controls are on. */
export function touchModeOn(): boolean {
  return document.documentElement.dataset['touch'] === 'on';
}

/**
 * Turns touch mode on when the main pointer is coarse (a phone or tablet) or after the first
 * touch (a laptop with a touch screen). Mouse and keyboard users never get it.
 */
export function startTouchMode(): void {
  const root = document.documentElement;
  const turnOn = () => {
    root.dataset['touch'] = 'on';
  };
  const coarse = window.matchMedia('(pointer: coarse)');
  if (coarse.matches) {
    turnOn();
  }
  coarse.addEventListener('change', () => {
    if (coarse.matches) {
      turnOn();
    }
  });
  window.addEventListener('touchstart', turnOn, { once: true, passive: true });
}

/** True when the screen is taller than wide on a touch device: the game asks to be turned. */
export function portraitOnTouch(): boolean {
  return touchModeOn() && window.matchMedia('(orientation: portrait)').matches;
}
