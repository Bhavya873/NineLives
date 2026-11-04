// One stage of the training viewer on the page (plan: all-stages viewer): the worker that
// steps its eight checkpoint crowds, the poses it has posted, the course meshes, the pooled
// cars and the camera. The page draws every panel into its own rectangle of one canvas.
import { Group, PerspectiveCamera, type Scene } from 'three';

import { FLOATS_PER_POSE } from '../render/pose-blend.ts';
import { buildCourseView, disposeCourseView } from '../render/course-view.ts';
import { CrowdOutcome } from '../wasm/nl_wasm.js';
import { type ArenaKind, followsCars, rowsShareSeeds, spreadsOnGrid } from './arena-choice.ts';
import {
  type CrowdView,
  type LabelMode,
  type RowPoses,
  beginCrowdFrame,
  createCrowdView,
  endCrowdFrame,
  updateCrowdRow,
} from './crowd-view.ts';
import { pickCrowdLeader } from './leader.ts';
import { aimFixedCamera, followCars, fovForAspect } from './panel-camera.ts';
import { advanceClock, newSnapshotClock, onSnapshot } from './snapshot-clock.ts';
import type {
  FromStageWorker,
  StageInit,
  StageRound,
  StageSnapshot,
  ToStageWorker,
} from './stage-protocol.ts';

/** How the camera of a course panel picks what to follow. */
export type CameraMode = 'crowd' | 'leader';

/** One stage panel. */
export interface StagePanel {
  arena: ArenaKind;
  /** The three.js layer the panel's cars and course are on. */
  layer: number;
  worker: Worker;
  camera: PerspectiveCamera;
  clock: ReturnType<typeof newSnapshotClock>;
  rowLabels: string[];
  carsPerCheckpoint: number;
  scene: Scene;
  view: CrowdView | null;
  course: Group | null;
  /** How many course shapes are drawn (0 on the flat arenas). */
  shapeCount: number;
  round: number;
  /** The poses of each checkpoint row at the last two snapshots. */
  rows: RowPoses[];
  /** True until the first snapshot of a new round has arrived. */
  restart: boolean;
  /** True while any drop of the round is still flying. */
  running: boolean;
  /** Wall-clock ms the worker spends per physics tick (from its last snapshot). */
  stepMs: number;
  /** Landed and crashed cars of each checkpoint row. */
  landed: number[];
  crashed: number[];
  /** Where the cars are, for the camera: the mean of the running cars, and the leader. */
  centre: { x: number; z: number; cars: number };
  leader: { x: number; z: number } | null;
  error: string | null;
}

/** What a panel needs to start. */
export interface StagePanelConfig {
  arena: ArenaKind;
  layer: number;
  policies: string[];
  rowLabels: string[];
  carsPerCheckpoint: number;
  firstSeed: number;
  pauseBetweenMs: number;
}

/** Starts the stage's worker and returns its (still empty) panel. */
export function createStagePanel(scene: Scene, config: StagePanelConfig): StagePanel {
  const worker = new Worker(new URL('./stage-worker.ts', import.meta.url), { type: 'module' });
  const camera = new PerspectiveCamera(60, 1, 0.1, 1500);
  camera.layers.enable(config.layer);
  aimFixedCamera(camera, config.arena);
  const panel: StagePanel = {
    arena: config.arena,
    layer: config.layer,
    worker,
    camera,
    clock: newSnapshotClock(),
    rowLabels: config.rowLabels,
    carsPerCheckpoint: config.carsPerCheckpoint,
    scene,
    view: null,
    course: null,
    shapeCount: 0,
    round: -1,
    rows: [],
    restart: true,
    running: true,
    stepMs: 0,
    landed: config.policies.map(() => 0),
    crashed: config.policies.map(() => 0),
    centre: { x: 0, z: 0, cars: 0 },
    leader: null,
    error: null,
  };
  worker.onmessage = (event: MessageEvent<FromStageWorker>) => {
    const message = event.data;
    if (message.type === 'round') {
      onRound(panel, message);
    } else if (message.type === 'snap') {
      onStageSnapshot(panel, message);
    } else {
      panel.error = message.message;
      console.error(`Stage ${config.arena}: ${message.message}`);
    }
  };
  worker.onerror = (event) => {
    panel.error = event.message;
    console.error(`Stage ${config.arena} worker: ${event.message}`);
  };
  const init: StageInit = {
    type: 'init',
    arena: config.arena,
    policies: config.policies,
    carsPerCheckpoint: config.carsPerCheckpoint,
    firstSeed: config.firstSeed,
    shared: rowsShareSeeds(config.arena),
    pauseBetweenMs: config.pauseBetweenMs,
  };
  send(panel, init);
  return panel;
}

function send(panel: StagePanel, message: ToStageWorker): void {
  panel.worker.postMessage(message);
}

