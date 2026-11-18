import { nextSpot, spotsOf, type AttentionSpot, type SpotPlace } from "./attention";
import { environmentNames, terminalSession, type SessionPlace } from "./sessions";
import type { AppState } from "./store";
import type { RuntimeKey } from "./runtimes";

export type Jump =
  | { kind: "open"; spot: AttentionSpot; place: SessionPlace; listedIn: string }
  | { kind: "dismiss"; spot: AttentionSpot };

export function reachableSpots(state: AppState): AttentionSpot[] {
  return spotsOf(state.views).filter(
    (spot) => terminalSession(state, spot.runtime, spot.target.environment, spot.target.terminal) !== undefined,
  );
}

export function listedEnvironment(state: AppState, runtime: RuntimeKey, name: string): string {
  if (environmentNames(state).includes(name)) return name;
  const hosted = state.views[runtime]?.snapshot?.hosted.find((entry) => entry.environment === name);
  return hosted?.origin?.environment ?? name;
}

export function nextJump(state: AppState, spots: readonly AttentionSpot[], current?: SpotPlace): Jump | undefined {
  const spot = nextSpot(spots, current);
  if (!spot) return undefined;
  const session = terminalSession(state, spot.runtime, spot.target.environment, spot.target.terminal);
  if (!session) return { kind: "dismiss", spot };
  return {
    kind: "open",
    spot,
    place: { runtime: spot.runtime, environment: spot.target.environment, session: session.id },
    listedIn: listedEnvironment(state, spot.runtime, spot.target.environment),
  };
}
