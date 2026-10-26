// The F3 performance overlay (SPEC §11.7): frames per second, and the milliseconds a
// frame spends on physics ticks and on drawing. main.ts times the work and hands the
// numbers in; this module only adds them up and shows them.

/** How often the readout refreshes, in milliseconds: steady numbers, not a flicker. */
const REFRESH_MS = 500;

/** The overlay element and what it has counted since its last refresh. */
export interface PerfOverlay {
  /** The corner element the text goes in. */
  element: HTMLElement;
  /** Frames drawn since the last refresh. */
  frames: number;
  /** Physics ticks run since the last refresh. */
  ticks: number;
  /** Milliseconds those ticks took. */
  tickMs: number;
  /** Milliseconds spent drawing since the last refresh. */
  renderMs: number;
  /** When the current count started, in `performance.now()` milliseconds. */
  windowStartMs: number;
}

/** Starts hidden, counting from `nowMs`. */
export function createPerfOverlay(element: HTMLElement, nowMs: number): PerfOverlay {
  element.hidden = true;
  return { element, frames: 0, ticks: 0, tickMs: 0, renderMs: 0, windowStartMs: nowMs };
}

/** F3: shows or hides the overlay. */
export function togglePerfOverlay(overlay: PerfOverlay): void {
  overlay.element.hidden = !overlay.element.hidden;
}

/** Counts one physics tick that took `ms`. */
export function recordTick(overlay: PerfOverlay, ms: number): void {
  overlay.ticks += 1;
  overlay.tickMs += ms;
}

/** Counts one drawn frame that took `renderMs`; refreshes the text every half second. */
export function recordFrame(overlay: PerfOverlay, renderMs: number, nowMs: number): void {
  overlay.frames += 1;
  overlay.renderMs += renderMs;
  const windowMs = nowMs - overlay.windowStartMs;
  if (windowMs < REFRESH_MS) {
    return;
  }
  const { frames, ticks, tickMs } = overlay;
  overlay.element.textContent = perfText(frames, windowMs, ticks, tickMs, overlay.renderMs);
  overlay.frames = 0;
  overlay.ticks = 0;
  overlay.tickMs = 0;
  overlay.renderMs = 0;
  overlay.windowStartMs = nowMs;
}

/**
 * The readout for `frames` frames over `windowMs`: frames per second, the mean
 * milliseconds per physics tick (SPEC budget 1.5 ms) and per drawn frame (8 ms).
 */
export function perfText(
  frames: number,
  windowMs: number,
  ticks: number,
  tickMs: number,
  renderMs: number,
): string {
  const fps = (frames * 1000) / windowMs;
  const msPerTick = ticks > 0 ? tickMs / ticks : 0;
  const msPerRender = frames > 0 ? renderMs / frames : 0;
  return `${fps.toFixed(0)} fps, tick ${msPerTick.toFixed(2)} ms, render ${msPerRender.toFixed(1)} ms`;
}
