// Picks the training viewer's showcase seeds (plan G5) and writes
// web/public/checkpoints/<run>/showcase.json:
//
//     node scripts/choose-showcase-seeds.mjs [blocks=16]
//
// For each arena whose rows share seeds (Stage A flat, D, F) it flies `blocks` consecutive
// blocks of 8 drops (seeds 0, 8, 16, ...) with the first and the last committed checkpoint,
// exactly as the viewer's Crowd does, and keeps the block `pickShowcase` ranks best. Run
// `npm run wasm:release` first. Deterministic: the same checkpoints give the same file.
import { readFileSync, writeFileSync } from 'node:fs';

import { pickShowcase } from '../src/viewer/showcase.ts';
import { Crowd, CrowdOutcome } from '../tests-node/load-wasm.mjs';

export const BLOCK_SIZE = 8;
export const SHOWCASE_ARENAS = { flat: 'A', driving: 'D', flight: 'F' };
const STEP_LIMIT = 120 * 600;
const CHECKPOINTS = new URL('../public/checkpoints/', import.meta.url);

/** How many of the `BLOCK_SIZE` cars of `stage` starting at `firstSeed` land with `policyJson`. */
export function flyBlock(policyJson, stage, firstSeed) {
  const crowd = Crowd.for_stage(policyJson, 'realistic', stage, BigInt(firstSeed), BLOCK_SIZE);
  let steps = 0;
  while (crowd.step()) {
    steps += 1;
    if (steps > STEP_LIMIT) {
      throw new Error(`the ${stage} crowd never finished`);
    }
  }
  const landed = Array.from(crowd.outcomes()).filter((o) => o === CrowdOutcome.Landed).length;
  crowd.free();
  return landed;
}

/** The newest run's id and its first and last checkpoint texts. */
export function loadRun() {
  const index = JSON.parse(readFileSync(new URL('index.json', CHECKPOINTS), 'utf8'));
  const run = index.runs.at(-1);
  const read = (entry) => readFileSync(new URL(`${run.run_id}/${entry.file}`, CHECKPOINTS), 'utf8');
  return { runId: run.run_id, first: read(run.checkpoints[0]), best: read(run.checkpoints.at(-1)) };
}

function main() {
  const blocks = Number(process.argv[2] ?? 16);
  const { runId, first, best } = loadRun();
  const seeds = {};
  for (const [arena, stage] of Object.entries(SHOWCASE_ARENAS)) {
    const results = [];
    for (let block = 0; block < blocks; block += 1) {
      const firstSeed = block * BLOCK_SIZE;
      results.push({
        firstSeed,
        size: BLOCK_SIZE,
        bestLanded: flyBlock(best, stage, firstSeed),
        firstLanded: flyBlock(first, stage, firstSeed),
      });
    }
    const pick = pickShowcase(results);
    seeds[arena] = {
      first_seed: pick.firstSeed,
      size: pick.size,
      best_landed: pick.bestLanded,
      first_landed: pick.firstLanded,
    };
    console.log(arena, seeds[arena]);
  }
  const curve = JSON.parse(readFileSync(new URL(`${runId}/curve.json`, CHECKPOINTS), 'utf8'));
  const file = {
    run_id: runId,
    physics_hash: curve.physics_hash,
    policy_hashes: {
      first: curve.checkpoints[0].policy_hash,
      best: curve.checkpoints.at(-1).policy_hash,
    },
    blocks_tried: blocks,
    seeds,
  };
  writeFileSync(
    new URL(`${runId}/showcase.json`, CHECKPOINTS),
    `${JSON.stringify(file, null, 2)}\n`,
  );
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main();
}
