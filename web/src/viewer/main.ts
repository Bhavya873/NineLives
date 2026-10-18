// The training viewer (SPEC §11.6, M6): a crowd of translucent cars replays the same Stage A
// drops with checkpoint 1, 2, 3… of the newest training run, so you can watch the policy
// improve. The sim (a wasm Crowd) flies every car; this page only draws them and counts
// how the drops end, then moves on to the next checkpoint. The drops land on the flat
// arena or, from M7, the obstacle arena (O key, the on-screen button or `?arena=obstacles`).
import { requireElement, setText } from '../dom.ts';
import { startGameLoop } from '../game-loop.ts';
import { FLOATS_PER_POSE } from '../render/pose-blend.ts';
import { createArena } from '../render/scene.ts';
import initWasm, { type Crowd, CrowdOutcome, Game } from '../wasm/nl_wasm.js';
import {
  type ArenaKind,
  arenaFromUrl,
  arenaLabel,
  newCrowd,
  otherArena,
  searchWithArena,
} from './arena-choice.ts';
import { createArenaView, showArena } from './arena-view.ts';
import { type RunEntry, loadCheckpoint, loadNewestRun } from './checkpoints.ts';
import { createCrowdView, updateCrowdView } from './crowd-view.ts';

/** Cars in the crowd (SPEC §11.6: about 64). */
const CROWD_SIZE = 64;
/** Drops use eval seeds 0…63, the same for every checkpoint, so they compare fairly. */
const FIRST_SEED = 0n;
/** How long the final picture stays before the next checkpoint starts, in seconds. */
const PAUSE_BETWEEN_S = 2.5;

/** A checkpoint to show, and the arena to fly it in. */
interface Request {
  index: number;
  arena: ArenaKind;
}

/** Where the viewer is: which checkpoint, its crowd, and when it finished. */
interface ViewerState {
  run: RunEntry;
  /** The arena the current crowd flies in (set only once that crowd exists). */
  arena: ArenaKind;
  checkpoint: number;
  crowd: Crowd | null;
  running: boolean;
  /** The request whose checkpoint file is being fetched; nothing advances meanwhile. */
  loading: Request | null;
  /** A request made while another was loading (the newest wins). */
  queued: Request | null;
  /** Why the last checkpoint could not be shown, if it could not. */
  error: string | null;
  finishedAtMs: number | null;
}

async function start(): Promise<void> {
  await initWasm();
  if (Game.floats_per_pose() !== FLOATS_PER_POSE) {
    throw new Error('The wasm build and the viewer disagree on the pose layout');
  }
  const status = requireElement('viewer-status');
  const run = await loadNewestRun();
  const toggle = requireElement('arena-toggle');
  const arena = createArena(requireElement('viewer'));
  const arenaView = createArenaView(arena.scene, arena.camera, arena.ground);
  const firstArena = arenaFromUrl(window.location.search);
  const state: ViewerState = {
    run,
    arena: firstArena,
    checkpoint: 0,
    crowd: null,
    running: false,
    loading: null,
    queued: null,
    error: null,
    finishedAtMs: null,
  };
  await showCheckpoint(state, { index: 0, arena: firstArena });
  // Wheels per car, read from the sim rather than assumed.
  const wheelPoses = state.crowd ? (state.crowd as Crowd).wheel_poses().length : 0;
  const wheelCount = wheelPoses / (CROWD_SIZE * FLOATS_PER_POSE);
  const view = createCrowdView(arena.scene, CROWD_SIZE, wheelCount);
  window.addEventListener('keydown', (event) => {
    if (event.code === 'ArrowRight' || event.code === 'KeyN') {
      stepCheckpoint(state, 1);
    }
    if (event.code === 'ArrowLeft' || event.code === 'KeyP') {
      stepCheckpoint(state, -1);
    }
    if (event.code === 'KeyO') {
      switchArena(state);
    }
  });
  toggle.addEventListener('click', () => switchArena(state));

  startGameLoop(Game.physics_dt_s(), {
    tick: () => {
      if (state.crowd && state.running) {
        state.running = state.crowd.step();
      }
    },
    render: () => {
      const crowd = state.crowd;
      if (!crowd) {
        return;
      }
      if (arenaView.kind !== state.arena) {
        // Read the shapes only when the arena changes: each call copies them out of wasm.
        showArena(arenaView, state.arena, crowd.course_shapes());
        view.spreadOnGrid = state.arena === 'flat';
        setText(toggle, `Switch to the ${arenaLabel(otherArena(state.arena)).toLowerCase()}`);
      }
      const outcomes = crowd.outcomes();
      updateCrowdView(view, crowd.chassis_poses(), crowd.wheel_poses(), outcomes);
      arena.renderer.render(arena.scene, arena.camera);
      setText(status, statusText(state, outcomes));
      advanceWhenDone(state);
    },
  });

  Object.assign(window, {
    ninelivesViewer: {
      checkpoint: () => state.checkpoint,
      checkpointCount: () => state.run.checkpoints.length,
      landed: () => count(state.crowd?.outcomes(), CrowdOutcome.Landed),
      arena: () => state.arena,
      courseShapeCount: () => arenaView.shapeCount,
    },
  });
}

