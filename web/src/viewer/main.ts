// The training viewer (SPEC §11.6): every exported checkpoint of the newest training run
// flies at once, one labelled row of translucent cars per checkpoint, all on the same
// drops, so you can see the policy improve from row to row. The sim (one wasm Crowd per
// checkpoint, every car flown by the Hybrid) flies every car; this page only draws them and
// counts how the episodes end. The cars fly Stage A drops on the flat arena or the obstacle
// arena (O key, the button or `?arena=obstacles`), or Stage D, Stage F and the Gauntlet
// (the stage picker or `?arena=driving|flight|gauntlet`).
import { requireElement, setText } from '../dom.ts';
import { startGameLoop } from '../game-loop.ts';
import { FLOATS_PER_POSE } from '../render/pose-blend.ts';
import { createArena } from '../render/scene.ts';
import initWasm, { type Crowd, CrowdOutcome, Game } from '../wasm/nl_wasm.js';
import {
  ARENAS,
  type ArenaKind,
  arenaFromUrl,
  arenaLabel,
  followsCars,
  newCrowd,
  otherArena,
  rowsShareSeeds,
  searchWithArena,
} from './arena-choice.ts';
import { createArenaView, followCars, showArena } from './arena-view.ts';
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
  const toggle = requireElement('arena-toggle');
  const picker = requireElement('stage-select') as HTMLSelectElement;
  for (const kind of ARENAS) {
    picker.add(new Option(arenaLabel(kind), kind));
  }
  const run = await loadNewestRun();
  const board = createBoard(requireElement('viewer-board'), run);
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
  picker.addEventListener('change', () => {
    const kind = ARENAS.find((arenaKind) => arenaKind === picker.value);
    if (kind) {
      startRound(state, kind, state.round);
    }
  });
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
        picker.value = state.arena;
      }
      const centre = { x: 0, z: 0, cars: 0 };
      state.rows.forEach((row, index) => {
        const outcomes = row.crowd.outcomes();
        const chassis = row.crowd.chassis_poses();
        updateCrowdRow(view, index, chassis, row.crowd.wheel_poses(), outcomes);
        addRunningCars(centre, chassis, outcomes);
        const cells = board[index];
        if (cells) {
          setText(cells.landed, String(count(outcomes, CrowdOutcome.Landed)));
          setText(cells.crashed, String(count(outcomes, CrowdOutcome.Crashed)));
        }
      });
      if (followsCars(state.arena) && centre.cars > 0) {
        followCars(arenaView, centre.x / centre.cars, centre.z / centre.cars);
      }
      arena.renderer.render(arena.scene, arena.camera);
      setText(status, statusText(state));
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
 * On the flat arena and the generated courses every row flies the same seeds (and so the same
 * course), so rows compare fairly; on the shared obstacle arena and the Gauntlet each row gets
 * its own seeds, so the cars do not sit on top of each other. */
function startRound(state: ViewerState, arena: ArenaKind, round: number): void {
  const rows = state.policies.length;
  const shared = rowsShareSeeds(arena);
  const roundSeed = BigInt(round * CARS_PER_CHECKPOINT * (shared ? 1 : rows));
  // Build the new crowds first, so a bad file never leaves freed crowds behind.
  const crowds = state.policies.map((policyJson, index) => {
    const rowSeed = shared ? 0n : BigInt(index * CARS_PER_CHECKPOINT);
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

/** Adds the position of every car still flying or driving to `centre`, for the camera. */
function addRunningCars(
  centre: { x: number; z: number; cars: number },
  chassis: Float32Array,
  outcomes: Uint8Array,
): void {
  outcomes.forEach((outcome, car) => {
    if (outcome === CrowdOutcome.Running) {
      centre.x += chassis[car * FLOATS_PER_POSE] ?? 0;
      centre.z += chassis[car * FLOATS_PER_POSE + 2] ?? 0;
      centre.cars += 1;
    }
  });
}

/** A checkpoint's training steps in millions, e.g. "16M". */
function stepsText(steps: number): string {
  return `${Number((steps / 1e6).toFixed(1))}M`;
}

/** Each checkpoint's label over its cars, e.g. "#3 (16M)". */
function rowLabels(run: RunEntry): string[] {
  return run.checkpoints.map((entry, index) => `#${index + 1} (${stepsText(entry.steps)})`);
}

/** The scoreboard cells a checkpoint's counts go in. */
interface BoardCells {
  landed: HTMLElement;
  crashed: HTMLElement;
}

/** Fills `table` with one row per checkpoint and returns each row's count cells. */
function createBoard(table: HTMLElement, run: RunEntry): BoardCells[] {
  const header = table.insertAdjacentElement('beforeend', document.createElement('tr'));
  for (const title of ['Checkpoint', 'Steps', 'Landed', 'Crashed']) {
    const cell = document.createElement('th');
    cell.textContent = title;
    header?.appendChild(cell);
  }
  return run.checkpoints.map((entry, index) => {
    const row = document.createElement('tr');
    const cells = [`#${index + 1}`, stepsText(entry.steps), '0', '0'].map((text) => {
      const cell = document.createElement('td');
      cell.textContent = text;
      row.appendChild(cell);
      return cell;
    });
    table.appendChild(row);
    return { landed: cells[2] as HTMLElement, crashed: cells[3] as HTMLElement };
  });
}

/** "Run flight-r4: 8 checkpoints at once, 8 drops each, flat arena, round 1". */
function statusText(state: ViewerState): string {
  const arena = arenaLabel(state.arena).toLowerCase();
  const checkpoints = `${state.rows.length} checkpoints at once`;
  return `Run ${state.run.run_id}: ${checkpoints}, ${CARS_PER_CHECKPOINT} drops each, ${arena}, round ${state.round + 1}`;
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
