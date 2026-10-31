// Stage tabs and showcase seeds (plan G5). The tabs are the curve's stages A-F; the crowd can
// fly stages A, D and F (the other stages have no Crowd in the wasm), so the other tabs show
// the curve alone. Showcase seeds are the first seed of the drops each tab starts on, chosen
// by `scripts/choose-showcase-seeds.mjs` and committed in `showcase.json`, so every visit
// opens on the same reproducible round. Pure.
import type { ArenaKind } from './arena-choice.ts';

/** The stage tabs, left to right. */
export const STAGE_TABS = ['A', 'B', 'C', 'D', 'E', 'F'] as const;
export type StageTab = (typeof STAGE_TABS)[number];

/** The arena a tab's crowd flies, or null when the crowd cannot fly that stage. */
export function tabArena(tab: StageTab): ArenaKind | null {
  switch (tab) {
    case 'A':
      return 'flat';
    case 'D':
      return 'driving';
    case 'F':
      return 'flight';
    default:
      return null;
  }
}

/** The tab an arena belongs to (the obstacle arena is Stage A, the Gauntlet Stage D). */
export function arenaTab(kind: ArenaKind): StageTab {
  switch (kind) {
    case 'flat':
    case 'obstacles':
      return 'A';
    case 'driving':
    case 'gauntlet':
      return 'D';
    case 'flight':
      return 'F';
  }
}

/** The tab named by `text`, if any. */
export function tabFromText(text: string | null): StageTab | null {
  return STAGE_TABS.find((tab) => tab === text) ?? null;
}

/** How one block of drops went, as flown by the best and the first checkpoint. */
export interface BlockResult {
  firstSeed: number;
  size: number;
  bestLanded: number;
  firstLanded: number;
}

/** The block that shows the policy off: most cars landed by the best checkpoint, then the
 * biggest gap over the first checkpoint, then the lowest seed. Null for no blocks. */
export function pickShowcase(blocks: BlockResult[]): BlockResult | null {
  let chosen: BlockResult | null = null;
  for (const block of blocks) {
    if (chosen === null || isBetter(block, chosen)) {
      chosen = block;
    }
  }
  return chosen;
}

function isBetter(block: BlockResult, chosen: BlockResult): boolean {
  if (block.bestLanded !== chosen.bestLanded) {
    return block.bestLanded > chosen.bestLanded;
  }
  const gap = block.bestLanded - block.firstLanded;
  const chosenGap = chosen.bestLanded - chosen.firstLanded;
  if (gap !== chosenGap) {
    return gap > chosenGap;
  }
  return block.firstSeed < chosen.firstSeed;
}

/** One committed showcase entry. */
export interface ShowcaseEntry {
  first_seed: number;
  size: number;
  best_landed: number;
  first_landed: number;
}

/** The contents of `checkpoints/<run>/showcase.json`. */
export interface ShowcaseFile {
  run_id: string;
  physics_hash: string;
  policy_hashes: { first: string; best: string };
  blocks_tried: number;
  /** Keyed by arena kind (only the arenas whose rows share seeds). */
  seeds: Partial<Record<ArenaKind, ShowcaseEntry>>;
}

/** The first seed of the round `round` of `arena`: the showcase seed, then successive blocks
 * of `size` drops. Zero when the arena has no showcase entry. */
export function showcaseSeed(
  file: ShowcaseFile | null,
  arena: ArenaKind,
  round: number,
  size: number,
): number {
  const entry = file?.seeds[arena];
  return (entry?.first_seed ?? 0) + round * size;
}
