// The nine lives: a row of pixel hearts, one emptied per crash, and the game-over
// screen once all nine are gone. It reads the sim's crash count; it never touches physics.
// A crash from flying out of bounds is a free respawn: `grantFreeRespawns` gives that life back.

import { focusDialog, setText } from '../shared/dom.ts';
import type { HudValues } from './hud.ts';

/** Lives per run: the game's name. */
export const LIVES = 9;
/** Wait after the last crash before the game-over screen, so the wreck can burn, in ms. */
const GAME_OVER_DELAY_MS = 1500;

/**
 * One heart, 9 × 8 pixels: o outline, f fill, h highlight, . empty. Drawn for this game
 * in the blocky style of pixel-art games, not copied from any.
 */
const HEART_PIXELS = [
  '.ooo.ooo.',
  'ohffofffo',
  'ohffffffo',
  'offfffffo',
  '.offfffo.',
  '..offfo..',
  '...ofo...',
  '....o....',
];

/** The CSS class for each pixel letter. */
const PIXEL_CLASS: Record<string, string> = { o: 'o', f: 'f', h: 'h' };

/** The hearts, the game-over screen and where this run's counts started. */
interface Lives {
  hearts: HTMLElement[];
  gameOver: HTMLElement;
  summary: HTMLElement;
  /** The sim's crash and save counts when this run began (they never reset). */
  crashesAtStart: number;
  savesAtStart: number;
  /** Crashes this run that cost nothing (out of bounds). */
  freeRespawns: number;
  /** When to show the game-over screen, once the last life is gone; null before. */
  gameOverAtMs: number | null;
  /** True while the game-over screen is up. */
  over: boolean;
  /** Gives focus back when the game-over screen closes. */
  restoreFocus: (() => void) | null;
}

/** Builds the nine hearts in `row` and wraps the game-over screen (it starts hidden). */
export function createLives(row: HTMLElement, gameOver: HTMLElement, summary: HTMLElement): Lives {
  const hearts: HTMLElement[] = [];
  for (let index = 0; index < LIVES; index += 1) {
    const heart = document.createElement('span');
    heart.className = 'heart';
    heart.innerHTML = heartSvg();
    row.appendChild(heart);
    hearts.push(heart);
  }
  gameOver.hidden = true;
  return {
    hearts,
    gameOver,
    summary,
    crashesAtStart: 0,
    savesAtStart: 0,
    freeRespawns: 0,
    gameOverAtMs: null,
    over: false,
    restoreFocus: null,
  };
}

/** Lives left in this run, from the sim's crash count less the free respawns. */
export function livesLeft(
  lives: Pick<Lives, 'crashesAtStart' | 'freeRespawns'>,
  values: Pick<HudValues, 'crashes'>,
): number {
  const costly = values.crashes - lives.crashesAtStart - lives.freeRespawns;
  return Math.max(0, LIVES - costly);
}

/** Gives back `count` lives for crashes that should not cost one. Call before `updateLives`. */
export function grantFreeRespawns(lives: Pick<Lives, 'freeRespawns'>, count: number): void {
  lives.freeRespawns += count;
}

/** Empties a heart per crash, and puts up the game-over screen after the last one. */
export function updateLives(lives: Lives, values: HudValues, nowMs: number): void {
  const left = livesLeft(lives, values);
  lives.hearts.forEach((heart, index) => heart.classList.toggle('lost', index >= left));
  if (left === 0 && lives.gameOverAtMs === null) {
    lives.gameOverAtMs = nowMs + GAME_OVER_DELAY_MS;
  }
  if (!lives.over && lives.gameOverAtMs !== null && nowMs >= lives.gameOverAtMs) {
    lives.over = true;
    setText(lives.summary, runSummary(lives, values));
    lives.gameOver.hidden = false;
    lives.restoreFocus = focusDialog(
      lives.gameOver,
      lives.gameOver.querySelector('button') ?? lives.gameOver,
    );
  }
}

/** Carries a run over a page change: `lost` lives are already gone and `saves` already made. Call once at start-up. */
export function restoreLives(
  lives: Pick<Lives, 'crashesAtStart' | 'freeRespawns' | 'savesAtStart'>,
  values: Pick<HudValues, 'crashes' | 'saves'>,
  lost: number,
  saves = 0,
): void {
  const clamped = Number.isFinite(lost) ? Math.min(LIVES - 1, Math.max(0, Math.floor(lost))) : 0;
  lives.crashesAtStart = values.crashes - clamped;
  lives.freeRespawns = 0;
  lives.savesAtStart = values.saves - (Number.isFinite(saves) ? Math.max(0, Math.floor(saves)) : 0);
}

/** Saves made in this run so far, for the page change. */
export function savesThisRun(
  lives: Pick<Lives, 'savesAtStart'>,
  values: Pick<HudValues, 'saves'>,
): number {
  return values.saves - lives.savesAtStart;
}

/** Starts a new run with nine lives from the sim's current counts. */
export function restartLives(lives: Lives, values: HudValues): void {
  lives.crashesAtStart = values.crashes;
  lives.savesAtStart = values.saves;
  lives.freeRespawns = 0;
  lives.gameOverAtMs = null;
  lives.over = false;
  lives.gameOver.hidden = true;
  lives.restoreFocus?.();
  lives.restoreFocus = null;
}

/** This run's saves, for the game-over screen. */
function runSummary(lives: Lives, values: HudValues): string {
  const saves = savesThisRun(lives, values);
  return `Out of lives - ${saves === 1 ? '1 save' : `${saves} saves`}`;
}

/** One heart as an SVG of square pixels. */
function heartSvg(): string {
  const rects: string[] = [];
  HEART_PIXELS.forEach((row, y) => {
    Array.from(row).forEach((pixel, x) => {
      const cssClass = PIXEL_CLASS[pixel];
      if (cssClass) {
        rects.push(`<rect class="${cssClass}" x="${x}" y="${y}" width="1" height="1"/>`);
      }
    });
  });
  return `<svg viewBox="0 0 9 8" shape-rendering="crispEdges" aria-hidden="true">${rects.join('')}</svg>`;
}
