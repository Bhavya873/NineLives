// The fixed-step game loop (SPEC §11.2).
// The browser draws frames at whatever rate it likes; the physics always steps at
// exactly 120 Hz. An "accumulator" collects real time and spends it in 1/120 s ticks.
// The renderer then blends the last two physics states so motion looks smooth.

/** Never run more than this many ticks in one frame; drop the rest (SPEC §11.2). */
const MAX_TICKS_PER_FRAME = 8;

/** What the loop needs from the rest of the game. */
export interface LoopCallbacks {
  /** Advance the physics by one fixed step. */
  tick: () => void;
  /** Draw a frame. `blend` is 0..1: how far we are between the previous and latest tick. */
  render: (blend: number) => void;
}

/**
 * Splits `elapsedS` of real time into whole physics ticks.
 * Returns how many ticks to run now and the time left over for next frame.
 * Kept separate from the browser so it can be unit tested.
 */
export function planTicks(
  leftoverS: number,
  elapsedS: number,
  tickS: number,
): { ticks: number; leftoverS: number } {
  let available = leftoverS + elapsedS;
  let ticks = Math.floor(available / tickS);
  if (ticks > MAX_TICKS_PER_FRAME) {
    // The tab was in the background or the machine is too slow: catch up only a
    // little and forget the rest, instead of freezing while we replay seconds of time.
    ticks = MAX_TICKS_PER_FRAME;
    available = ticks * tickS;
  }
  return { ticks, leftoverS: available - ticks * tickS };
}

/** Starts the loop with `requestAnimationFrame`. Runs until the page closes. */
export function startGameLoop(tickS: number, callbacks: LoopCallbacks): void {
  let leftoverS = 0;
  let lastFrameMs: number | null = null;

  function onFrame(nowMs: number): void {
    const elapsedS = lastFrameMs === null ? 0 : (nowMs - lastFrameMs) / 1000;
    lastFrameMs = nowMs;

    const plan = planTicks(leftoverS, elapsedS, tickS);
    for (let tick = 0; tick < plan.ticks; tick += 1) {
      callbacks.tick();
    }
    leftoverS = plan.leftoverS;

    callbacks.render(leftoverS / tickS);
    requestAnimationFrame(onFrame);
  }

  requestAnimationFrame(onFrame);
}
