// The leader camera (plan G5): follows the best car of the whole crowd. On the courses the
// cars run along +X, so the best car is the one furthest along; cars still running beat
// cars that already landed, which beat crashed ones. Pure.

/** `CrowdOutcome` values (crates/nl-wasm/src/crowd.rs). */
const RUNNING = 0;
const LANDED = 1;

/** Where the leader is and which of the given cars it is. */
interface Leader {
  /** Index into the list of cars given to `pickLeader`. */
  car: number;
  x: number;
  z: number;
}

/** Rank of an outcome: running beats landed beats crashed or timed out. */
function rank(outcome: number): number {
  return outcome === RUNNING ? 2 : outcome === LANDED ? 1 : 0;
}

/** The leader among the cars of one crowd (each chassis pose is `stride` floats starting
 * x, y, z; `outcomes` has one `CrowdOutcome` code per car), or null if there are no cars. */
export function pickLeader(
  poses: Float32Array,
  outcomes: Uint8Array,
  stride: number,
): Leader | null {
  let best: Leader | null = null;
  let bestRank = -1;
  for (let car = 0; car < outcomes.length; car += 1) {
    const carRank = rank(outcomes[car] ?? 0);
    const x = poses[car * stride] ?? 0;
    if (carRank > bestRank || (carRank === bestRank && best !== null && x > best.x)) {
      best = { car, x, z: poses[car * stride + 2] ?? 0 };
      bestRank = carRank;
    }
  }
  return best;
}

/** The leader over several crowds (rows): the car's row too, or null. */
export function pickCrowdLeader(
  rows: { poses: Float32Array; outcomes: Uint8Array }[],
  stride: number,
): (Leader & { row: number }) | null {
  let best: (Leader & { row: number }) | null = null;
  let bestRank = -1;
  for (let index = 0; index < rows.length; index += 1) {
    const row = rows[index];
    const leader = row ? pickLeader(row.poses, row.outcomes, stride) : null;
    if (!row || !leader) {
      continue;
    }
    const leaderRank = rank(row.outcomes[leader.car] ?? 0);
    if (leaderRank > bestRank || (leaderRank === bestRank && best !== null && leader.x > best.x)) {
      best = { ...leader, row: index };
      bestRank = leaderRank;
    }
  }
  return best;
}
