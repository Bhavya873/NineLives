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

/** Where the viewer is: which checkpoint, its crowd, and when it finished. */
interface ViewerState {
  run: RunEntry;
  /** The arena asked for (the next crowd is built there). */
  arena: ArenaKind;
  /** The arena the current crowd flies in. */
  crowdArena: ArenaKind;
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
  const toggle = requireElement('arena-toggle');
  const arena = createArena(requireElement('viewer'));
  const arenaView = createArenaView(arena.scene, arena.camera, arena.ground);
  const firstArena = arenaFromUrl(window.location.search);
  const state: ViewerState = {
    run,
    arena: firstArena,
    crowdArena: firstArena,
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
      if (arenaView.kind !== state.crowdArena) {
        // Read the shapes only when the arena changes: each call copies them out of wasm.
        showArena(arenaView, state.crowdArena, crowd.course_shapes());
        view.spreadOnGrid = state.crowdArena === 'flat';
        setText(toggle, `Switch to the ${arenaLabel(otherArena(state.crowdArena)).toLowerCase()}`);
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
      arena: () => state.crowdArena,
      courseShapeCount: () => arenaView.shapeCount,
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
    const arenaKind = state.arena;
    const crowd = newCrowd(arenaKind, policyJson, FIRST_SEED, CROWD_SIZE);
    state.crowd?.free();
    state.crowd = crowd;
    state.crowdArena = arenaKind;
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

/** Replays the current checkpoint in the other arena, and keeps the URL in step so a
 * reload stays there. */
function switchArena(state: ViewerState): void {
  state.arena = otherArena(state.arena);
  const search = searchWithArena(window.location.search, state.arena);
  window.history.replaceState(null, '', `${window.location.pathname}${search}`);
  void showCheckpoint(state, state.checkpoint);
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

/** "Obstacle arena · run stage-a-004 · checkpoint 3 of 8 · 18.0M steps · landed 40 · crashed 20 · flying 4". */
function statusText(state: ViewerState, outcomes: Uint8Array): string {
  if (state.error) {
    return state.error;
  }
  const entry = state.run.checkpoints[state.checkpoint];
  const millions = ((entry?.steps ?? 0) / 1e6).toFixed(1);
  const total = state.run.checkpoints.length;
  return [
    arenaLabel(state.crowdArena),
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
