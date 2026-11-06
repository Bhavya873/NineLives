// Keeps the lives lost across a trip to the training viewer and back (sessionStorage, so a
// new tab starts fresh). The game saves before a watch link leaves; `?play=1&resume=1`
// reads it once. A run that is over, or no saved run, means nine fresh lives. It keeps the
// lives lost and the saves made, so the game-over count carries on.

/** The sessionStorage key. */
export const RUN_KEY = 'ninelives.run';

/** The slice of `Storage` used here. */
interface RunStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/** Saves the lives lost so far; a finished run (no lives left) saves nothing. */
export function saveRun(
  storage: RunStorage | null,
  run: { lostLives: number; saves: number },
  livesLeft: number,
): void {
  try {
    if (livesLeft <= 0) {
      storage?.removeItem(RUN_KEY);
    } else {
      storage?.setItem(RUN_KEY, JSON.stringify(run));
    }
  } catch {
    // storage may be blocked; the next visit then starts fresh
  }
}

/** The lives lost and saves made in the saved run, taken out of storage (zeros when none). */
export function takeSavedRun(storage: RunStorage | null): { lostLives: number; saves: number } {
  const none = { lostLives: 0, saves: 0 };
  try {
    const raw = storage?.getItem(RUN_KEY);
    storage?.removeItem(RUN_KEY);
    if (!raw) {
      return none;
    }
    const saved = JSON.parse(raw) as { lostLives?: unknown; saves?: unknown };
    const count = (value: unknown) =>
      typeof value === 'number' && Number.isFinite(value) && value > 0 ? value : 0;
    return { lostLives: count(saved.lostLives), saves: count(saved.saves) };
  } catch {
    return none;
  }
}

/** The browser's sessionStorage, or null when blocked. */
export function sessionStore(): RunStorage | null {
  try {
    return window.sessionStorage;
  } catch {
    return null;
  }
}
