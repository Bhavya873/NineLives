// The training viewer (SPEC §11.6): every exported checkpoint of the newest training run
// flies at once, one labelled row of translucent cars per checkpoint, all on the same Stage A
// drops, so you can see the policy improve from row to row. The sim (one wasm Crowd per
// checkpoint) flies every car; this page only draws them and counts how the drops end.
// The drops land on the flat arena or the obstacle arena (O key, the button or `?arena=obstacles`).
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
import { createCrowdView, updateCrowdRow } from './crowd-view.ts';

/** Cars per checkpoint: 8 checkpoints × 8 cars is SPEC §11.6's crowd of about 64. */
const CARS_PER_CHECKPOINT = 8;
/** How long the final picture stays before the next round of drops, in seconds. */
const PAUSE_BETWEEN_S = 2.5;

/** One checkpoint's crowd and whether its drops are still flying. */
interface Row {
  crowd: Crowd;
  running: boolean;
}

/** Where the viewer is: the run, its checkpoints' policies and the current round. */
interface ViewerState {
  run: RunEntry;
  /** Each checkpoint's policy file text, in training order. */
  policies: string[];
  arena: ArenaKind;
  /** Round 0, 1, 2…: each round flies a new block of drops. */
  round: number;
  rows: Row[];
  finishedAtMs: number | null;
}

async function start(): Promise<void> {
  await initWasm();
  if (Game.floats_per_pose() !== FLOATS_PER_POSE) {
    throw new Error('The wasm build and the viewer disagree on the pose layout');
  }
  const status = requireElement('viewer-status');
  const board = requireElement('viewer-board');
  const toggle = requireElement('arena-toggle');
  const run = await loadNewestRun();
  const policies = await Promise.all(run.checkpoints.map((entry) => loadCheckpoint(run, entry)));
  const arena = createArena(requireElement('viewer'));
  const arenaView = createArenaView(arena.scene, arena.camera, arena.ground);
  const state: ViewerState = {
    run,
    policies,
    arena: arenaFromUrl(window.location.search),
    round: 0,
    rows: [],
    finishedAtMs: null,
  };
  startRound(state, state.arena, 0);
  // Wheels per car, read from the sim rather than assumed.
  const wheelPoses = state.rows[0]?.crowd.wheel_poses().length ?? 0;
  const wheelCount = wheelPoses / (CARS_PER_CHECKPOINT * FLOATS_PER_POSE);
  const view = createCrowdView(arena.scene, rowLabels(run), CARS_PER_CHECKPOINT, wheelCount);
  const switchArena = () => startRound(state, otherArena(state.arena), state.round);
  window.addEventListener('keydown', (event) => {
    if (event.code === 'KeyO') {
      switchArena();
    }
  });
  toggle.addEventListener('click', switchArena);

  startGameLoop(Game.physics_dt_s(), {
    tick: () => {
      for (const row of state.rows) {
        if (row.running) {
          row.running = row.crowd.step();
        }
      }
    },
    render: () => {
      if (arenaView.kind !== state.arena) {
        // Read the shapes only when the arena changes: each call copies them out of wasm.
        showArena(arenaView, state.arena, state.rows[0]?.crowd.course_shapes());
        view.spreadOnGrid = state.arena === 'flat';
        setText(toggle, `Switch to the ${arenaLabel(otherArena(state.arena)).toLowerCase()}`);
      }
      const lines: string[] = [];
      state.rows.forEach((row, index) => {
        const outcomes = row.crowd.outcomes();
        updateCrowdRow(view, index, row.crowd.chassis_poses(), row.crowd.wheel_poses(), outcomes);
        lines.push(
          `${rowLabels(state.run)[index]} · landed ${count(outcomes, CrowdOutcome.Landed)}/${CARS_PER_CHECKPOINT}`,
        );
      });
      arena.renderer.render(arena.scene, arena.camera);
      setText(status, statusText(state));
      setText(board, lines.join('\n'));
      nextRoundWhenDone(state);
    },
  });

  Object.assign(window, {
    ninelivesViewer: {
      checkpointCount: () => state.rows.length,
      landed: () => totalLanded(state),
      arena: () => state.arena,
      courseShapeCount: () => arenaView.shapeCount,
    },
  });
}

/** Builds every checkpoint's crowd for round `round` in `arena`, replacing the old ones.
 * On the flat arena every row flies the same drops, so rows compare fairly; on the shared
 * obstacle course each row gets its own drops, so the cars do not sit on top of each other. */
function startRound(state: ViewerState, arena: ArenaKind, round: number): void {
  const rows = state.policies.length;
  const roundSeed = BigInt(round * CARS_PER_CHECKPOINT * (arena === 'flat' ? 1 : rows));
  // Build the new crowds first, so a bad file never leaves freed crowds behind.
  const crowds = state.policies.map((policyJson, index) => {
    const rowSeed = arena === 'flat' ? 0n : BigInt(index * CARS_PER_CHECKPOINT);
    return newCrowd(arena, policyJson, roundSeed + rowSeed, CARS_PER_CHECKPOINT);
  });
  for (const row of state.rows) {
    row.crowd.free();
  }
  state.rows = crowds.map((crowd) => ({ crowd, running: true }));
  state.arena = arena;
  state.round = round;
  state.finishedAtMs = null;
  // Keep the URL in step so a reload stays in this arena.
  const search = searchWithArena(window.location.search, arena);
  window.history.replaceState(null, '', `${window.location.pathname}${search}`);
}

/** Once every drop has ended, waits a moment, then flies the next round of drops. */
function nextRoundWhenDone(state: ViewerState): void {
  if (state.rows.some((row) => row.running)) {
    return;
  }
  const nowMs = performance.now();
  if (state.finishedAtMs === null) {
    state.finishedAtMs = nowMs;
  } else if (nowMs - state.finishedAtMs > PAUSE_BETWEEN_S * 1000) {
    startRound(state, state.arena, state.round + 1);
  }
}

/** Each checkpoint's label, e.g. "#3 · 16M steps". */
function rowLabels(run: RunEntry): string[] {
  return run.checkpoints.map((entry, index) => {
    const millions = Number((entry.steps / 1e6).toFixed(1));
    return `#${index + 1} · ${millions}M steps`;
  });
}

/** "Flat arena · run stage-a-004 · 8 checkpoints at once · round 1". */
function statusText(state: ViewerState): string {
  return [
    arenaLabel(state.arena),
    `run ${state.run.run_id}`,
    `${state.rows.length} checkpoints at once`,
    `round ${state.round + 1}`,
  ].join(' · ');
}

/** Cars landed so far, over every checkpoint. */
function totalLanded(state: ViewerState): number {
  let total = 0;
  for (const row of state.rows) {
    total += count(row.crowd.outcomes(), CrowdOutcome.Landed);
  }
  return total;
}

/** How many of `outcomes` equal `outcome`. */
function count(outcomes: Uint8Array, outcome: CrowdOutcome): number {
  let total = 0;
  for (const value of outcomes) {
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
