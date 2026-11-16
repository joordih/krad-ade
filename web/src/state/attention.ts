import type { AttentionKind } from "../protocol/AttentionKind";
import type { AttentionNotice } from "../protocol/AttentionNotice";
import type { AttentionSnapshot } from "../protocol/AttentionSnapshot";
import type { AttentionTarget } from "../protocol/AttentionTarget";
import type { AttentionWaiting } from "../protocol/AttentionWaiting";

/** What the sidebar needs from the runtime's attention queue and log. */
export type Attention = {
  waiting: AttentionWaiting[];
  history: AttentionNotice[];
};

export const EMPTY_ATTENTION: Attention = { waiting: [], history: [] };

export type AttentionSpot = {
  runtime: string;
  target: AttentionTarget;
  kind: AttentionKind;
  since: string;
};

export function targetKey(target: AttentionTarget): string {
  return `${target.environment}/${target.terminal}`;
}

function timeOf(at: string): number {
  const parsed = Date.parse(at);
  return Number.isFinite(parsed) ? parsed : 0;
}

/** The runtime's first snapshot: its waiting queue and history, newest first. */
export function attentionFromSnapshot(snapshot: AttentionSnapshot): Attention {
  return { waiting: snapshot.waiting, history: [...snapshot.history].reverse() };
}

/** A raised notice replaces its target in the queue and joins the log. */
export function attentionRaised(attention: Attention, notice: AttentionNotice): Attention {
  const key = targetKey(notice.target);
  const entry: AttentionWaiting = { target: notice.target, kind: notice.kind, since: notice.at };
  return {
    waiting: [...attention.waiting.filter((known) => targetKey(known.target) !== key), entry],
    history: [notice, ...attention.history],
  };
}

export function attentionLeft(attention: Attention, target: AttentionTarget): Attention {
  const key = targetKey(target);
  return { ...attention, waiting: attention.waiting.filter((known) => targetKey(known.target) !== key) };
}

export function attentionCleared(attention: Attention): Attention {
  return { ...attention, history: [] };
}

/** The queue the desktop's jump walks: oldest first. */
export function orderedWaiting(waiting: readonly AttentionWaiting[]): AttentionWaiting[] {
  return [...waiting].sort((left, right) => timeOf(left.since) - timeOf(right.since));
}

/** The waiting entry after the current one, wrapping, or the first when none is current. */
export function nextWaiting(
  waiting: readonly AttentionWaiting[],
  current?: AttentionTarget,
): AttentionWaiting | undefined {
  const ordered = orderedWaiting(waiting);
  const first = ordered[0];
  if (!current || !first) return first;
  const key = targetKey(current);
  const at = ordered.findIndex((entry) => targetKey(entry.target) === key);
  if (at < 0) return first;
  return ordered[(at + 1) % ordered.length] ?? first;
}

/** How many waiting entries belong to a session's terminals. */
export function countIn(
  waiting: readonly AttentionWaiting[],
  environment: string,
  terminals: readonly number[],
): number {
  const held = new Set(terminals);
  return waiting.filter((entry) => entry.target.environment === environment && held.has(entry.target.terminal))
    .length;
}

/** Every waiting entry across this browser's runtimes, oldest first. */
export function spotsOf(views: Readonly<Record<string, { attention: Attention }>>): AttentionSpot[] {
  return Object.entries(views)
    .flatMap(([runtime, view]) =>
      view.attention.waiting.map((entry) => ({
        runtime,
        target: entry.target,
        kind: entry.kind,
        since: entry.since,
      })),
    )
    .sort((left, right) => timeOf(left.since) - timeOf(right.since));
}

function spotOf(spot: AttentionSpot, current: { runtime?: string; environment: string; terminal: number }): boolean {
  return (
    spot.target.environment === current.environment &&
    spot.target.terminal === current.terminal &&
    (current.runtime === undefined || spot.runtime === current.runtime)
  );
}

/** The waiting spot after the current one, wrapping, or the first when none is current. */
export function nextSpot(
  spots: readonly AttentionSpot[],
  current?: { runtime?: string; environment: string; terminal: number },
): AttentionSpot | undefined {
  const ordered = [...spots].sort((left, right) => timeOf(left.since) - timeOf(right.since));
  const first = ordered[0];
  if (!current || !first) return first;
  const at = ordered.findIndex((spot) => spotOf(spot, current));
  if (at < 0) return first;
  return ordered[(at + 1) % ordered.length] ?? first;
}
