// Keeps the viewer smooth on a slow GPU (plan: viewer smoothness). The crowd is many
// see-through cars, so drawing cost scales with the pixels. This watches how long frames
// really take and, when they run slow, steps the render resolution down. It only ever steps
// down, so the picture never flips back and forth between sharp and soft. Pure.

/** Frames averaged before each decision. */
export const QUALITY_WINDOW_FRAMES = 60;
/** A frame slower than this on average (about 40 fps) triggers a step down, in ms. */
const SLOW_FRAME_MS = 25;
/** A gap longer than this (a hidden tab, a debugger pause) is not slowness the picture can fix,
 * so it is not counted. */
const MAX_COUNTED_FRAME_MS = 250;
/** How much one step lowers the pixel ratio. */
const RATIO_STEP = 0.25;
/** The pixel ratio never goes below this (at 0.75 the picture is soft but readable). */
export const MIN_PIXEL_RATIO = 0.75;

/** What the controller has counted since its last decision. */
interface QualityState {
  ratio: number;
  frames: number;
  elapsedMs: number;
}

/** Starts at `ratio`, with nothing counted. */
export function newQuality(ratio: number): QualityState {
  return { ratio, frames: 0, elapsedMs: 0 };
}

/**
 * Counts one frame that took `frameMs` from the last frame's start. After a full window
 * returns the lower ratio to switch to when frames were slow; otherwise null.
 */
export function recordQualityFrame(state: QualityState, frameMs: number): number | null {
  if (frameMs > MAX_COUNTED_FRAME_MS) {
    return null;
  }
  state.frames += 1;
  state.elapsedMs += frameMs;
  if (state.frames < QUALITY_WINDOW_FRAMES) {
    return null;
  }
  const meanMs = state.elapsedMs / state.frames;
  state.frames = 0;
  state.elapsedMs = 0;
  if (meanMs <= SLOW_FRAME_MS || state.ratio <= MIN_PIXEL_RATIO) {
    return null;
  }
  state.ratio = Math.max(MIN_PIXEL_RATIO, state.ratio - RATIO_STEP);
  return state.ratio;
}
