// The training viewer (SPEC §11.6, M6): a crowd of translucent cars replays the same Stage A
// drops with checkpoint 1, 2, 3… of the newest training run, so you can watch the policy
// improve. The sim (a wasm Crowd) flies every car; this page only draws them and counts
// how the drops end, then moves on to the next checkpoint.
import { requireElement, setText } from '../dom.ts';
import { startGameLoop } from '../game-loop.ts';
import { FLOATS_PER_POSE } from '../render/pose-blend.ts';
import { createArena } from '../render/scene.ts';
import initWasm, { Crowd, CrowdOutcome, Game } from '../wasm/nl_wasm.js';
import { type RunEntry, loadCheckpoint, loadNewestRun } from './checkpoints.ts';
import { createCrowdView, updateCrowdView } from './crowd-view.ts';

/** Cars in the crowd (SPEC §11.6: about 64). */
const CROWD_SIZE = 64;
/** Drops use eval seeds 0…63, the same for every checkpoint, so they compare fairly. */
const FIRST_SEED = 0n;
/** How long the final picture stays before the next checkpoint starts, in seconds. */
const PAUSE_BETWEEN_S = 2.5;

/** Where the viewer is: which checkpoint, its crowd, and when it finished. */
interface ViewerState {
  run: RunEntry;
  checkpoint: number;
  crowd: Crowd | null;
  running: boolean;
  /** A checkpoint file is being fetched; nothing advances until it arrives. */
  loading: boolean;
  /** A checkpoint asked for while another was loading (the newest request wins). */
  queued: number | null;
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
  const arena = createArena(requireElement('viewer'));
  arena.camera.position.set(0, 70, 105);
  arena.camera.lookAt(0, 2, 0);
  const state: ViewerState = {
    run,
    checkpoint: 0,
    crowd: null,
    running: false,
    loading: false,
    queued: null,
    error: null,
    finishedAtMs: null,
  };
  await showCheckpoint(state, 0);
  // Wheels per car, read from the sim rather than assumed.
  const wheelPoses = state.crowd ? (state.crowd as Crowd).wheel_poses().length : 0;
  const wheelCount = wheelPoses / (CROWD_SIZE * FLOATS_PER_POSE);
  const view = createCrowdView(arena.scene, CROWD_SIZE, wheelCount);
  window.addEventListener('keydown', (event) => {
    if (event.code === 'ArrowRight' || event.code === 'KeyN') {
      void showCheckpoint(state, state.checkpoint + 1);
    }
    if (event.code === 'ArrowLeft' || event.code === 'KeyP') {
      void showCheckpoint(state, state.checkpoint - 1);
    }
  });

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
    },
  });
}

/** Starts checkpoint `index` (wrapping round) on the same drops. */
async function showCheckpoint(state: ViewerState, index: number): Promise<void> {
  if (state.loading) {
    // Remember the newest request; it runs as soon as the current load ends.
    state.queued = index;
    return;
  }
  const total = state.run.checkpoints.length;
  const wrapped = ((index % total) + total) % total;
  const entry = state.run.checkpoints[wrapped];
  if (!entry) {
    return;
  }
  state.loading = true;
  try {
    const policyJson = await loadCheckpoint(state.run, entry);
    // Build the new crowd first, so a bad file never leaves a freed crowd behind.
    const crowd = new Crowd(policyJson, 'realistic', FIRST_SEED, CROWD_SIZE);
    state.crowd?.free();
    state.crowd = crowd;
    state.checkpoint = wrapped;
    state.running = true;
    state.finishedAtMs = null;
    state.error = null;
  } catch (error) {
    // Keep showing the last crowd; auto-advance retries after the usual pause.
    state.error = `Checkpoint ${wrapped + 1}: ${String(error)}`;
    state.finishedAtMs = performance.now();
  } finally {
    state.loading = false;
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
    void showCheckpoint(state, state.checkpoint + 1);
  }
}

/** "Run stage-a-004 · checkpoint 3 of 8 · 18.0M steps · landed 40 · crashed 20 · flying 4". */
function statusText(state: ViewerState, outcomes: Uint8Array): string {
  if (state.error) {
    return state.error;
  }
  const entry = state.run.checkpoints[state.checkpoint];
  const millions = ((entry?.steps ?? 0) / 1e6).toFixed(1);
  const total = state.run.checkpoints.length;
  return [
    `Run ${state.run.run_id}`,
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