/** Starts or stops the stage's stepping (a stage nobody can see does not need to run). */
export function setStageRunning(panel: StagePanel, on: boolean): void {
  send(panel, { type: 'run', on });
}

/** A new round: builds the pooled cars the first time, and the new course. */
function onRound(panel: StagePanel, message: StageRound): void {
  if (!panel.view) {
    panel.view = createCrowdView(
      panel.scene,
      panel.rowLabels,
      panel.carsPerCheckpoint,
      message.wheelsPerCar,
      panel.layer,
    );
    panel.view.spreadOnGrid = spreadsOnGrid(panel.arena);
  }
  if (panel.course) {
    panel.scene.remove(panel.course);
    disposeCourseView(panel.course);
    panel.course = null;
  }
  if (message.shapes !== null) {
    panel.course = buildCourseView(message.shapes);
    panel.course.traverse((object) => object.layers.set(panel.layer));
    panel.scene.add(panel.course);
  }
  panel.shapeCount = panel.course?.children.length ?? 0;
  panel.round = message.round;
  panel.restart = true;
}

/** New poses: the old latest become the previous, so the picture can blend between them. */
function onStageSnapshot(panel: StagePanel, message: StageSnapshot): void {
  const restart = panel.restart;
  message.rows.forEach((snapshot, index) => {
    const current = panel.rows[index];
    if (snapshot) {
      panel.rows[index] =
        !current || restart
          ? {
              previousChassis: snapshot.chassis,
              latestChassis: snapshot.chassis,
              previousWheels: snapshot.wheels,
              latestWheels: snapshot.wheels,
              outcomes: snapshot.outcomes,
            }
          : {
              previousChassis: current.latestChassis,
              latestChassis: snapshot.chassis,
              previousWheels: current.latestWheels,
              latestWheels: snapshot.wheels,
              outcomes: snapshot.outcomes,
            };
    } else if (current) {
      // The row finished: it holds still.
      current.previousChassis = current.latestChassis;
      current.previousWheels = current.latestWheels;
    }
  });
  onSnapshot(panel.clock, message.simTimeS, restart);
  panel.restart = false;
  panel.running = message.running;
  panel.stepMs = message.stepMs;
}

/** Places the panel's cars between the last two snapshots, for a frame of `realDtS` seconds.
 * `labels` says which labels show; call once per frame, before rendering the panel. */
export function updateStagePanel(panel: StagePanel, realDtS: number, labels: LabelMode): void {
  const { view } = panel;
  if (!view || panel.rows.length === 0) {
    return;
  }
  const blend = advanceClock(panel.clock, realDtS);
  view.labels = labels;
  const centre = { x: 0, z: 0, cars: 0 };
  beginCrowdFrame(view);
  panel.rows.forEach((row, index) => {
    updateCrowdRow(view, index, row, blend);
    panel.landed[index] = count(row.outcomes, CrowdOutcome.Landed);
    panel.crashed[index] = count(row.outcomes, CrowdOutcome.Crashed);
    addRunningCars(centre, row.latestChassis, row.outcomes);
  });
  endCrowdFrame(view);
  panel.centre = centre;
  const leader = pickCrowdLeader(
    panel.rows.map((row) => ({ poses: row.latestChassis, outcomes: row.outcomes })),
    FLOATS_PER_POSE,
  );
  panel.leader = leader ? { x: leader.x, z: leader.z } : null;
}

/** Aims the panel's camera: fixed on the drop arenas, trailing the cars on the courses. */
export function aimStageCamera(panel: StagePanel, mode: CameraMode): void {
  if (!followsCars(panel.arena)) {
    return;
  }
  if (mode === 'leader' && panel.leader) {
    followCars(panel.camera, panel.leader.x, panel.leader.z);
  } else if (panel.centre.cars > 0) {
    followCars(
      panel.camera,
      panel.centre.x / panel.centre.cars,
      panel.centre.z / panel.centre.cars,
    );
  }
}

/** Matches the panel's camera to the rectangle it is drawn into. */
export function setStageAspect(panel: StagePanel, width: number, height: number): void {
  const aspect = width / Math.max(1, height);
  if (Math.abs(panel.camera.aspect - aspect) > 1e-6) {
    panel.camera.aspect = aspect;
    panel.camera.fov = fovForAspect(aspect);
    panel.camera.updateProjectionMatrix();
  }
}

/** Cars landed and crashed over the panel's whole crowd. */
export function stageTotals(panel: StagePanel): { landed: number; crashed: number } {
  return {
    landed: panel.landed.reduce((sum, value) => sum + value, 0),
    crashed: panel.crashed.reduce((sum, value) => sum + value, 0),
  };
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

/** How many of `outcomes` equal `outcome`. */
export function count(outcomes: Uint8Array, outcome: CrowdOutcome): number {
  let total = 0;
  for (const value of outcomes) {
    if (value === outcome) {
      total += 1;
    }
  }
  return total;
}
