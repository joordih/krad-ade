import { describe, expect, it } from "vitest";
import type { AttentionKind } from "../protocol/AttentionKind";
import type { AttentionNotice } from "../protocol/AttentionNotice";
import type { AttentionWaiting } from "../protocol/AttentionWaiting";
import {
  attentionCleared,
  attentionFromSnapshot,
  attentionLeft,
  attentionRaised,
  countIn,
  EMPTY_ATTENTION,
  nextSpot,
  spotsOf,
  type Attention,
} from "./attention";

const target = (terminal: number) => ({ environment: "main", terminal });
const at = (minute: number) => `2026-11-16T10:${String(minute).padStart(2, "0")}:00Z`;

function notice(terminal: number, kind: AttentionKind, minute: number): AttentionNotice {
  return { target: target(terminal), kind, title: `agent ${terminal}`, at: at(minute) };
}

function waiting(terminal: number, kind: AttentionKind, minute: number): AttentionWaiting {
  return { target: target(terminal), kind, since: at(minute) };
}

describe("attention state", () => {
  it("replaces a raised target in the queue and keeps the newest history first", () => {
    let attention: Attention = EMPTY_ATTENTION;
    attention = attentionRaised(attention, notice(1, "ask", 1));
    attention = attentionRaised(attention, notice(1, "permission", 2));
    expect(attention.waiting).toHaveLength(1);
    expect(attention.waiting[0]?.kind).toBe("permission");
    expect(attention.history.map((entry) => entry.kind)).toEqual(["permission", "ask"]);
  });

  it("forgets a target that stopped waiting and clears only the log", () => {
    const started = attentionRaised(attentionRaised(EMPTY_ATTENTION, notice(1, "ask", 1)), notice(2, "error", 2));
    const left = attentionLeft(started, target(1));
    expect(left.waiting.map((entry) => entry.target.terminal)).toEqual([2]);
    expect(left.history).toHaveLength(2);
    expect(attentionCleared(left).history).toHaveLength(0);
    expect(attentionCleared(left).waiting).toHaveLength(1);
  });

  it("reads a snapshot's history newest first", () => {
    const attention = attentionFromSnapshot({
      waiting: [waiting(3, "finished", 3)],
      history: [notice(1, "ask", 1), notice(2, "error", 2)],
    });
    expect(attention.waiting[0]?.target.terminal).toBe(3);
    expect(attention.history.map((entry) => entry.target.terminal)).toEqual([2, 1]);
  });

  it("walks one runtime's queue oldest first and wraps", () => {
    const spots = spotsOf({ local: { attention: { ...EMPTY_ATTENTION, waiting: [waiting(2, "ask", 2), waiting(1, "permission", 1), waiting(3, "error", 3)] } } });
    expect(spots.map((spot) => spot.target.terminal)).toEqual([1, 2, 3]);
    expect(nextSpot(spots, target(2))?.target.terminal).toBe(3);
    expect(nextSpot(spots, target(3))?.target.terminal).toBe(1);
    expect(nextSpot(spots)?.target.terminal).toBe(1);
    expect(nextSpot(spots, target(9))?.target.terminal).toBe(1);
  });

  it("counts the queue of one session's terminals", () => {
    const queue = [waiting(1, "ask", 1), waiting(2, "error", 2), waiting(3, "finished", 3)];
    expect(countIn(queue, "main", [1, 3])).toBe(2);
    expect(countIn(queue, "other", [1, 3])).toBe(0);
    expect(countIn(queue, "main", [])).toBe(0);
  });

  it("jumps across runtimes and wraps", () => {
    const views = {
      local: { attention: { ...EMPTY_ATTENTION, waiting: [waiting(2, "ask", 2)] } },
      "runner:1": { attention: { ...EMPTY_ATTENTION, waiting: [waiting(1, "error", 1)] } },
    };
    const spots = spotsOf(views);
    expect(spots.map((spot) => spot.runtime)).toEqual(["runner:1", "local"]);
    expect(nextSpot(spots)?.target.terminal).toBe(1);
    expect(nextSpot(spots, { environment: "main", terminal: 1 })?.target.terminal).toBe(2);
    expect(nextSpot(spots, { environment: "main", terminal: 2 })?.target.terminal).toBe(1);
    expect(nextSpot([])).toBeUndefined();
  });
});
