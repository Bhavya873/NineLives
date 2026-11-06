// The training viewer's curve (plan G5): how each committed checkpoint flies in the CURRENT
// physics, per stage. `python -m ninelives.export --viewer-curve` writes `curve.json`; this
// module types it and draws one stage's line as an SVG string. Pure: no DOM, no wasm. The
// marks carry classes; viewer.css colours them with the theme tokens.

/** The title the plan requires: it is a re-evaluation, not the training history. */
export const CURVE_TITLE = 'Checkpoint evaluation under current physics';

/** What the page shows for it: the full title stays the exporter's key. */
export const CURVE_LABEL = 'Checkpoint results';

/** One stage's rates for one checkpoint, each 0..1. */
interface StageRates {
  success_rate: number;
  save_rate: number;
  crash_rate: number;
}

/** One checkpoint's row of the curve file. */
interface CurveCheckpoint {
  file: string;
  steps: number;
  policy_hash: string | null;
  stages: Record<string, StageRates>;
}

/** The contents of `checkpoints/<run>/curve.json`. */
export interface CurveFile {
  title: string;
  run_id: string;
  episodes: number;
  stages: string[];
  physics_hash: string;
  controller_config_hash: string;
  git_sha: string;
  checkpoints: CurveCheckpoint[];
}

/** One plotted point. */
interface CurvePoint {
  steps: number;
  rate: number;
  file: string;
}

/** `stage`'s success rate per checkpoint, in training order (empty for an unknown stage). */
export function curveSeries(curve: CurveFile, stage: string): CurvePoint[] {
  const points: CurvePoint[] = [];
  for (const entry of curve.checkpoints) {
    const rates = entry.stages[stage];
    if (rates) {
      points.push({ steps: entry.steps, rate: rates.success_rate, file: entry.file });
    }
  }
  return points;
}

/** The checkpoint with the highest success rate on `stage` (the later one on a tie), or
 * null when the stage has no points. */
export function bestCheckpoint(curve: CurveFile, stage: string): CurvePoint | null {
  let best: CurvePoint | null = null;
  for (const point of curveSeries(curve, stage)) {
    if (best === null || point.rate >= best.rate) {
      best = point;
    }
  }
  return best;
}

/** Throws unless `value` looks like a curve file, so a bad file fails loudly. */
export function parseCurve(value: unknown): CurveFile {
  const curve = value as Partial<CurveFile> | null;
  if (!curve || curve.title !== CURVE_TITLE || !Array.isArray(curve.checkpoints)) {
    throw new Error(`curve.json is not a "${CURVE_TITLE}" file`);
  }
  return curve as CurveFile;
}

const WIDTH = 320;
const HEIGHT = 140;
const PAD = { left: 34, right: 10, top: 10, bottom: 24 };

/** An SVG (as text) of `stage`'s success rate against training steps; the best point is
 * filled rust. Rates run 0..100 % up the page. */
export function curveSvg(curve: CurveFile, stage: string): string {
  const points = curveSeries(curve, stage);
  const best = bestCheckpoint(curve, stage);
  const maxSteps = Math.max(1, ...points.map((point) => point.steps));
  const x = (steps: number) => PAD.left + ((WIDTH - PAD.left - PAD.right) * steps) / maxSteps;
  const y = (rate: number) => PAD.top + (HEIGHT - PAD.top - PAD.bottom) * (1 - rate);
  const grid = [0, 0.5, 1]
    .map(
      (rate) =>
        `<line x1="${PAD.left}" x2="${WIDTH - PAD.right}" y1="${y(rate)}" y2="${y(rate)}" class="chart-grid"/>` +
        `<text x="${PAD.left - 4}" y="${y(rate) + 4}" text-anchor="end" class="chart-text">${rate * 100}%</text>`,
    )
    .join('');
  const line = points.map((point) => `${x(point.steps).toFixed(1)},${y(point.rate).toFixed(1)}`);
  const dots = points
    .map((point) => {
      const kind = point.file === best?.file ? 'chart-best' : 'chart-dot';
      return (
        `<circle cx="${x(point.steps).toFixed(1)}" cy="${y(point.rate).toFixed(1)}" r="${kind === 'chart-best' ? 6 : 4.5}" class="${kind}">` +
        `<title>${point.file}: ${(point.rate * 100).toFixed(0)}% at ${(point.steps / 1e6).toFixed(0)}M steps</title></circle>`
      );
    })
    .join('');
  const axis = `<text x="${WIDTH / 2}" y="${HEIGHT - 4}" text-anchor="middle" class="chart-text">steps (max ${(maxSteps / 1e6).toFixed(0)}M)</text>`;
  return (
    `<svg viewBox="0 0 ${WIDTH} ${HEIGHT}" width="${WIDTH}" height="${HEIGHT}" role="img" aria-label="Stage ${stage} success rate per checkpoint">` +
    `${grid}<polyline class="chart-line" points="${line.join(' ')}"/>${dots}${axis}</svg>`
  );
}