/** The newest request: the one queued, else the one loading, else what is shown. */
function latestRequest(state: ViewerState): Request {
  return state.queued ?? state.loading ?? { index: state.checkpoint, arena: state.arena };
}

/** Moves `step` checkpoints on from the newest request, in that request's arena. */
function stepCheckpoint(state: ViewerState, step: number): void {
  const latest = latestRequest(state);
  void showCheckpoint(state, { index: latest.index + step, arena: latest.arena });
}

/** Replays the newest requested checkpoint in the other arena. */
function switchArena(state: ViewerState): void {
  const latest = latestRequest(state);
  void showCheckpoint(state, { index: latest.index, arena: otherArena(latest.arena) });
}

/** Starts checkpoint `request.index` (wrapping round) on the same drops in
 * `request.arena`. The arena and the URL change only once the new crowd exists, so a
 * failed load leaves everything as it was. */
async function showCheckpoint(state: ViewerState, request: Request): Promise<void> {
  if (state.loading) {
    // Remember the newest request; it runs as soon as the current load ends.
    state.queued = request;
    return;
  }
  const total = state.run.checkpoints.length;
  const wrapped = ((request.index % total) + total) % total;
  const entry = state.run.checkpoints[wrapped];
  if (!entry) {
    return;
  }
  state.loading = { index: wrapped, arena: request.arena };
  try {
    const policyJson = await loadCheckpoint(state.run, entry);
    // Build the new crowd first, so a bad file never leaves a freed crowd behind.
    const crowd = newCrowd(request.arena, policyJson, FIRST_SEED, CROWD_SIZE);
    state.crowd?.free();
    state.crowd = crowd;
    state.arena = request.arena;
    state.checkpoint = wrapped;
    state.running = true;
    state.finishedAtMs = null;
    state.error = null;
    // Keep the URL in step so a reload stays in this arena.
    const search = searchWithArena(window.location.search, request.arena);
    window.history.replaceState(null, '', `${window.location.pathname}${search}`);
  } catch (error) {
    // Keep showing the last crowd; auto-advance retries after the usual pause.
    state.error = `Checkpoint ${wrapped + 1}: ${String(error)}`;
    state.finishedAtMs = performance.now();
  } finally {
    state.loading = null;
  }
  const queued = state.queued;
  state.queued = null;
  if (queued !== null) {
    await showCheckpoint(state, queued);
  }
}

/** Once every drop has ended, waits a moment, then moves to the next checkpoint. */
function advanceWhenDone(state: ViewerState): void {
  if (state.running || state.loading) {
    return;
  }
  const nowMs = performance.now();
  if (state.finishedAtMs === null) {
    state.finishedAtMs = nowMs;
  } else if (nowMs - state.finishedAtMs > PAUSE_BETWEEN_S * 1000) {
    stepCheckpoint(state, 1);
  }
}

/** "Obstacle arena · run stage-a-004 · checkpoint 3 of 8 · 18.0M steps · landed 40 · crashed 20 · flying 4". */
function statusText(state: ViewerState, outcomes: Uint8Array): string {
  if (state.error) {
    return state.error;
  }
  const entry = state.run.checkpoints[state.checkpoint];
  const millions = ((entry?.steps ?? 0) / 1e6).toFixed(1);
  const total = state.run.checkpoints.length;
  return [
    arenaLabel(state.arena),
    `run ${state.run.run_id}`,
    `checkpoint ${state.checkpoint + 1} of ${total}`,
    `${millions}M steps`,
    `landed ${count(outcomes, CrowdOutcome.Landed)}`,
    `crashed ${count(outcomes, CrowdOutcome.Crashed)}`,
    `flying ${count(outcomes, CrowdOutcome.Running)}`,
  ].join(' · ');
}

/** How many of `outcomes` equal `outcome`. */
function count(outcomes: Uint8Array | undefined, outcome: CrowdOutcome): number {
  let total = 0;
  for (const value of outcomes ?? []) {
    if (value === outcome) {
      total += 1;
    }
  }
  return total;
}

start().catch((error: unknown) => {
  console.error(error);
  const status = document.getElementById('viewer-status');
  if (status) {
    status.textContent = String(error);
  }
});
