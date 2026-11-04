// The nine lives: a row of pixel hearts, one emptied per crash, and the black game-over
// screen once all nine are gone. It reads the sim's crash count; it never touches physics.

import { focusDialog, setText } from '../dom.ts';
import { formatTime, type HudValues } from './hud.ts';

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
export interface Lives {
  hearts: HTMLElement[];
  gameOver: HTMLElement;
  summary: HTMLElement;
  /** The sim's crash, save and time counts when this run began (they never reset). */
  crashesAtStart: number;
  savesAtStart: number;
  timeAtStartS: number;
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
    timeAtStartS: 0,
    gameOverAtMs: null,
    over: false,
    restoreFocus: null,
  };
}

/** Lives left in this run, from the sim's crash count. */
export function livesLeft(lives: Lives, values: HudValues): number {
  return Math.max(0, LIVES - (values.crashes - lives.crashesAtStart));
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

/** Starts a new run with nine lives from the sim's current counts. */
export function restartLives(lives: Lives, values: HudValues): void {
  lives.crashesAtStart = values.crashes;
  lives.savesAtStart = values.saves;
  lives.timeAtStartS = values.timeS;
  lives.gameOverAtMs = null;
  lives.over = false;
  lives.gameOver.hidden = true;
  lives.restoreFocus?.();
  lives.restoreFocus = null;
}

/** This run's saves and time, for the game-over screen. */
export function runSummary(lives: Lives, values: HudValues): string {
  const saves = values.saves - lives.savesAtStart;
  const time = formatTime(values.timeS - lives.timeAtStartS);
  const savesText = saves === 1 ? '1 save' : `${saves} saves`;
  return `${savesText} in ${time}`;
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
