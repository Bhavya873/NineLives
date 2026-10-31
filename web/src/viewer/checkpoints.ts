// Finds the training viewer's checkpoints (SPEC §9.12): `ninelives.export --viewer` writes
// up to 8 policy files per run to public/checkpoints/<run-id>/<n>.json plus an index.json
// listing them, in training order. The viewer shows the newest run.
import { type CurveFile, parseCurve } from './curve.ts';
import type { ShowcaseFile } from './showcase.ts';

/** One exported checkpoint: where its file is and how far training had got. */
export interface CheckpointEntry {
  file: string;
  steps: number;
}

/** A run's exported checkpoints, oldest first. */
export interface RunEntry {
  run_id: string;
  checkpoints: CheckpointEntry[];
}

/** The URL of a file under public/checkpoints/. */
function checkpointUrl(path: string): string {
  return `${import.meta.env.BASE_URL}checkpoints/${path}`;
}

/** The newest run in the index, or an error naming what is missing. */
export async function loadNewestRun(): Promise<RunEntry> {
  const response = await fetch(checkpointUrl('index.json'));
  if (!response.ok) {
    throw new Error('No checkpoints/index.json: run `python -m ninelives.export --viewer` first');
  }
  const index = (await response.json()) as { runs: RunEntry[] };
  const newest = index.runs.at(-1);
  if (!newest || newest.checkpoints.length === 0) {
    throw new Error('checkpoints/index.json lists no checkpoints');
  }
  return newest;
}

/** The text of one checkpoint's policy file. */
export async function loadCheckpoint(run: RunEntry, entry: CheckpointEntry): Promise<string> {
  const response = await fetch(checkpointUrl(`${run.run_id}/${entry.file}`));
  if (!response.ok) {
    throw new Error(`Checkpoint ${run.run_id}/${entry.file} is missing`);
  }
  return response.text();
}

/** The run's curve file (`curve.json`, from `export --viewer-curve`), or null if absent. */
export async function loadCurve(run: RunEntry): Promise<CurveFile | null> {
  const response = await fetch(checkpointUrl(`${run.run_id}/curve.json`));
  return response.ok ? parseCurve(await response.json()) : null;
}

/** The run's showcase seeds (`showcase.json`), or null if absent. */
export async function loadShowcase(run: RunEntry): Promise<ShowcaseFile | null> {
  const response = await fetch(checkpointUrl(`${run.run_id}/showcase.json`));
  return response.ok ? ((await response.json()) as ShowcaseFile) : null;
}
