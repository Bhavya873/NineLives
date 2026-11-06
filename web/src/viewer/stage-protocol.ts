// The messages between the viewer page and a stage worker. Each stage (flat arena, Stage B,
// obstacle arena, Stage D, Stage F) steps its eight checkpoint crowds in its own Web Worker, so
// the main thread only draws and the five stages together never block a frame. The worker
// posts the cars' poses a few times per frame; the page blends between the last two (see
// snapshot-clock.ts). Types only.
import type { ArenaKind } from './arena-choice.ts';

/** Page to worker: build the stage's crowds and start flying. */
export interface StageInit {
  type: 'init';
  arena: ArenaKind;
  /** Each checkpoint's policy file text, in training order. */
  policies: string[];
  carsPerCheckpoint: number;
  /** The first seed of round 0 (the committed showcase seed, or 0). */
  firstSeed: number;
  /** True if every checkpoint row flies the same seeds. */
  shared: boolean;
  /** How long the final picture stays before the next round, in ms. */
  pauseBetweenMs: number;
}

/** Page to worker: start or stop stepping (a stage nobody can see does not need to run). */
interface StageRun {
  type: 'run';
  on: boolean;
}

export type ToStageWorker = StageInit | StageRun;

/** One checkpoint row's poses and outcomes at one tick. */
export interface RowSnapshot {
  chassis: Float32Array;
  wheels: Float32Array;
  outcomes: Uint8Array;
}

/** Worker to page: a new round of drops has been built. */
export interface StageRound {
  type: 'round';
  round: number;
  /** The course's shapes as JSON, or null on the flat arenas. */
  shapes: string | null;
  wheelsPerCar: number;
  /** Each checkpoint row's car tags in car order ("barrel L"); empty strings where a car has
   * none (only Stage F cars have a flip plate). */
  carTags: string[][];
}

/** Worker to page: the poses after `simTimeS` seconds of this round. A row that has not
 * changed since the last snapshot (it finished) is null. */
export interface StageSnapshot {
  type: 'snap';
  round: number;
  simTimeS: number;
  rows: (RowSnapshot | null)[];
  /** True while any drop of the round is still flying. */
  running: boolean;
  /** Mean wall-clock ms the worker spent per physics tick since the last snapshot. */
  stepMs: number;
}

/** Worker to page: something failed. */
interface StageError {
  type: 'error';
  message: string;
}

export type FromStageWorker = StageRound | StageSnapshot | StageError;

/** The first seed of every checkpoint row for round `round`: the same seeds for all rows when
 * `shared` (so rows compare fairly), else a block per row so the cars do not overlap. */
export function rowSeeds(
  firstSeed: number,
  shared: boolean,
  round: number,
  rows: number,
  carsPerCheckpoint: number,
): bigint[] {
  const seeds: bigint[] = [];
  for (let row = 0; row < rows; row += 1) {
    seeds.push(
      shared
        ? BigInt(firstSeed + round * carsPerCheckpoint)
        : BigInt(round * carsPerCheckpoint * rows + row * carsPerCheckpoint),
    );
  }
  return seeds;
}
