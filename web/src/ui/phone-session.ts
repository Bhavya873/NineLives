// Best-effort phone niceties when Play is pressed: full screen and a screen wake lock so the
// display does not dim mid-run. Both can be refused (iOS Safari has no full screen for pages);
// a refusal is ignored and nothing else depends on it.

/** The held wake lock, kept so it can be taken again after the tab was hidden. */
let wakeLock: WakeLockSentinel | null = null;

async function holdWakeLock(): Promise<void> {
  try {
    wakeLock = (await navigator.wakeLock?.request('screen')) ?? null;
  } catch {
    wakeLock = null;
  }
}

/** Asks for full screen and a wake lock. Call from the Play button's click. */
export function enterPhoneSession(): void {
  try {
    void document.documentElement.requestFullscreen?.().catch(() => undefined);
  } catch {
    // no full screen here: the game works in the browser window
  }
  void holdWakeLock();
  // A browser drops the lock when the tab is hidden; take it again on return.
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible' && wakeLock?.released) {
      void holdWakeLock();
    }
  });
}
