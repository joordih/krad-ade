import type { AttentionKind } from "../protocol/AttentionKind";
import type { AttentionNotice } from "../protocol/AttentionNotice";
import type { AttentionSnapshot } from "../protocol/AttentionSnapshot";
import type { AttentionTarget } from "../protocol/AttentionTarget";
import type { AttentionWaiting } from "../protocol/AttentionWaiting";
import type { RuntimeKey } from "./runtimes";

export type Attention = {
  waiting: AttentionWaiting[];
  history: AttentionNotice[];
};

export const EMPTY_ATTENTION: Attention = { waiting: [], history: [] };

export type AttentionSpot = {
  runtime: RuntimeKey;
  target: AttentionTarget;
  kind: AttentionKind;
  since: string;
};

export type SpotPlace = { runtime?: RuntimeKey; environment: string; terminal: number };

function sameTarget(left: AttentionTarget, right: AttentionTarget): boolean {
  return left.environment === right.environment && left.terminal === right.terminal;
}

function timeOf(at: string): number {
  const parsed = Date.parse(at);
  return Number.isFinite(parsed) ? parsed : 0;
}

function oldestFirst<T extends { since: string }>(entries: readonly T[]): T[] {
  return [...entries].sort((left, right) => timeOf(left.since) - timeOf(right.since));
}

export function attentionFromSnapshot(snapshot: AttentionSnapshot): Attention {
  return { waiting: snapshot.waiting, history: [...snapshot.history].reverse() };
}

export function attentionRaised(attention: Attention, notice: AttentionNotice): Attention {
  const entry: AttentionWaiting = { target: notice.target, kind: notice.kind, since: notice.at };
  return {
    waiting: [...attention.waiting.filter((known) => !sameTarget(known.target, notice.target)), entry],
    history: [notice, ...attention.history],
  };
}

export function attentionLeft(attention: Attention, target: AttentionTarget): Attention {
  return { ...attention, waiting: attention.waiting.filter((known) => !sameTarget(known.target, target)) };
}

export function attentionCleared(attention: Attention): Attention {
  return { ...attention, history: [] };
}

export function countIn(waiting: readonly AttentionWaiting[], environment: string, terminals: readonly number[]): number {
  const held = new Set(terminals);
  return waiting.filter((entry) => entry.target.environment === environment && held.has(entry.target.terminal)).length;
}

export function spotsOf(views: Readonly<Record<RuntimeKey, { attention: Attention }>>): AttentionSpot[] {
  return oldestFirst(
    Object.entries(views).flatMap(([runtime, view]) =>
      view.attention.waiting.map((entry) => ({ runtime, target: entry.target, kind: entry.kind, since: entry.since })),
    ),
  );
}

function isAt(spot: AttentionSpot, place: SpotPlace): boolean {
  return sameTarget(spot.target, place) && (place.runtime === undefined || spot.runtime === place.runtime);
}

export function nextSpot(spots: readonly AttentionSpot[], current?: SpotPlace): AttentionSpot | undefined {
  const ordered = oldestFirst(spots);
  const first = ordered[0];
  if (!current || !first) return first;
  const at = ordered.findIndex((spot) => isAt(spot, current));
  if (at < 0) return first;
  return ordered[(at + 1) % ordered.length] ?? first;
}
