// The "stunt ready" hint: while airborne, shows the stunt keys the planner would accept
// right now (the wasm `stunt_ready_mask`, read-only). The planner still decides on a key
// press; this only tells the player a press would be taken.
import { StuntCode, STUNT_NAME } from '../stunt-requests.ts';

/** Stunt code and its key label, in display order. */
const READY_KEYS: Array<[number, string]> = [
  [StuntCode.BarrelLeft, 'Q'],
  [StuntCode.BarrelRight, 'E'],
  [StuntCode.FrontFlip, 'Z'],
  [StuntCode.BackFlip, 'B'],
];

/** The hint for a mask (bit `code - 1` set means that stunt would be authorized); '' for none. */
export function stuntReadyText(mask: number): string {
  const ready = READY_KEYS.filter(([code]) => (mask & (1 << (code - 1))) !== 0);
  if (ready.length === 0) {
    return '';
  }
  return `Stunt ready: ${ready.map(([, key]) => key).join(' ')}`;
}

/** The names of the ready stunts, for an accessible label. */
export function stuntReadyNames(mask: number): string[] {
  return READY_KEYS.filter(([code]) => (mask & (1 << (code - 1))) !== 0).map(
    ([code]) => STUNT_NAME[code] ?? 'Stunt',
  );
}
