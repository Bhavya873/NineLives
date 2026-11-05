// A stage worker (plan: all-stages viewer): steps one stage's eight checkpoint crowds in real
// time on its own thread and posts their poses, so the five stages never block the page's
// frames. It owns the rounds too: when every drop has ended it waits a moment, then builds the
// next block of drops. The page only draws what it is sent (stage-protocol.ts).
import { planTicks } from '../game/game-loop.ts';
import { FLOATS_PER_POSE } from '../render/pose-blend.ts';
import initWasm, { type Crowd, Game } from '../wasm/nl_wasm.js';
import { newCrowd } from './arena-choice.ts';
import {
  type FromStageWorker,
  type RowSnapshot,
  type StageInit,
  type ToStageWorker,
  rowSeeds,
} from './stage-protocol.ts';

/** How often the loop wakes, in ms; each wake steps the ticks real time has earned. */
const LOOP_MS = 4;
/** Poses are posted at most this often, in ms (a little under one 60 Hz frame). */
const POST_MS = 8;

/** One checkpoint's crowd and whether its drops still fly. */
interface WorkerRow {
  crowd: Crowd;
  running: boolean;
  /** True once the final poses of a finished row have been posted. */
  posted: boolean;
}

interface Stage {
  init: StageInit;
  tickS: number;
  round: number;
  rows: WorkerRow[];
  ticks: number;
  /** Real time earned but not yet stepped, in s. */
  leftoverS: number;
  lastLoopMs: number | null;
  lastPostMs: number;
  finishedAtMs: number | null;
  /** Wall-clock ms spent stepping, and ticks stepped, since the last post. */
  stepMs: number;
  stepTicks: number;
}

let stage: Stage | null = null;
let running = true;
let timer: ReturnType<typeof setTimeout> | null = null;

function post(message: FromStageWorker, transfer: Transferable[] = []): void {
  (self as unknown as Worker).postMessage(message, transfer);
}

self.onmessage = (event: MessageEvent<ToStageWorker>) => {
  const message = event.data;
  if (message.type === 'init') {
    start(message).catch((error: unknown) => post({ type: 'error', message: String(error) }));
  } else {
    running = message.on;
    if (stage) {
      // Time spent paused is not owed to the cars.
      stage.lastLoopMs = null;
      stage.leftoverS = 0;
    }
  }
};

async function start(init: StageInit): Promise<void> {
  await initWasm();
  if (Game.floats_per_pose() !== FLOATS_PER_POSE) {
    throw new Error('The wasm build and the viewer disagree on the pose layout');
  }
  stage = {
    init,
    tickS: Game.physics_dt_s(),
    round: -1,
    rows: [],
    ticks: 0,
    leftoverS: 0,
    lastLoopMs: null,
    lastPostMs: 0,
    finishedAtMs: null,
    stepMs: 0,
    stepTicks: 0,
  };
  startRound(stage, 0);
  loop();
}

/** Builds every checkpoint's crowd for `round`, replacing the old ones, and posts the round
 * and the drops' start poses. */
function startRound(current: Stage, round: number): void {
  const { init } = current;
  const seeds = rowSeeds(
    init.firstSeed,
    init.shared,
    round,
    init.policies.length,
    init.carsPerCheckpoint,
  );
  // Build the new crowds first, so a bad file never leaves freed crowds behind.
  const crowds = init.policies.map((policy, index) =>
    newCrowd(init.arena, policy, seeds[index] ?? 0n, init.carsPerCheckpoint),
  );
  for (const row of current.rows) {
    row.crowd.free();
  }
  current.rows = crowds.map((crowd) => ({ crowd, running: true, posted: false }));
  current.round = round;
  current.ticks = 0;
  current.finishedAtMs = null;
  const first = crowds[0];
  const wheelPoses = first?.wheel_poses().length ?? 0;
  post({
    type: 'round',
    round,
    shapes: first?.course_shapes() ?? null,
    wheelsPerCar: wheelPoses / (init.carsPerCheckpoint * FLOATS_PER_POSE),
  });
  postSnapshot(current, performance.now());
}

/** Posts the poses of every row that moved (or just finished) since the last post. */
function postSnapshot(current: Stage, nowMs: number): void {
  const rows: (RowSnapshot | null)[] = [];
  const transfer: Transferable[] = [];
  for (const row of current.rows) {
    if (!row.running && row.posted) {
      rows.push(null);
      continue;
    }
    const snapshot = {
      chassis: row.crowd.chassis_poses(),
      wheels: row.crowd.wheel_poses(),
      outcomes: row.crowd.outcomes(),
    };
    transfer.push(snapshot.chassis.buffer, snapshot.wheels.buffer, snapshot.outcomes.buffer);
    rows.push(snapshot);
    row.posted = !row.running;
  }
  post(
    {
      type: 'snap',
      round: current.round,
      simTimeS: current.ticks * current.tickS,
      rows,
      running: current.rows.some((row) => row.running),
      stepMs: current.stepTicks > 0 ? current.stepMs / current.stepTicks : 0,
    },
    transfer,
  );
  current.stepMs = 0;
  current.stepTicks = 0;
  current.lastPostMs = nowMs;
}

function loop(): void {
  timer = setTimeout(loop, LOOP_MS);
  const current = stage;
  if (!current || !running) {
    return;
  }
  try {
    const nowMs = performance.now();
    const elapsedS = current.lastLoopMs === null ? 0 : (nowMs - current.lastLoopMs) / 1000;
    current.lastLoopMs = nowMs;
    if (current.finishedAtMs !== null) {
      if (nowMs - current.finishedAtMs > current.init.pauseBetweenMs) {
        startRound(current, current.round + 1);
      }
      return;
    }
    const plan = planTicks(current.leftoverS, elapsedS, current.tickS);
    current.leftoverS = plan.leftoverS;
    if (plan.ticks > 0) {
      const stepStartMs = performance.now();
      for (let tick = 0; tick < plan.ticks; tick += 1) {
        for (const row of current.rows) {
          if (row.running) {
            row.running = row.crowd.step();
          }
        }
        current.ticks += 1;
      }
      current.stepMs += performance.now() - stepStartMs;
      current.stepTicks += plan.ticks;
    }
    const anyRunning = current.rows.some((row) => row.running);
    if (plan.ticks > 0 && (!anyRunning || nowMs - current.lastPostMs >= POST_MS)) {
      postSnapshot(current, nowMs);
    }
    if (!anyRunning) {
      current.finishedAtMs = nowMs;
    }
  } catch (error) {
    post({ type: 'error', message: String(error) });
    if (timer !== null) {
      clearTimeout(timer);
    }
  }
}
