// Keeps the lives lost across a trip to the training viewer and back (sessionStorage, so a
// new tab starts fresh). The game saves before a watch link leaves; `?play=1&resume=1`
// reads it once. A run that is over, or no saved run, means nine fresh lives.

/** The sessionStorage key. */
export const RUN_KEY = 'ninelives.run';

/** The slice of `Storage` used here. */
interface RunStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/** Saves the lives lost so far; a finished run (no lives left) saves nothing. */
export function saveRun(storage: RunStorage | null, lostLives: number, livesLeft: number): void {
  try {
    if (livesLeft <= 0) {
      storage?.removeItem(RUN_KEY);
    } else {
      storage?.setItem(RUN_KEY, JSON.stringify({ lostLives }));
    }
  } catch {
    // storage may be blocked; the next visit then starts fresh
  }
}

/** The lives lost in the saved run, taken out of storage (0 when none or unreadable). */
export function takeSavedRun(storage: RunStorage | null): number {
  try {
    const raw = storage?.getItem(RUN_KEY);
    storage?.removeItem(RUN_KEY);
    if (!raw) {
      return 0;
    }
    const lost = (JSON.parse(raw) as { lostLives?: unknown }).lostLives;
    return typeof lost === 'number' && Number.isFinite(lost) && lost > 0 ? lost : 0;
  } catch {
    return 0;
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
