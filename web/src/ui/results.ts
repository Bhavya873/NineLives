// The results screen: a definition list of a run's numbers (run-stats.ts), shown in
// the game-over screen and, with Tab, as an overlay in the middle of a run.

/** Fills `list` with one label and one value per row. */
export function renderResults(list: HTMLElement, rows: Array<[string, string]>): void {
  list.replaceChildren();
  for (const [label, value] of rows) {
    const term = document.createElement('dt');
    term.textContent = label;
    const detail = document.createElement('dd');
    detail.textContent = value;
    list.append(term, detail);
  }
}
